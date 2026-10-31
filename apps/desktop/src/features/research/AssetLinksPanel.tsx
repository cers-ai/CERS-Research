import { useCallback, useEffect, useState } from "react";

import {
  linkAssetToNode,
  listAssets,
  listNodeAssets,
  unlinkAssetFromNode,
  type AssetItem,
  type LinkedAsset,
} from "../../lib/ipc";

const KIND_LABELS: Record<string, string> = {
  literature: "文献",
  dataset: "数据集",
  code: "代码",
  document: "文档",
  image: "图片",
  other: "其他",
};

interface AssetLinksPanelProps {
  nodeId: string;
}

export function AssetLinksPanel({ nodeId }: AssetLinksPanelProps) {
  const [linked, setLinked] = useState<LinkedAsset[]>([]);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AssetItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setLinked(await listNodeAssets(nodeId));
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [nodeId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const runSearch = async () => {
    if (searching) {
      return;
    }
    setSearching(true);
    setError(null);
    try {
      const items = await listAssets(query.trim() || undefined, undefined);
      const linkedIds = new Set(linked.map((item) => item.id));
      setResults(items.filter((item) => !linkedIds.has(item.id)).slice(0, 12));
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setSearching(false);
    }
  };

  const link = async (assetId: string) => {
    setBusyId(assetId);
    try {
      await linkAssetToNode(nodeId, assetId);
      setResults((current) => current.filter((item) => item.id !== assetId));
      await reload();
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusyId(null);
    }
  };

  const unlink = async (assetId: string) => {
    setBusyId(assetId);
    try {
      await unlinkAssetFromNode(nodeId, assetId);
      await reload();
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="asset-links">
      <div className="lit-search">
        <input
          value={query}
          placeholder="搜索本地资产，留空列出前 12 项"
          aria-label="搜索本地研究资产"
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
          className="btn btn--compact"
          disabled={searching}
          onClick={() => void runSearch()}
        >
          {searching ? "查找中…" : "查找"}
        </button>
      </div>

      <p className="muted lit-note">
        引用只建立节点与本地文件的关联，不复制、不移动原文。对话会读取有限摘录，不能据此声称已通读全文。
      </p>

      {error && <div className="canvas-error">{error}</div>}

      {results.length > 0 && (
        <div className="asset-link-results">
          {results.map((asset) => (
            <div className="asset-link-row" key={asset.id}>
              <div className="asset-link-main">
                <strong>{asset.name}</strong>
                <span className="muted">{asset.relativePath}</span>
              </div>
              <span className="tag tag--muted">{KIND_LABELS[asset.kind] ?? asset.kind}</span>
              <button
                type="button"
                className="btn btn--compact"
                disabled={busyId === asset.id}
                onClick={() => void link(asset.id)}
              >
                {busyId === asset.id ? "引用中…" : "引用"}
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="section-head">
        <h4 className="saved-sources-title">已引用本地资料</h4>
        <span className="tag tag--muted">{linked.length}</span>
      </div>
      {linked.length === 0 ? (
        <p className="muted">当前节点还没有引用本地资料。</p>
      ) : (
        <div className="asset-link-results">
          {linked.map((asset) => (
            <div className="asset-link-row" key={asset.id}>
              <div className="asset-link-main">
                <strong>{asset.name}</strong>
                <span className="muted">{asset.relativePath}</span>
              </div>
              <span className="tag tag--muted">{KIND_LABELS[asset.kind] ?? asset.kind}</span>
              {asset.status === "missing" && <span className="tag tag--warning">源文件不可用</span>}
              {!asset.hasText && <span className="tag">暂无正文</span>}
              <button
                type="button"
                className="btn btn--ghost btn--compact"
                disabled={busyId === asset.id}
                onClick={() => void unlink(asset.id)}
              >
                移除
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
  return "资产引用操作失败，请稍后重试。";
}
