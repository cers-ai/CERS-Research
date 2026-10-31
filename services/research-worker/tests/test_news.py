from __future__ import annotations

from chengsi_worker.news import (
    NewsConfig,
    _deduplicate,
    _news_record,
    _parse_feed,
    search_news,
)

RSS = """<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>科技</title>
<item>
  <title>城市绿地与高温研究进展</title>
  <link>https://example.com/a</link>
  <description>研究团队发布新的观测结果。</description>
  <pubDate>Wed, 29 Oct 2025 08:00:00 GMT</pubDate>
</item>
<item>
  <title>无关的娱乐新闻</title>
  <link>https://example.com/b</link>
  <description>与检索主题无关。</description>
</item>
</channel></rss>
"""

ATOM = """<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<entry>
  <title>Atom 条目</title>
  <link href="https://example.com/atom"/>
  <summary>摘要内容</summary>
  <updated>2025-10-30T10:00:00Z</updated>
</entry>
</feed>
"""


def test_parses_rss_and_atom() -> None:
    feed = _parse_feed(RSS.encode())
    assert len(feed) == 2
    assert feed[0]["title"] == "城市绿地与高温研究进展"
    assert feed[0]["url"] == "https://example.com/a"
    assert feed[0]["publishedAt"].startswith("Wed, 29 Oct 2025")

    atom = _parse_feed(ATOM.encode())
    assert len(atom) == 1
    assert atom[0]["title"] == "Atom 条目"
    assert atom[0]["url"] == "https://example.com/atom"


def test_news_record_marks_metadata_only() -> None:
    config = NewsConfig(id="xinhua", name="新华网科技", driver="rss", url="https://example.com")
    record = _news_record(
        config,
        title="标题",
        url="https://example.com/a",
        summary="摘要",
        published_at="2025-10-30T10:00:00Z",
        retrieved_at="2026-10-31T00:00:00Z",
    )
    assert record["coverage"] == "metadata_only"
    assert record["type"] == "news"
    assert record["year"] == 2025
    assert record["authors"] == []


def test_deduplicates_news_by_url() -> None:
    config = NewsConfig(id="a", name="来源 A", driver="rss", url="https://example.com")
    first = _news_record(
        config,
        title="同一事件",
        url="https://example.com/a",
        summary="",
        published_at=None,
        retrieved_at="2026-10-31T00:00:00Z",
    )
    other = NewsConfig(id="b", name="来源 B", driver="rss", url="https://example.com")
    second = _news_record(
        other,
        title="同一事件",
        url="https://example.com/a",
        summary="",
        published_at=None,
        retrieved_at="2026-10-31T00:00:00Z",
    )
    merged = _deduplicate([first, second])
    assert len(merged) == 1
    assert merged[0]["alsoFoundIn"] == ["来源 B"]


def test_search_news_without_sources() -> None:
    result = search_news({"query": "绿地", "sources": []})
    assert result["ok"] is False
    assert "新闻来源" in result["message"]
