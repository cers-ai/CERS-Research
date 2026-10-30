from __future__ import annotations

from chengsi_worker.sources import (
    _deduplicate,
    _normalize_doi,
    _openalex_abstract,
)


def _record(
    provider: str,
    provider_name: str,
    *,
    doi: str | None,
    title: str,
    coverage: str,
    abstract: str | None = None,
) -> dict[str, object]:
    return {
        "id": f"src_{provider}",
        "externalId": "x",
        "provider": provider,
        "providerName": provider_name,
        "title": title,
        "authors": ["A"],
        "year": 2024,
        "venue": "Journal",
        "doi": doi,
        "url": None,
        "abstract": abstract,
        "coverage": coverage,
        "type": "article",
        "alsoFoundIn": [],
        "retrievedAt": "2026-10-29T00:00:00Z",
    }


def test_deduplicates_same_doi_and_merges_coverage() -> None:
    first = _record(
        "openalex",
        "OpenAlex",
        doi="10.1000/example",
        title="Urban heat",
        coverage="metadata_only",
    )
    second = _record(
        "crossref",
        "Crossref",
        doi="10.1000/EXAMPLE",
        title="Urban heat",
        coverage="abstract_only",
        abstract="An abstract.",
    )

    merged = _deduplicate([first, second])
    assert len(merged) == 1
    assert merged[0]["coverage"] == "abstract_only"
    assert merged[0]["abstract"] == "An abstract."
    assert "Crossref" in merged[0]["alsoFoundIn"]


def test_normalizes_doi_prefixes() -> None:
    assert _normalize_doi("https://doi.org/10.1000/ABC") == "10.1000/abc"
    assert _normalize_doi("http://dx.doi.org/10.1000/ABC") == "10.1000/abc"
    assert _normalize_doi(None) is None


def test_reconstructs_openalex_abstract() -> None:
    inverted = {"Urban": [0], "heat": [1], "islands": [2]}
    assert _openalex_abstract(inverted) == "Urban heat islands"
