import { useEffect, useState } from "react";

import {
  listSavedSources,
  removeSavedSource,
  type SavedSource,
} from "../../lib/ipc";

const COVERAGE_LABELS: Record<string, string> = {
  metadata_only: "仅题录",
  abstract_only: "仅摘要",
  fulltext: "已获全文",
  unavailable: "不可获取",
};

interface SavedSourcesPanelProps {
  nodeId: string;
  refreshKey: number;
  onChanged: () => void;
}

export function SavedSourcesPanel({
  nodeId,
  refreshKey,
  onChanged,
}: SavedSourcesPanelProps) {
  const [sources, setSources] = useState<SavedSource[]>([]);
  const [loading, setLoading] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listSavedSources(nodeId)
      .then((items) => {
        if (!cancelled) {
          setSources(items);
          setError(null);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(toMessage(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [nodeId, refreshKey]);

  const remove = async (sourceId: string) => {
    setRemovingId(sourceId);
    try {
      await removeSavedSource(nodeId, sourceId);
      setSources((current) => current.filter((source) => source.id !== sourceId));
      setError(null);
      onChanged();
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setRemovingId(null);
    }
  };

  return (
    <div className="saved-sources">
      <div className="section-head">
        <h4 className="saved-sources-title">已收藏来源</h4>
        <span className="tag tag--muted">{sources.length}</span>
      </div>

      {loading && <p className="muted">正在读取已收藏来源…</p>}
      {error && <div className="canvas-error">{error}</div>}
      {!loading && !error && sources.length === 0 && (
        <p className="muted">
          检索结果可以收藏到当前节点；对话会优先参考这些来源，并标注仅题录或仅摘要。
        </p>
      )}

      {sources.length > 0 && (
        <div className="saved-source-list">
          {sources.map((source) => (
            <div className="saved-source-row" key={source.id}>
              <div className="saved-source-main">
                <strong>{source.title}</strong>
                <small>
                  {source.authors.length
                    ? `${source.authors.slice(0, 3).join(", ")}${
                        source.authors.length > 3 ? " 等" : ""
                      }`
                    : "作者未知"}
                  {source.year ? ` · ${source.year}` : ""}
                  {source.venue ? ` · ${source.venue}` : ""}
                </small>
              </div>
              <span className="tag tag--muted">
                {COVERAGE_LABELS[source.coverage] ?? source.coverage}
              </span>
              <button
                type="button"
                className="btn btn--danger-ghost btn--compact"
                disabled={removingId === source.id}
                onClick={() => void remove(source.id)}
              >
                {removingId === source.id ? "移除中…" : "移除"}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "读取已收藏来源失败。";
}
