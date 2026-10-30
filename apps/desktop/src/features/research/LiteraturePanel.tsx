import { useEffect, useState } from "react";

import {
  listSavedSources,
  saveAcademicSource,
  searchAcademicSources,
  type AcademicRecord,
  type AcademicSearchResponse,
} from "../../lib/ipc";

const COVERAGE_LABELS: Record<string, string> = {
  metadata_only: "仅题录",
  abstract_only: "仅摘要",
  fulltext: "已获全文",
  unavailable: "不可获取",
};

interface LiteraturePanelProps {
  nodeId: string;
  refreshKey: number;
  onSaved: () => void;
}

export function LiteraturePanel({
  nodeId,
  refreshKey,
  onSaved,
}: LiteraturePanelProps) {
  const [query, setQuery] = useState("");
  const [yearFrom, setYearFrom] = useState("");
  const [yearTo, setYearTo] = useState("");
  const [searching, setSearching] = useState(false);
  const [response, setResponse] = useState<AcademicSearchResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [savedKeys, setSavedKeys] = useState<Set<string>>(new Set());
  const [savingId, setSavingId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    listSavedSources(nodeId)
      .then((sources) => {
        if (!cancelled) {
          setSavedKeys(new Set(sources.map(recordKey)));
        }
      })
      .catch(() => {
        // 收藏状态读取失败不阻塞检索。
      });
    return () => {
      cancelled = true;
    };
  }, [nodeId, refreshKey]);

  const save = async (record: AcademicRecord) => {
    setSavingId(record.id);
    try {
      await saveAcademicSource({
        provider: record.provider,
        providerName: record.providerName,
        externalId: record.externalId,
        doi: record.doi,
        url: record.url,
        title: record.title,
        authors: record.authors,
        year: record.year,
        venue: record.venue,
        abstract: record.abstract,
        coverage: record.coverage,
        recordType: record.type,
        retrievedAt: record.retrievedAt,
        nodeId,
      });
      setSavedKeys((current) => new Set(current).add(recordKey(record)));
      setError(null);
      onSaved();
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setSavingId(null);
    }
  };

  const runSearch = async () => {
    const text = query.trim();
    if (!text || searching) {
      return;
    }

    setSearching(true);
    setError(null);
    try {
      const result = await searchAcademicSources(
        text,
        parseYear(yearFrom),
        parseYear(yearTo),
      );
      setResponse(result);
      if (!result.ok && result.message) {
        setError(result.message);
      }
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setSearching(false);
    }
  };

  const copyText = async (id: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      window.setTimeout(() => setCopiedId((current) => (current === id ? null : current)), 1800);
    } catch {
      setError("无法写入剪贴板，请手动选择文本复制。");
    }
  };

  return (
    <div className="literature-panel">
      <div className="lit-search">
        <input
          value={query}
          placeholder="检索关键词，例如：城市绿地 局地热环境"
          aria-label="学术检索关键词"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void runSearch();
            }
          }}
        />
        <input
          className="lit-year"
          value={yearFrom}
          placeholder="起始年"
          inputMode="numeric"
          aria-label="起始年份"
          onChange={(event) => setYearFrom(event.target.value)}
        />
        <input
          className="lit-year"
          value={yearTo}
          placeholder="结束年"
          inputMode="numeric"
          aria-label="结束年份"
          onChange={(event) => setYearTo(event.target.value)}
        />
        <button
          type="button"
          className="btn btn--primary btn--compact"
          disabled={searching || !query.trim()}
          onClick={() => void runSearch()}
        >
          {searching ? "检索中…" : "检索"}
        </button>
      </div>

      <p className="muted lit-note">
        检索已启用的学术来源。题录或摘要不等于已获全文，也不会生成不存在的页码。
      </p>

      {error && <div className="canvas-error">{error}</div>}

      {response && (
        <>
          <div className="lit-statuses">
            {response.sourceStatuses.map((status) => (
              <span
                key={status.id}
                className={`tag ${status.ok ? "tag--success" : "tag--warning"}`}
                title={status.error ?? undefined}
              >
                {status.name}
                {status.ok ? ` · ${status.count}` : " · 失败"}
              </span>
            ))}
          </div>

          {response.results.length === 0 ? (
            <p className="muted">没有检索到结果。可以换关键词或放宽年份范围。</p>
          ) : (
            <>
              <p className="muted">
                去重后 {response.results.length} 条结果 · 检索时间{" "}
                {formatTimestamp(response.retrievedAt)}
              </p>
              <div className="lit-results">
                {response.results.map((record) => (
                  <article className="lit-result" key={record.id}>
                    <h4 className="lit-title">{record.title || "未命名文献"}</h4>
                    <p className="lit-meta">
                      {record.authors.length
                        ? `${record.authors.slice(0, 4).join(", ")}${
                            record.authors.length > 4 ? " 等" : ""
                          }`
                        : "作者未知"}
                      {record.year ? ` · ${record.year}` : ""}
                      {record.venue ? ` · ${record.venue}` : ""}
                    </p>

                    <div className="lit-tags">
                      <span
                        className={`tag ${
                          record.coverage === "abstract_only" ? "tag--warning" : ""
                        }`}
                      >
                        {COVERAGE_LABELS[record.coverage] ?? record.coverage}
                      </span>
                      <span className="tag">{record.providerName}</span>
                      {record.alsoFoundIn.map((name) => (
                        <span className="tag tag--muted" key={name}>
                          {name}
                        </span>
                      ))}
                      {record.type && <span className="tag tag--muted">{record.type}</span>}
                    </div>

                    {record.abstract && (
                      <p className="lit-abstract" title={record.abstract}>
                        {record.abstract}
                      </p>
                    )}

                    <div className="lit-actions">
                      {record.doi && (
                        <span className="lit-doi mono">DOI: {record.doi}</span>
                      )}
                      <button
                        type="button"
                        className={`btn btn--compact ${
                          savedKeys.has(recordKey(record)) ? "" : "btn--primary"
                        }`}
                        disabled={
                          savedKeys.has(recordKey(record)) ||
                          savingId === record.id
                        }
                        onClick={() => void save(record)}
                      >
                        {savedKeys.has(recordKey(record))
                          ? "已收藏"
                          : savingId === record.id
                            ? "收藏中…"
                            : "收藏到节点"}
                      </button>
                      <button
                        type="button"
                        className="btn btn--compact"
                        onClick={() =>
                          void copyText(record.id, citationOf(record))
                        }
                      >
                        {copiedId === record.id ? "已复制" : "复制引用"}
                      </button>
                      {record.url && (
                        <button
                          type="button"
                          className="btn btn--ghost btn--compact"
                          onClick={() => void copyText(`${record.id}_url`, record.url ?? "")}
                        >
                          {copiedId === `${record.id}_url` ? "链接已复制" : "复制链接"}
                        </button>
                      )}
                    </div>
                  </article>
                ))}
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

function citationOf(record: AcademicRecord): string {
  const authors = record.authors.length
    ? `${record.authors.slice(0, 3).join(", ")}${
        record.authors.length > 3 ? " 等" : ""
      }`
    : "作者未知";
  const venue = record.venue ?? "";
  const year = record.year ? String(record.year) : "";
  const identifier = record.doi ? `DOI: ${record.doi}` : record.url ?? "";
  return [authors, record.title, venue, year, identifier].filter(Boolean).join(". ");
}

function parseYear(value: string): number | undefined {
  const parsed = Number.parseInt(value.trim(), 10);
  if (!Number.isFinite(parsed) || parsed < 1800 || parsed > 2200) {
    return undefined;
  }
  return parsed;
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

function recordKey(record: { doi: string | null; title: string }): string {
  if (record.doi) {
    return `doi:${record.doi.toLowerCase()}`;
  }
  return `title:${record.title.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "")}`;
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "检索失败，请稍后重试。";
}
