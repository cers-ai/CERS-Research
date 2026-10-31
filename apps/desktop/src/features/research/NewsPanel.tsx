import { useEffect, useState } from "react";

import {
  listSavedSources,
  saveAcademicSource,
  searchNews,
  type AcademicRecord,
  type AcademicSearchResponse,
} from "../../lib/ipc";

interface NewsPanelProps {
  nodeId: string;
  refreshKey: number;
  onSaved: () => void;
}

export function NewsPanel({ nodeId, refreshKey, onSaved }: NewsPanelProps) {
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState<AcademicSearchResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedKeys, setSavedKeys] = useState<Set<string>>(new Set());
  const [savingId, setSavingId] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

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

  const runSearch = async () => {
    if (loading) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const result = await searchNews(query.trim(), 12);
      setResponse(result);
      if (!result.ok && result.message) {
        setError(result.message);
      }
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setLoading(false);
    }
  };

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
          placeholder="关键词可选；留空则读取已启用订阅的最新资讯"
          aria-label="资讯检索关键词"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void runSearch();
            }
          }}
        />
        <button
          type="button"
          className="btn btn--primary btn--compact"
          disabled={loading}
          onClick={() => void runSearch()}
        >
          {loading ? "获取中…" : "获取资讯"}
        </button>
      </div>

      <p className="muted lit-note">
        资讯来自已启用的 RSS/Atom 与新闻检索服务。这里只保留标题、摘要、原站链接与检索时间，
        不抓取正文，也不把多家转载当作独立证实。
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
            <p className="muted">没有获取到资讯。可以换关键词，或在设置中检查订阅是否启用。</p>
          ) : (
            <>
              <p className="muted">
                {response.results.length} 条结果 · 检索时间 {formatTimestamp(response.retrievedAt)}
              </p>
              <div className="lit-results">
                {response.results.map((record) => (
                  <article className="lit-result" key={record.id}>
                    <h4 className="lit-title">{record.title}</h4>
                    <p className="lit-meta">
                      {record.venue || record.providerName}
                      {record.publishedAt
                        ? ` · 发布 ${formatTimestamp(record.publishedAt)}`
                        : " · 发布时间未知"}
                    </p>

                    <div className="lit-tags">
                      <span className="tag tag--warning">仅题录</span>
                      <span className="tag">{record.providerName}</span>
                      {record.alsoFoundIn.map((name) => (
                        <span className="tag tag--muted" key={name}>
                          亦见 {name}
                        </span>
                      ))}
                    </div>

                    {record.abstract && (
                      <p className="lit-abstract" title={record.abstract}>
                        {record.abstract}
                      </p>
                    )}

                    <div className="lit-actions">
                      <button
                        type="button"
                        className={`btn btn--compact ${
                          savedKeys.has(recordKey(record)) ? "" : "btn--primary"
                        }`}
                        disabled={
                          savedKeys.has(recordKey(record)) || savingId === record.id
                        }
                        onClick={() => void save(record)}
                      >
                        {savedKeys.has(recordKey(record))
                          ? "已收藏"
                          : savingId === record.id
                            ? "收藏中…"
                            : "收藏到节点"}
                      </button>
                      {record.url && (
                        <>
                          <button
                            type="button"
                            className="btn btn--ghost btn--compact"
                            onClick={() => void copyText(`${record.id}_url`, record.url ?? "")}
                          >
                            {copiedId === `${record.id}_url` ? "链接已复制" : "复制链接"}
                          </button>
                          <a
                            className="btn btn--ghost btn--compact"
                            href={record.url}
                            target="_blank"
                            rel="noreferrer"
                          >
                            打开原站
                          </a>
                        </>
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
  return "资讯获取失败，请稍后重试。";
}
