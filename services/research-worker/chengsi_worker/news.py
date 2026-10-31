"""新闻与资讯来源检索（M2）。

只读取 RSS/Atom 与新闻检索服务返回的公开元数据；不抓取正文，
也不把转载数量当作独立证实。新闻条目统一标记为仅题录。
"""

from __future__ import annotations

import hashlib
import re
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import UTC, datetime
from email.utils import parsedate_to_datetime
from time import perf_counter
from typing import Any

import httpx

DEFAULT_MAX_RESULTS = 12
MAX_RESULTS = 30
DEFAULT_TIMEOUT = 20.0
MAX_SUMMARY_CHARS = 600
USER_AGENT = "ChengsiAcademic/0.1"


@dataclass
class NewsConfig:
    id: str
    name: str
    driver: str
    url: str
    api_key: str | None = None
    language: str | None = None


def search_news(payload: dict[str, Any]) -> dict[str, Any]:
    query = str(payload.get("query") or "").strip()
    configs = _configs(payload.get("sources"))
    if not configs:
        return {"ok": False, "message": "没有启用的新闻来源，请先在设置中启用。"}

    max_results = _clamp_int(payload.get("maxResults"), 1, MAX_RESULTS, DEFAULT_MAX_RESULTS)
    network = payload.get("network") if isinstance(payload.get("network"), dict) else {}
    proxy = _optional_str(network.get("proxyUrl")) if network else None
    timeout = _clamp_float(
        network.get("timeoutSeconds") if network else None, 5.0, 60.0, DEFAULT_TIMEOUT
    )
    retrieved_at = _now_iso()

    outcomes: dict[str, tuple[list[dict[str, Any]], str | None, int]] = {}
    with ThreadPoolExecutor(max_workers=min(6, len(configs))) as executor:
        futures = {
            executor.submit(
                _search_one,
                config,
                query,
                max_results,
                proxy,
                timeout,
                retrieved_at,
            ): config.id
            for config in configs
        }
        for future in as_completed(futures):
            source_id = futures[future]
            outcomes[source_id] = future.result()

    statuses: list[dict[str, Any]] = []
    records: list[dict[str, Any]] = []
    for config in configs:
        items, error, latency = outcomes.get(config.id, ([], "检索未执行。", 0))
        statuses.append(
            {
                "id": config.id,
                "name": config.name,
                "ok": error is None,
                "count": len(items),
                "error": error,
                "latencyMs": latency,
            }
        )
        records.extend(items)

    deduped = _deduplicate(records)
    deduped.sort(key=_sort_key)

    return {
        "ok": any(status["ok"] for status in statuses),
        "query": query,
        "results": deduped,
        "sourceStatuses": statuses,
        "retrievedAt": retrieved_at,
        "message": None if deduped else "没有检索到资讯。",
    }


def _search_one(
    config: NewsConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> tuple[list[dict[str, Any]], str | None, int]:
    started = perf_counter()
    try:
        with httpx.Client(
            timeout=timeout,
            follow_redirects=True,
            headers={"User-Agent": USER_AGENT},
            proxy=proxy or None,
        ) as client:
            if config.driver == "rss":
                items = _search_rss(client, config, query, limit, retrieved_at)
            elif config.driver == "serpapi":
                items = _search_serpapi(client, config, query, limit, retrieved_at)
            elif config.driver == "newsapi":
                items = _search_newsapi(client, config, query, limit, retrieved_at)
            else:
                raise ValueError(f"不支持的新闻来源驱动：{config.driver}")
    except Exception as exc:  # noqa: BLE001 - 单一来源失败不影响其他来源
        return [], _describe_error(exc), int((perf_counter() - started) * 1000)
    return items[:limit], None, int((perf_counter() - started) * 1000)


def _search_rss(
    client: httpx.Client,
    config: NewsConfig,
    query: str,
    limit: int,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    response = client.get(config.url)
    response.raise_for_status()
    entries = _parse_feed(response.content)
    terms = _query_terms(query)
    items: list[dict[str, Any]] = []
    for entry in entries:
        title = entry["title"]
        summary = entry["summary"]
        if terms and not _matches(f"{title} {summary}", terms):
            continue
        items.append(
            _news_record(
                config,
                title=title,
                url=entry["url"],
                summary=summary,
                published_at=entry["publishedAt"],
                retrieved_at=retrieved_at,
            )
        )
        if len(items) >= limit:
            break
    return items


def _search_serpapi(
    client: httpx.Client,
    config: NewsConfig,
    query: str,
    limit: int,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    if not config.api_key:
        raise ValueError("缺少 API Key。")
    if not query:
        raise ValueError("SerpAPI 新闻检索需要关键词。")
    response = client.get(
        config.url,
        params={
            "engine": "google_news",
            "q": query,
            "api_key": config.api_key,
            "hl": "zh-cn",
            "gl": "cn",
        },
    )
    response.raise_for_status()
    payload = response.json()
    if payload.get("error"):
        raise ValueError(str(payload["error"]))
    items: list[dict[str, Any]] = []
    for row in payload.get("news_results") or []:
        if not isinstance(row, dict):
            continue
        items.append(
            _news_record(
                config,
                title=_clean_text(str(row.get("title") or "")),
                url=_optional_str(row.get("link")),
                summary=_clean_text(str(row.get("snippet") or "")),
                published_at=_parse_date(row.get("date")),
                retrieved_at=retrieved_at,
            )
        )
        if len(items) >= limit:
            break
    return items


def _search_newsapi(
    client: httpx.Client,
    config: NewsConfig,
    query: str,
    limit: int,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    if not config.api_key:
        raise ValueError("缺少 API Key。")
    if not query:
        raise ValueError("NewsAPI 检索需要关键词。")
    response = client.get(
        config.url,
        params={
            "q": query,
            "apiKey": config.api_key,
            "pageSize": limit,
            "sortBy": "publishedAt",
            "language": config.language or "zh",
        },
    )
    response.raise_for_status()
    payload = response.json()
    if payload.get("status") == "error":
        raise ValueError(str(payload.get("message") or "NewsAPI 返回错误。"))
    items: list[dict[str, Any]] = []
    for row in payload.get("articles") or []:
        if not isinstance(row, dict):
            continue
        raw_source = row.get("source")
        source: dict[str, Any] = raw_source if isinstance(raw_source, dict) else {}
        items.append(
            _news_record(
                config,
                title=_clean_text(str(row.get("title") or "")),
                url=_optional_str(row.get("url")),
                summary=_clean_text(str(row.get("description") or "")),
                published_at=_parse_date(row.get("publishedAt")),
                retrieved_at=retrieved_at,
                venue=_optional_str(source.get("name")),
            )
        )
        if len(items) >= limit:
            break
    return items


def _parse_feed(content: bytes) -> list[dict[str, Any]]:
    try:
        root = ET.fromstring(content)
    except ET.ParseError as exc:
        raise ValueError(f"订阅内容不是有效的 XML：{exc}") from exc

    entries: list[dict[str, Any]] = []
    if _local_name(root.tag) == "feed":
        for entry in _children(root, "entry"):
            title = _child_text(entry, "title")
            summary = _child_text(entry, "summary") or _child_text(entry, "content")
            entries.append(
                {
                    "title": title,
                    "url": _entry_link(entry),
                    "summary": summary,
                    "publishedAt": _child_text(entry, "updated")
                    or _child_text(entry, "published"),
                }
            )
        return entries

    for item in root.iter():
        if _local_name(item.tag) != "item":
            continue
        entries.append(
            {
                "title": _child_text(item, "title"),
                "url": _child_text(item, "link") or _child_text(item, "guid"),
                "summary": _child_text(item, "description") or _child_text(item, "summary"),
                "publishedAt": _child_text(item, "pubDate") or _child_text(item, "date"),
            }
        )
    return entries


def _news_record(
    config: NewsConfig,
    *,
    title: str,
    url: str | None,
    summary: str,
    published_at: str | None,
    retrieved_at: str,
    venue: str | None = None,
) -> dict[str, Any]:
    title = title.strip()
    digest = hashlib.sha1(f"{config.id}|{url or title}".encode()).hexdigest()[:16]
    published = _parse_date(published_at)
    year = None
    if published:
        try:
            year = datetime.fromisoformat(published.replace("Z", "+00:00")).year
        except ValueError:
            year = None
    return {
        "id": f"news_{digest}",
        "externalId": None,
        "provider": config.id,
        "providerName": config.name,
        "title": title or "未命名资讯",
        "authors": [],
        "year": year,
        "venue": venue or config.name,
        "doi": None,
        "url": url,
        "abstract": summary[:MAX_SUMMARY_CHARS] or None,
        "coverage": "metadata_only",
        "type": "news",
        "publishedAt": published,
        "alsoFoundIn": [],
        "retrievedAt": retrieved_at,
    }


def _deduplicate(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: dict[str, dict[str, Any]] = {}
    order: list[str] = []
    for record in records:
        key = _dedupe_key(record)
        if key in seen:
            existing = seen[key]
            names = existing.setdefault("alsoFoundIn", [])
            provider = record["providerName"]
            if provider not in names and provider != existing["providerName"]:
                names.append(provider)
            continue
        seen[key] = record
        order.append(key)
    return [seen[key] for key in order]


def _dedupe_key(record: dict[str, Any]) -> str:
    url = str(record.get("url") or "").strip().lower()
    if url:
        return f"url:{url}"
    title = re.sub(r"\W+", "", str(record.get("title") or "").lower())
    return f"title:{title}"


def _sort_key(record: dict[str, Any]) -> tuple[str, str]:
    published = record.get("publishedAt") or ""
    return (str(published), str(record.get("title") or ""))


def _configs(raw: Any) -> list[NewsConfig]:
    if not isinstance(raw, list):
        return []
    configs: list[NewsConfig] = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        source_id = str(item.get("id") or "").strip()
        driver = str(item.get("driver") or "").strip()
        url = str(item.get("url") or "").strip()
        if not source_id or not driver or not url:
            continue
        configs.append(
            NewsConfig(
                id=source_id,
                name=str(item.get("name") or source_id),
                driver=driver,
                url=url,
                api_key=_optional_str(item.get("apiKey")),
                language=_optional_str(item.get("language")),
            )
        )
    return configs


def _query_terms(query: str) -> list[str]:
    return [term.lower() for term in re.split(r"\s+", query.strip()) if term]


def _matches(text: str, terms: list[str]) -> bool:
    lowered = text.lower()
    return any(term in lowered for term in terms)


def _entry_link(entry: ET.Element) -> str | None:
    for link in _children(entry, "link"):
        href = link.get("href")
        if href:
            return href
        if link.text:
            return link.text.strip()
    return None


def _children(element: ET.Element, name: str) -> list[ET.Element]:
    return [child for child in element if _local_name(child.tag) == name.lower()]


def _child_text(element: ET.Element, name: str) -> str:
    for child in _children(element, name):
        text = "".join(child.itertext()).strip()
        if text:
            return _clean_text(text)
    return ""


def _local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].lower()


def _clean_text(value: str) -> str:
    text = re.sub(r"<[^>]+>", " ", value)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def _parse_date(value: Any) -> str | None:
    text = _optional_str(value)
    if not text:
        return None
    try:
        parsed = parsedate_to_datetime(text)
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=UTC)
        return parsed.astimezone(UTC).isoformat().replace("+00:00", "Z")
    except (TypeError, ValueError, OverflowError):
        pass
    try:
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=UTC)
        return parsed.astimezone(UTC).isoformat().replace("+00:00", "Z")
    except ValueError:
        return None


def _now_iso() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def _optional_str(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _clamp_int(value: Any, low: int, high: int, default: int) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError):
        return default
    return max(low, min(high, number))


def _clamp_float(value: Any, low: float, high: float, default: float) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return default
    return max(low, min(high, number))


def _describe_error(exc: Exception) -> str:
    if isinstance(exc, httpx.TimeoutException):
        return "请求超时。"
    if isinstance(exc, httpx.HTTPStatusError):
        return f"来源返回 HTTP {exc.response.status_code}。"
    if isinstance(exc, httpx.RequestError):
        return f"网络请求失败：{type(exc).__name__}。"
    return str(exc) or type(exc).__name__
