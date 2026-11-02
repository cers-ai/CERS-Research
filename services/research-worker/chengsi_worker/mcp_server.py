"""Built-in stdio MCP server using the bundled web tools."""

from __future__ import annotations

import json
import sys
from typing import Any

from .web import fetch_page, list_feed

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
                "maxChars": {"type": "integer", "description": "最多返回字符数"},
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
                "limit": {"type": "integer", "description": "最多返回条目数"},
            },
            "required": ["url"],
        },
    },
]


def serve_mcp() -> int:
    while True:
        raw = sys.stdin.readline()
        if not raw:
            break
        try:
            message = json.loads(raw)
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
                        "protocolVersion": str(
                            (message.get("params") or {}).get(
                                "protocolVersion", "2024-11-05"
                            )
                        ),
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "cers-web-tools", "version": "0.2.0"},
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
                if name == "fetch_web_page":
                    result = fetch_page(arguments)
                elif name == "list_feed":
                    result = list_feed(arguments)
                else:
                    raise ValueError(f"未知工具：{name}")
                if not result.get("ok"):
                    raise ValueError(str(result.get("message") or "工具执行失败。"))
                _respond(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": json.dumps(result, ensure_ascii=False),
                                }
                            ]
                        },
                    }
                )
            except Exception as exc:  # noqa: BLE001 - MCP 需要把错误返回给调用方
                _respond(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": {
                            "isError": True,
                            "content": [
                                {"type": "text", "text": f"{type(exc).__name__}: {exc}"}
                            ],
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
    return 0


def _respond(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()
