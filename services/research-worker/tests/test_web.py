from __future__ import annotations

import xml.etree.ElementTree as ET

from chengsi_worker.web import _item, fetch_page


def test_fetch_page_rejects_non_http_url() -> None:
    result = fetch_page({"url": "file:///etc/passwd"})
    assert result["ok"] is False
    assert "http" in result["message"]


def test_parses_rss_item() -> None:
    item = ET.fromstring(
        "<item><title>标题</title><link>https://example.com/a</link>"
        "<description>摘要</description><pubDate>Wed, 29 Oct 2025 08:00:00 GMT</pubDate></item>"
    )
    parsed = _item(item, atom=False)
    assert parsed["title"] == "标题"
    assert parsed["url"] == "https://example.com/a"
    assert "摘要" in parsed["summary"]
