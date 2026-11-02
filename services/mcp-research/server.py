"""CERS Academic MCP 研究工具服务（stdio JSON-RPC）。

暴露两个低风险工具：
- fetch_web_page：读取公开网页并提取标题与正文；
- list_feed：读取公开 RSS/Atom 订阅条目。

只访问用户明确给出的 URL，不执行任意代码，不写入本地文件。
"""

from __future__ import annotations

import json
import re
import sys
import xml.etree.ElementTree as ET
from html import unescape
from html.parser import HTMLParser
from typing import Any

import httpx

USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0 Safari/537.36 CERS-Academic/0.1"
)
MAX_BYTES = 2_000_000
DEFAULT_CHARS = 6_000


class _TextExtractor(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.title = ""
        self._in_title = False
        self._skip = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "title":
            self._in_title = True
        if tag in ("script", "style", "noscript"):
            self._skip += 1
        if tag in ("p", "br", "div", "li", "tr", "h1", "h2", "h3", "h4", "section"):
            self.parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag == "title":
            self._in_title = False
        if tag in ("script", "style", "noscript") and self._skip:
            self._skip -= 1

    def handle_data(self, data: str) -> None:
        if self._skip:
            return
        if self._in_title:
            self.title += data
            return
        self.parts.append(data)

    def text(self) -> str:
        raw = "".join(self.parts)
        raw = unescape(raw)
        raw = re.sub(r"\r", "", raw)
        raw = re.sub(r"[ \t]+", " ", raw)
        raw = re.sub(r"\n[ \t]*\n+", "\n\n", raw)
        return raw.strip()


def _fetch(url: str) -> tuple[str, bytes, str]:
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        "Referer": "https://www.google.com/",
    }
    with httpx.Client(
        timeout=25,
        follow_redirects=True,
        headers=headers,
        trust_env=True,
    ) as client:
        response = client.get(url)
        response.raise_for_status()
        content_type = response.headers.get("content-type", "").split(";")[0].strip().lower()
        body = response.content[:MAX_BYTES]
        charset = response.encoding or "utf-8"
        return content_type, body, charset


def fetch_web_page(arguments: dict[str, Any]) -> str:
    url = str(arguments.get("url") or "").strip()
    if not url.startswith(("http://", "https://")):
        raise ValueError("url 必须是 http 或 https 地址。")
    max_chars = _clamp_int(arguments.get("maxChars"), 500, 20_000, DEFAULT_CHARS)
    content_type, body, charset = _fetch(url)
    if content_type in ("text/html", "application/xhtml+xml"):
        parser = _TextExtractor()
        parser.feed(body.decode(charset, errors="replace"))
        text = parser.text()
        title = re.sub(r"\s+", " ", parser.title).strip()
    else:
        text = body.decode(charset, errors="replace")
        title = ""
    return json.dumps(
        {
            "url": url,
            "title": title,
            "contentType": content_type,
            "chars": len(text),
            "text": text[:max_chars],
            "truncated": len(text) > max_chars,
        },
        ensure_ascii=False,
    )


def list_feed(arguments: dict[str, Any]) -> str:
    url = str(arguments.get("url") or "").strip()
    if not url.startswith(("http://", "https://")):
        raise ValueError("url 必须是 http 或 https 地址。")
    limit = _clamp_int(arguments.get("limit"), 1, 30, 10)
    _, body, _ = _fetch(url)
    root = ET.fromstring(body)
    items: list[dict[str, Any]] = []
    if _local(root.tag) == "feed":
        for entry in root:
            if _local(entry.tag) != "entry":
                continue
            items.append(_feed_item(entry, atom=True))
            if len(items) >= limit:
                break
    else:
        for item in root.iter():
            if _local(item.tag) != "item":
                continue
            items.append(_feed_item(item, atom=False))
            if len(items) >= limit:
                break
    return json.dumps({"url": url, "items": items}, ensure_ascii=False)


def _feed_item(element: ET.Element, atom: bool) -> dict[str, Any]:
    def text(name: str) -> str:
        for child in element:
            if _local(child.tag) == name:
                value = "".join(child.itertext()).strip()
                if value:
                    return value
        return ""

    link = text("link")
    if atom and not link:
        for child in element:
            if _local(child.tag) == "link" and child.get("href"):
                link = str(child.get("href"))
                break
    return {
        "title": text("title"),
        "url": link or text("guid"),
        "summary": _strip(text("summary") or text("description") or text("content"))[:500],
        "publishedAt": text("updated") or text("pubDate") or text("published"),
    }


def _strip(value: str) -> str:
    text = re.sub(r"<[^>]+>", " ", value)
    return re.sub(r"\s+", " ", unescape(text)).strip()


def _local(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].lower()


def _clamp_int(value: Any, low: int, high: int, default: int) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError):
        return default
    return max(low, min(high, number))


TOOLS = [
    {
        "name": "fetch_web_page",
        "description": (
            "读取公开网页并提取标题与正文。适合核实监管文件、新闻原文和企业公告；"
            "只返回文本，不执行网页代码。"
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "http 或 https 网页地址"},
                "maxChars": {"type": "integer", "description": "最多返回的字符数"},
            },
            "required": ["url"],
        },
    },
    {
        "name": "list_feed",
        "description": "读取公开 RSS/Atom 订阅，返回标题、链接、摘要和发布时间。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "RSS/Atom 地址"},
                "limit": {"type": "integer", "description": "最多返回的条目数"},
            },
            "required": ["url"],
        },
    },
    {
        "name": "strip_html",
        "description": "把 HTML 片段转换为纯文本，用于本地验证工具调用链。",
        "inputSchema": {
            "type": "object",
            "properties": {"html": {"type": "string"}},
            "required": ["html"],
        },
    },
]


def handle_call(name: str, arguments: dict[str, Any]) -> str:
    if name == "fetch_web_page":
        return fetch_web_page(arguments)
    if name == "list_feed":
        return list_feed(arguments)
    if name == "strip_html":
        return _strip(str(arguments.get("html") or ""))
    raise ValueError(f"未知工具：{name}")


def _respond(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def main() -> None:
    while True:
        raw = sys.stdin.readline()
        if not raw:
            break
        line = raw.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = message.get("method")
        request_id = message.get("id")
        if request_id is None:
            continue
        if method == "initialize":
            _respond(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "protocolVersion": message.get("params", {}).get(
                            "protocolVersion", "2024-11-05"
                        ),
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "cers-research-tools", "version": "0.1.0"},
                    },
                }
            )
        elif method == "tools/list":
            _respond({"jsonrpc": "2.0", "id": request_id, "result": {"tools": TOOLS}})
        elif method == "tools/call":
            params = message.get("params") or {}
            name = str(params.get("name") or "")
            arguments = params.get("arguments")
            if not isinstance(arguments, dict):
                arguments = {}
            try:
                text = handle_call(name, arguments)
                _respond(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": {"content": [{"type": "text", "text": text}]},
                    }
                )
            except Exception as exc:  # noqa: BLE001 - 工具错误需要返回给调用方
                _respond(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": {
                            "isError": True,
                            "content": [{"type": "text", "text": f"{type(exc).__name__}: {exc}"}],
                        },
                    }
                )
        elif method == "ping":
            _respond({"jsonrpc": "2.0", "id": request_id, "result": {}})
        else:
            _respond(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32601, "message": f"不支持的方法：{method}"},
                }
            )


if __name__ == "__main__":
    main()
