"""Bundled web fetch and RSS reading tools (no external runtime)."""

from __future__ import annotations

import html
import re
import xml.etree.ElementTree as ET
from typing import Any

import httpx

USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0 Safari/537.36 CERS-Academic/0.2"
)
MAX_BYTES = 2_000_000
DEFAULT_CHARS = 6_000


def fetch_page(payload: dict[str, Any]) -> dict[str, Any]:
    url = str(payload.get("url") or "").strip()
    if not url.startswith(("http://", "https://")):
        return {"ok": False, "message": "url 必须是 http 或 https 地址。"}
    max_chars = _clamp(payload.get("maxChars"), 500, 20_000, DEFAULT_CHARS)
    try:
        with httpx.Client(
            timeout=25,
            follow_redirects=True,
            trust_env=True,
            headers={
                "User-Agent": USER_AGENT,
                "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
                "Referer": "https://www.google.com/",
            },
        ) as client:
            response = client.get(url)
            response.raise_for_status()
    except httpx.HTTPError as exc:
        return {"ok": False, "message": f"读取网页失败：{exc}"}

    content_type = response.headers.get("content-type", "").split(";")[0].strip().lower()
    text = response.text
    is_html = (
        content_type in ("text/html", "application/xhtml+xml")
        or "<html" in text[:500].lower()
    )
    if is_html:
        title_match = re.search(r"<title[^>]*>(.*?)</title>", text, re.I | re.S)
        title = (
            html.unescape(re.sub(r"\s+", " ", title_match.group(1)).strip())
            if title_match
            else ""
        )
        body = re.sub(r"<(script|style|noscript)[^>]*>.*?</\1>", " ", text, flags=re.I | re.S)
        body = re.sub(r"<[^>]+>", "\n", body)
        body = html.unescape(body)
        body = re.sub(r"[ \t]+", " ", body)
        body = re.sub(r"\n[ \t]*\n+", "\n\n", body).strip()
    else:
        title = ""
        body = text
    return {
        "ok": True,
        "url": url,
        "title": title,
        "contentType": content_type,
        "chars": len(body),
        "text": body[:max_chars],
        "truncated": len(body) > max_chars,
    }


def list_feed(payload: dict[str, Any]) -> dict[str, Any]:
    url = str(payload.get("url") or "").strip()
    if not url.startswith(("http://", "https://")):
        return {"ok": False, "message": "url 必须是 http 或 https 地址。"}
    limit = _clamp(payload.get("limit"), 1, 30, 10)
    try:
        with httpx.Client(
            timeout=25,
            follow_redirects=True,
            trust_env=True,
            headers={"User-Agent": USER_AGENT},
        ) as client:
            response = client.get(url)
            response.raise_for_status()
        root = ET.fromstring(response.content)
    except (httpx.HTTPError, ET.ParseError) as exc:
        return {"ok": False, "message": f"读取订阅失败：{exc}"}
    items = []
    if _local(root.tag) == "feed":
        entries = [child for child in root if _local(child.tag) == "entry"]
    else:
        entries = [item for item in root.iter() if _local(item.tag) == "item"]
    for entry in entries[:limit]:
        items.append(_item(entry, atom=_local(root.tag) == "feed"))
    return {"ok": True, "url": url, "items": items}


def _item(element: ET.Element, atom: bool) -> dict[str, Any]:
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
        "summary": re.sub(
            r"<[^>]+>",
            " ",
            text("summary") or text("description") or text("content"),
        )[:500],
        "publishedAt": text("updated") or text("pubDate") or text("published"),
    }


def _local(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].lower()


def _clamp(value: Any, low: int, high: int, default: int) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError):
        return default
    return max(low, min(high, number))
