"""学术来源检索适配器（M2）。

只做检索、元数据归一化与去重；不下载全文，不把题录伪装成全文。
"""

from __future__ import annotations

import hashlib
import re
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import UTC, datetime
from time import perf_counter
from typing import Any

import httpx

DEFAULT_MAX_RESULTS = 5
MAX_RESULTS = 10
DEFAULT_TIMEOUT = 20.0
MAX_ABSTRACT_CHARS = 1_200
USER_AGENT = "ChengsiAcademic/0.1"

_COVERAGE_RANK = {"unavailable": 0, "metadata_only": 1, "abstract_only": 2, "fulltext": 3}
_DOI_PREFIXES = (
    "https://doi.org/",
    "http://doi.org/",
    "https://dx.doi.org/",
    "http://dx.doi.org/",
)


@dataclass
class SourceConfig:
    id: str
    name: str
    driver: str
    url: str
    api_key: str | None = None
    contact_email: str | None = None
    language: str | None = None


def search_sources(payload: dict[str, Any]) -> dict[str, Any]:
    query = str(payload.get("query") or "").strip()
    if not query:
        return {"ok": False, "message": "检索关键词不能为空。"}

    configs = _configs(payload.get("sources"))
    if not configs:
        return {"ok": False, "message": "没有启用的学术来源。"}

    max_results = _clamp_int(payload.get("maxResults"), 1, MAX_RESULTS, DEFAULT_MAX_RESULTS)
    year_from = _optional_int(payload.get("yearFrom"))
    year_to = _optional_int(payload.get("yearTo"))
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
                year_from,
                year_to,
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
        "message": None if deduped else "没有检索到结果。",
    }


def _search_one(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    year_from: int | None,
    year_to: int | None,
    retrieved_at: str,
) -> tuple[list[dict[str, Any]], str | None, int]:
    started = perf_counter()
    try:
        records = _run_adapter(config, query, limit, proxy, timeout, retrieved_at)
        if year_from is not None or year_to is not None:
            records = [
                record
                for record in records
                if _year_matches(record.get("year"), year_from, year_to)
            ]
        return records, None, int((perf_counter() - started) * 1000)
    except Exception as exc:  # noqa: BLE001 - 每个来源的失败需要独立归一化
        return [], _safe_error(exc), int((perf_counter() - started) * 1000)


def _run_adapter(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    if config.driver == "openalex":
        return _openalex(config, query, limit, proxy, timeout, retrieved_at)
    if config.driver == "crossref":
        return _crossref(config, query, limit, proxy, timeout, retrieved_at)
    if config.driver == "arxiv":
        return _arxiv(config, query, limit, proxy, timeout, retrieved_at)
    if config.driver == "semantic_scholar":
        return _semantic_scholar(config, query, limit, proxy, timeout, retrieved_at)
    if config.driver == "pubmed":
        return _pubmed(config, query, limit, proxy, timeout, retrieved_at)
    if config.driver == "europe_pmc":
        return _europe_pmc(config, query, limit, proxy, timeout, retrieved_at)
    raise ValueError(f"不支持的学术来源驱动：{config.driver}")


def _openalex(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    params: dict[str, Any] = {"search": query, "per-page": limit}
    if config.api_key:
        params["api_key"] = config.api_key
    if config.contact_email:
        params["mailto"] = config.contact_email
    data = _get_json(
        f"{config.url.rstrip('/')}/works",
        params,
        {"Accept": "application/json", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    items = data.get("results", []) if isinstance(data, dict) else []
    records: list[dict[str, Any]] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        doi = _normalize_doi(item.get("doi"))
        location = item.get("primary_location") or {}
        source = location.get("source") or {}
        authors = [
            authorship.get("author", {}).get("display_name")
            for authorship in item.get("authorships", [])
            if isinstance(authorship, dict) and isinstance(authorship.get("author"), dict)
        ]
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=str(item.get("id", "")).rsplit("/", 1)[-1],
                title=item.get("title") or "",
                authors=[name for name in authors if name],
                year=_optional_int(item.get("publication_year")),
                venue=source.get("display_name"),
                doi=doi,
                url=location.get("landing_page_url") or _doi_url(doi),
                abstract=_openalex_abstract(item.get("abstract_inverted_index")),
                type_=item.get("type") or "article",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _crossref(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    params: dict[str, Any] = {"query": query, "rows": limit}
    if config.contact_email:
        params["mailto"] = config.contact_email
    data = _get_json(
        f"{config.url.rstrip('/')}/works",
        params,
        {"Accept": "application/json", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    message = data.get("message", {}) if isinstance(data, dict) else {}
    items = message.get("items", []) if isinstance(message, dict) else []
    records: list[dict[str, Any]] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        doi = _normalize_doi(item.get("DOI"))
        authors = []
        for author in item.get("author", []):
            if not isinstance(author, dict):
                continue
            name = " ".join(
                part for part in (author.get("given"), author.get("family")) if part
            ) or author.get("name")
            if name:
                authors.append(str(name))
        year = None
        issued = item.get("issued") or {}
        parts = issued.get("date-parts") if isinstance(issued, dict) else None
        if isinstance(parts, list) and parts and isinstance(parts[0], list) and parts[0]:
            year = _optional_int(parts[0][0])
        containers = item.get("container-title") or []
        titles = item.get("title") or []
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=doi or str(item.get("URL") or ""),
                title=titles[0] if isinstance(titles, list) and titles else "",
                authors=authors,
                year=year,
                venue=containers[0] if isinstance(containers, list) and containers else None,
                doi=doi,
                url=item.get("URL") or _doi_url(doi),
                abstract=_strip_markup(item.get("abstract")),
                type_=item.get("type") or "article",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _arxiv(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    params = {
        "search_query": f"all:{query}",
        "start": 0,
        "max_results": limit,
        "sortBy": "relevance",
    }
    text = _get_text(
        config.url,
        params,
        {"Accept": "application/atom+xml", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    root = ET.fromstring(text)
    namespace = {"a": "http://www.w3.org/2005/Atom", "arxiv": "http://arxiv.org/schemas/atom"}
    records: list[dict[str, Any]] = []
    for entry in root.findall("a:entry", namespace):
        identifier = entry.findtext("a:id", default="", namespaces=namespace)
        title = entry.findtext("a:title", default="", namespaces=namespace)
        summary = entry.findtext("a:summary", default="", namespaces=namespace)
        published = entry.findtext("a:published", default="", namespaces=namespace)
        authors = [
            name
            for name in (
                author.findtext("a:name", default="", namespaces=namespace)
                for author in entry.findall("a:author", namespace)
            )
            if name
        ]
        doi_element = entry.find("arxiv:doi", namespace)
        doi = _normalize_doi(doi_element.text if doi_element is not None else None)
        alternative = identifier
        for link in entry.findall("a:link", namespace):
            if link.get("rel") == "alternate" and link.get("href"):
                alternative = link.get("href", alternative)
                break
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=identifier.rsplit("/abs/", 1)[-1] or identifier,
                title=title,
                authors=authors,
                year=_optional_int(published[:4]),
                venue="arXiv",
                doi=doi,
                url=alternative,
                abstract=summary,
                type_="preprint",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _semantic_scholar(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    headers = {"Accept": "application/json", "User-Agent": USER_AGENT}
    if config.api_key:
        headers["x-api-key"] = config.api_key
    data = _get_json(
        f"{config.url.rstrip('/')}/paper/search",
        {
            "query": query,
            "limit": limit,
            "fields": "title,authors,year,venue,abstract,externalIds,url,publicationTypes",
        },
        headers,
        proxy,
        timeout,
    )
    items = data.get("data", []) if isinstance(data, dict) else []
    records: list[dict[str, Any]] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        external = item.get("externalIds") or {}
        doi = _normalize_doi(external.get("DOI"))
        external_id = str(
            external.get("DOI")
            or external.get("ArXiv")
            or external.get("CorpusId")
            or item.get("paperId")
            or ""
        )
        authors = [
            author.get("name")
            for author in item.get("authors", [])
            if isinstance(author, dict) and author.get("name")
        ]
        types = item.get("publicationTypes") or []
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=external_id,
                title=item.get("title") or "",
                authors=[name for name in authors if name],
                year=_optional_int(item.get("year")),
                venue=item.get("venue"),
                doi=doi,
                url=item.get("url") or _doi_url(doi),
                abstract=item.get("abstract"),
                type_=types[0] if isinstance(types, list) and types else "article",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _pubmed(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    base = config.url.rstrip("/")
    common: dict[str, Any] = {"db": "pubmed", "retmode": "json", "tool": "chengsi"}
    if config.contact_email:
        common["email"] = config.contact_email
    if config.api_key:
        common["api_key"] = config.api_key

    search = _get_json(
        f"{base}/esearch.fcgi",
        {**common, "term": query, "retmax": limit, "sort": "relevance"},
        {"Accept": "application/json", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    result = search.get("esearchresult", {}) if isinstance(search, dict) else {}
    ids = result.get("idlist", []) if isinstance(result, dict) else []
    if not ids:
        return []

    summary = _get_json(
        f"{base}/esummary.fcgi",
        {**common, "id": ",".join(str(item) for item in ids)},
        {"Accept": "application/json", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    summary_result = summary.get("result", {}) if isinstance(summary, dict) else {}
    records: list[dict[str, Any]] = []
    for uid in ids:
        item = summary_result.get(str(uid), {}) if isinstance(summary_result, dict) else {}
        if not isinstance(item, dict):
            continue
        doi = None
        for article_id in item.get("articleids", []):
            if isinstance(article_id, dict) and article_id.get("idtype") == "doi":
                doi = _normalize_doi(article_id.get("value"))
                break
        authors = [
            author.get("name")
            for author in item.get("authors", [])
            if isinstance(author, dict) and author.get("name")
        ]
        pubdate = str(item.get("pubdate") or "")
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=str(uid),
                title=item.get("title") or "",
                authors=[name for name in authors if name],
                year=_optional_int(pubdate[:4]),
                venue=item.get("fulljournalname") or item.get("source"),
                doi=doi,
                url=f"https://pubmed.ncbi.nlm.nih.gov/{uid}/",
                abstract=None,
                type_="article",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _europe_pmc(
    config: SourceConfig,
    query: str,
    limit: int,
    proxy: str | None,
    timeout: float,
    retrieved_at: str,
) -> list[dict[str, Any]]:
    data = _get_json(
        f"{config.url.rstrip('/')}/search",
        {
            "query": query,
            "format": "json",
            "pageSize": limit,
            "resultType": "core",
        },
        {"Accept": "application/json", "User-Agent": USER_AGENT},
        proxy,
        timeout,
    )
    result_list = data.get("resultList", {}) if isinstance(data, dict) else {}
    items = result_list.get("result", []) if isinstance(result_list, dict) else []
    records: list[dict[str, Any]] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        doi = _normalize_doi(item.get("doi"))
        source = str(item.get("source") or "MED")
        identifier = str(item.get("id") or "")
        authors = [
            name.strip() for name in str(item.get("authorString") or "").split(",") if name.strip()
        ]
        records.append(
            _record(
                provider=config.id,
                provider_name=config.name,
                external_id=identifier,
                title=item.get("title") or "",
                authors=authors,
                year=_optional_int(item.get("pubYear")),
                venue=item.get("journalTitle"),
                doi=doi,
                url=f"https://europepmc.org/article/{source}/{identifier}",
                abstract=item.get("abstractText"),
                type_=item.get("pubType") or "article",
                retrieved_at=retrieved_at,
            )
        )
    return records


def _record(
    *,
    provider: str,
    provider_name: str,
    external_id: str,
    title: str,
    authors: list[str],
    year: int | None,
    venue: str | None,
    doi: str | None,
    url: str | None,
    abstract: str | None,
    type_: str,
    retrieved_at: str,
) -> dict[str, Any]:
    cleaned_title = _clean_text(title)
    cleaned_abstract = _strip_markup(abstract)
    if cleaned_abstract:
        cleaned_abstract = cleaned_abstract[:MAX_ABSTRACT_CHARS]
    coverage = "abstract_only" if cleaned_abstract else "metadata_only"
    identity = f"{provider}|{external_id or doi or cleaned_title}"
    digest = hashlib.sha1(identity.encode("utf-8")).hexdigest()[:16]

    return {
        "id": f"src_{digest}",
        "externalId": external_id or None,
        "provider": provider,
        "providerName": provider_name,
        "title": cleaned_title,
        "authors": authors[:20],
        "year": year,
        "venue": _clean_text(venue) or None,
        "doi": doi,
        "url": url,
        "abstract": cleaned_abstract or None,
        "coverage": coverage,
        "type": type_,
        "alsoFoundIn": [],
        "retrievedAt": retrieved_at,
    }


def _deduplicate(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[str, dict[str, Any]] = {}
    order: list[str] = []

    for record in records:
        key = _dedup_key(record)
        existing = grouped.get(key)
        if existing is None:
            grouped[key] = record
            order.append(key)
            continue
        _merge(existing, record)

    return [grouped[key] for key in order]


def _merge(target: dict[str, Any], incoming: dict[str, Any]) -> None:
    if incoming.get("providerName") and incoming["providerName"] not in target["alsoFoundIn"]:
        if incoming["providerName"] != target["providerName"]:
            target["alsoFoundIn"].append(incoming["providerName"])

    target_rank = _COVERAGE_RANK.get(str(target.get("coverage")), 0)
    incoming_rank = _COVERAGE_RANK.get(str(incoming.get("coverage")), 0)
    if incoming_rank > target_rank:
        target["coverage"] = incoming["coverage"]

    for field in ("abstract", "doi", "url", "venue", "year"):
        if not target.get(field) and incoming.get(field):
            target[field] = incoming[field]
    if not target.get("authors") and incoming.get("authors"):
        target["authors"] = incoming["authors"]


def _dedup_key(record: dict[str, Any]) -> str:
    doi = record.get("doi")
    if doi:
        return f"doi:{str(doi).lower()}"
    title = _normalize_title(str(record.get("title") or ""))
    if title:
        return f"title:{title}"
    return f"id:{record.get('id')}"


def _normalize_title(value: str) -> str:
    return re.sub(r"[^\w\u4e00-\u9fff]+", "", value.lower())


def _openalex_abstract(inverted: Any) -> str | None:
    if not isinstance(inverted, dict):
        return None
    positions: list[tuple[int, str]] = []
    for word, indexes in inverted.items():
        if not isinstance(indexes, list):
            continue
        for index in indexes:
            if isinstance(index, int):
                positions.append((index, str(word)))
    if not positions:
        return None
    positions.sort(key=lambda item: item[0])
    return " ".join(word for _, word in positions)


def _get_json(
    url: str,
    params: dict[str, Any],
    headers: dict[str, str],
    proxy: str | None,
    timeout: float,
) -> Any:
    with httpx.Client(timeout=timeout, proxy=proxy or None, follow_redirects=True) as client:
        response = client.get(url, params=params, headers=headers)
        response.raise_for_status()
        return response.json()


def _get_text(
    url: str,
    params: dict[str, Any],
    headers: dict[str, str],
    proxy: str | None,
    timeout: float,
) -> str:
    with httpx.Client(timeout=timeout, proxy=proxy or None, follow_redirects=True) as client:
        response = client.get(url, params=params, headers=headers)
        response.raise_for_status()
        return response.text


def _configs(value: Any) -> list[SourceConfig]:
    if not isinstance(value, list):
        return []
    configs: list[SourceConfig] = []
    for item in value:
        if not isinstance(item, dict):
            continue
        source_id = str(item.get("id") or "").strip()
        driver = str(item.get("driver") or "").strip()
        url = str(item.get("url") or "").strip()
        if not source_id or not driver or not url:
            continue
        configs.append(
            SourceConfig(
                id=source_id,
                name=str(item.get("name") or source_id),
                driver=driver,
                url=url,
                api_key=_optional_str(item.get("apiKey")),
                contact_email=_optional_str(item.get("contactEmail")),
                language=_optional_str(item.get("language")),
            )
        )
    return configs


def _normalize_doi(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    doi = value.strip()
    for prefix in _DOI_PREFIXES:
        if doi.lower().startswith(prefix):
            doi = doi[len(prefix) :]
            break
    return doi.lower() or None


def _doi_url(doi: str | None) -> str | None:
    return f"https://doi.org/{doi}" if doi else None


def _strip_markup(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    text = re.sub(r"<[^>]+>", " ", value)
    text = re.sub(r"\s+", " ", text).strip()
    return text or None


def _clean_text(value: Any) -> str:
    if not isinstance(value, str):
        return ""
    return re.sub(r"\s+", " ", value).strip()


def _optional_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.isdigit():
        return int(value)
    return None


def _optional_str(value: Any) -> str | None:
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def _clamp_int(value: Any, minimum: int, maximum: int, fallback: int) -> int:
    parsed = _optional_int(value)
    if parsed is None:
        return fallback
    return max(minimum, min(maximum, parsed))


def _clamp_float(value: Any, minimum: float, maximum: float, fallback: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return fallback
    return max(minimum, min(maximum, float(value)))


def _year_matches(year: Any, year_from: int | None, year_to: int | None) -> bool:
    if not isinstance(year, int):
        return year_from is None and year_to is None
    if year_from is not None and year < year_from:
        return False
    if year_to is not None and year > year_to:
        return False
    return True


def _sort_key(record: dict[str, Any]) -> tuple[int, str]:
    year = record.get("year")
    return (-(year if isinstance(year, int) else 0), _normalize_title(record["title"]))


def _now_iso() -> str:
    return datetime.now(UTC).isoformat(timespec="seconds").replace("+00:00", "Z")


def _safe_error(exc: Exception) -> str:
    if isinstance(exc, httpx.HTTPStatusError):
        return f"HTTP {exc.response.status_code}"
    if isinstance(exc, httpx.RequestError):
        return f"网络请求失败：{type(exc).__name__}"
    if isinstance(exc, ET.ParseError):
        return "来源返回的 XML 无法解析。"
    return f"检索失败：{type(exc).__name__}"
