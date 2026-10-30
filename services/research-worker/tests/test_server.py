from __future__ import annotations

import json
import threading
import urllib.error
import urllib.request

import pytest

from chengsi_worker.server import chat_endpoint, create_server

TOKEN = "test-token-1234567890"


def _request(
    port: int,
    path: str,
    *,
    token: str | None = TOKEN,
    method: str = "GET",
) -> urllib.request.addinfourl:
    headers = {}
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    data = b"" if method == "POST" else None
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        headers=headers,
        data=data,
        method=method,
    )
    return urllib.request.urlopen(request, timeout=2)


def test_binds_loopback_only() -> None:
    server = create_server(TOKEN)
    try:
        assert server.server_address[0] == "127.0.0.1"
    finally:
        server.server_close()


def test_health_requires_token_and_shutdown_stops_server() -> None:
    server = create_server(TOKEN)
    port = int(server.server_address[1])
    thread = threading.Thread(
        target=server.serve_forever,
        kwargs={"poll_interval": 0.05},
        daemon=True,
    )
    thread.start()

    try:
        with _request(port, "/health") as response:
            payload = json.load(response)
        assert payload["ok"] is True
        assert payload["contractVersion"] == 1
        assert payload["python"].startswith("3.13.")

        with pytest.raises(urllib.error.HTTPError) as error:
            _request(port, "/health", token=None)
        assert error.value.code == 401

        with _request(port, "/shutdown", method="POST") as response:
            assert json.load(response)["ok"] is True

        thread.join(timeout=3)
        assert not thread.is_alive()
    finally:
        server.server_close()


def test_chat_endpoint_building() -> None:
    assert (
        chat_endpoint("https://api.deepseek.com", "openai_compatible")
        == "https://api.deepseek.com/chat/completions"
    )
    assert (
        chat_endpoint("http://127.0.0.1:11434", "ollama")
        == "http://127.0.0.1:11434/v1/chat/completions"
    )


def test_chat_rejects_missing_model_and_messages() -> None:
    server = create_server(TOKEN)
    port = int(server.server_address[1])
    thread = threading.Thread(
        target=server.serve_forever,
        kwargs={"poll_interval": 0.05},
        daemon=True,
    )
    thread.start()

    try:
        request = urllib.request.Request(
            f"http://127.0.0.1:{port}/chat",
            data=json.dumps({}).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {TOKEN}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=2) as response:
            payload = json.load(response)
        assert payload["ok"] is False
        assert "模型" in payload["message"]
    finally:
        server.shutdown()
        thread.join(timeout=3)
        server.server_close()
