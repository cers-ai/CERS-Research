"""本机回环 Sidecar 服务（M0）。

只监听 127.0.0.1；启动时由 Rust 注入一次性会话令牌。stdout 仅用于握手行，
日志统一写 stderr，避免污染 IPC 握手。
"""

from __future__ import annotations

import hmac
import json
import os
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

import httpx

from . import __version__
from .experiments import run_experiment
from .reports import export_report
from .sources import search_sources

CONTRACT_VERSION = 1
_BIND_HOST = "127.0.0.1"
_MAX_BODY_BYTES = 1_000_000


def _runtime_payload() -> dict[str, Any]:
    return {
        "ok": True,
        "python": ".".join(str(part) for part in sys.version_info[:3]),
        "version": __version__,
        "contractVersion": CONTRACT_VERSION,
        "pid": os.getpid(),
    }


class _Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "ChengsiSidecar/0.1"

    def do_GET(self) -> None:
        if not self._authorized():
            return
        if self.path == "/health":
            self._send_json(200, _runtime_payload())
        else:
            self._send_json(404, {"ok": False, "error": "not_found"})

    def do_POST(self) -> None:
        if not self._authorized():
            return
        if self.path == "/shutdown":
            self._send_json(200, {"ok": True})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return

        if self.path in (
            "/chat",
            "/sources/search",
            "/experiments/run",
            "/reports/export",
        ):
            payload = self._read_json_body()
            if payload is None:
                return
            if self.path == "/chat":
                self._send_json(200, _chat(payload))
            elif self.path == "/sources/search":
                self._send_json(200, search_sources(payload))
            elif self.path == "/experiments/run":
                self._send_json(200, run_experiment(payload))
            else:
                self._send_json(200, export_report(payload))
            return

        self._send_json(404, {"ok": False, "error": "not_found"})

    def _read_json_body(self) -> dict[str, Any] | None:
        length_header = self.headers.get("Content-Length", "0")
        try:
            length = int(length_header)
        except ValueError:
            self._send_json(400, {"ok": False, "message": "Content-Length 无效。"})
            return None
        if length <= 0 or length > _MAX_BODY_BYTES:
            self._send_json(413, {"ok": False, "message": "请求体为空或过大。"})
            return None
        try:
            payload = json.loads(self.rfile.read(length))
        except json.JSONDecodeError:
            self._send_json(400, {"ok": False, "message": "请求体不是有效 JSON。"})
            return None
        if not isinstance(payload, dict):
            self._send_json(400, {"ok": False, "message": "请求体必须是 JSON 对象。"})
            return None
        return payload

    def _authorized(self) -> bool:
        expected = f"Bearer {getattr(self.server, 'auth_token', '')}"
        actual = self.headers.get("Authorization", "")
        if hmac.compare_digest(actual, expected):
            return True
        self._send_json(401, {"ok": False, "error": "unauthorized"})
        return False

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        print(
            f"[chengsi-worker] {self.address_string()} {format % args}",
            file=sys.stderr,
            flush=True,
        )


class SidecarServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address: tuple[str, int], token: str) -> None:
        if not token:
            raise ValueError("Sidecar 会话令牌不能为空")
        self.auth_token = token
        super().__init__(address, _Handler)


def create_server(token: str, port: int = 0) -> SidecarServer:
    """创建仅绑定回环地址的服务；port=0 时由系统分配空闲端口。"""
    return SidecarServer((_BIND_HOST, port), token)


def serve(token: str) -> int:
    server = create_server(token)
    _, port = server.server_address[:2]
    handshake = {
        "port": port,
        "pid": os.getpid(),
        "python": ".".join(str(part) for part in sys.version_info[:3]),
        "version": __version__,
        "contractVersion": CONTRACT_VERSION,
    }
    print(json.dumps(handshake, ensure_ascii=False), flush=True)

    try:
        server.serve_forever(poll_interval=0.2)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


def chat_endpoint(base_url: str, protocol: str) -> str:
    base = base_url.rstrip("/")
    if base.endswith("/chat/completions"):
        return base
    if protocol == "ollama" and not base.endswith("/v1"):
        return f"{base}/v1/chat/completions"
    return f"{base}/chat/completions"


def _chat(payload: dict[str, Any]) -> dict[str, Any]:
    provider = payload.get("provider")
    messages = payload.get("messages")
    model_id = str(payload.get("modelId") or "").strip()

    if not isinstance(provider, dict) or not model_id:
        return {"ok": False, "message": "缺少模型服务或模型 ID。"}
    if not isinstance(messages, list) or not messages:
        return {"ok": False, "message": "缺少对话消息。"}

    base_url = str(provider.get("baseUrl") or "").strip()
    if not base_url.startswith(("http://", "https://")):
        return {"ok": False, "message": "模型服务 Base URL 无效。"}

    protocol = str(provider.get("protocol") or "openai_compatible")
    endpoint = chat_endpoint(base_url, protocol)
    api_key = payload.get("apiKey")
    timeout = float(provider.get("timeoutSeconds") or 90)
    proxy = provider.get("proxy") or None

    headers = {
        "Content-Type": "application/json",
        "User-Agent": "ChengsiAcademic/0.1",
    }
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"

    request_body = {
        "model": model_id,
        "messages": messages,
        "stream": False,
        "max_tokens": int(payload.get("maxTokens") or 2048),
        "temperature": float(payload.get("temperature") or 0.4),
    }

    try:
        with httpx.Client(timeout=timeout, proxy=proxy) as client:
            response = client.post(endpoint, headers=headers, json=request_body)
    except httpx.RequestError as exc:
        return {"ok": False, "message": f"网络请求失败：{exc}"}

    if response.status_code >= 400:
        return {
            "ok": False,
            "statusCode": response.status_code,
            "message": _response_error(response) or f"模型服务返回 HTTP {response.status_code}",
        }

    try:
        data = response.json()
    except ValueError:
        return {"ok": False, "message": "模型服务返回了非 JSON 响应。"}

    try:
        content = data["choices"][0]["message"]["content"] or ""
    except (KeyError, IndexError, TypeError):
        return {"ok": False, "message": "模型响应缺少 choices[0].message.content。"}

    return {
        "ok": True,
        "content": str(content),
        "model": str(data.get("model") or model_id),
        "usage": data.get("usage"),
    }


def _response_error(response: httpx.Response) -> str | None:
    try:
        data = response.json()
    except ValueError:
        return None
    error = data.get("error")
    if isinstance(error, dict):
        message = error.get("message")
        return str(message) if message else None
    if isinstance(error, str):
        return error
    return None
