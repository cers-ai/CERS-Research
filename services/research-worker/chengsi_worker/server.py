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

from . import __version__

CONTRACT_VERSION = 1
_BIND_HOST = "127.0.0.1"


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
        else:
            self._send_json(404, {"ok": False, "error": "not_found"})

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
