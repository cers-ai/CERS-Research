import { useCallback, useEffect, useState } from "react";

import { Modal } from "../../components/Modal";
import {
  getAssetText,
  listAssets,
  scanWorkspace,
  type AssetItem,
  type ScanSummary,
  type WorkspaceInfo,
} from "../../lib/ipc";

interface AssetsPageProps {
  workspace: WorkspaceInfo | null;
}

const KIND_LABELS: Record<string, string> = {
  literature: "文献",
  dataset: "数据集",
  code: "代码与脚本",
  document: "文档",
  image: "图片",
  other: "其他",
};

const KIND_ICONS: Record<string, string> = {
  literature: "▤",
  dataset: "▥",
  code: "⌘",
  document: "▦",
  image: "◨",
  other: "◌",
};

const FILTERS: Array<[string, string]> = [
  ["all", "全部"],
  ["literature", "文献"],
  ["dataset", "数据集"],
  ["code", "代码与脚本"],
  ["document", "文档"],
  ["image", "图片"],
  ["other", "其他"],
];

const STATUS_LABELS: Record<string, string> = {
  indexed: "已索引",
  parsing: "解析中",
  unsupported: "不支持",
  missing: "源文件不可用",
  relocated: "已重定位",
  error: "错误",
};

export function AssetsPage({ workspace }: AssetsPageProps) {
  const [assets, setAssets] = useState<AssetItem[]>([]);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [scanning, setScanning] = useState(false);
  const [loading, setLoading] = useState(false);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<AssetItem | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  useEffect(() => {
    if (!workspace) {
      setAssets([]);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(() => {
      setLoading(true);
      listAssets(query.trim() || undefined, kind === "all" ? undefined : kind)
        .then((results) => {
          if (!cancelled) {
            setAssets(results);
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
    }, 220);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [workspace, query, kind]);

  useEffect(() => {
    if (!selected) {
      setPreview(null);
      return;
    }

    let cancelled = false;
    setPreview(null);
    setPreviewLoading(true);
    getAssetText(selected.id, selected.kind === "literature" ? 60_000 : undefined)
      .then((text) => {
        if (!cancelled) {
          setPreview(text);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(toMessage(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setPreviewLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selected]);

  const handleScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const result = await scanWorkspace();
      setSummary(result);
      const results = await listAssets(
        query.trim() || undefined,
        kind === "all" ? undefined : kind,
      );
      setAssets(results);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setScanning(false);
    }
  }, [query, kind]);

  if (!workspace) {
    return (
      <section className="page page--centered">
        <div className="empty-state">
          <div className="empty-icon" aria-hidden="true">
            ▤
          </div>
          <h1>打开项目后查看研究资产</h1>
          <p className="empty-lead">
            研究资产是项目内统一的逻辑视图：文献、数据集、代码、成果与报告在同一处检索。
          </p>
        </div>
      </section>
    );
  }

  const filtering = query.trim().length > 0 || kind !== "all";

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <div className="eyebrow">研究资产</div>
          <h1>统一逻辑资产视图</h1>
          <p className="page-subtitle">
            {workspace.workspacePath} · 文件保留原位，索引不复制、不移动原始文件
          </p>
        </div>
        <div className="page-head-actions">
          <button
            type="button"
            className="btn btn--primary"
            disabled={scanning}
            onClick={() => void handleScan()}
          >
            {scanning ? "扫描中…" : assets.length ? "重新扫描" : "扫描 Workspace"}
          </button>
        </div>
      </header>

      {error && (
        <div className="canvas-error" role="alert">
          <span>{error}</span>
          <button
            type="button"
            className="btn btn--ghost btn--compact"
            onClick={() => setError(null)}
          >
            关闭
          </button>
        </div>
      )}

      {summary && (
        <div className={`scan-summary ${summary.truncated ? "is-warning" : ""}`}>
          已扫描 {summary.scanned} 个文件 · 新增 {summary.added} · 更新 {summary.updated} ·
          重定位 {summary.relinked} · 缺失 {summary.missing} · 跳过 {summary.skipped} ·{" "}
          {summary.durationMs} ms
          {summary.truncated && "（文件过多，本次仅索引前 5000 个）"}
        </div>
      )}

      <div className="asset-toolbar">
        <label className="searchbox">
          <span aria-hidden="true">⌕</span>
          <input
            value={query}
            placeholder="按文件名、路径或正文内容搜索……"
            aria-label="搜索研究资产"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <span className="muted">
          {loading ? "正在检索…" : `${assets.length} 项${filtering ? "匹配" : "资产"}`}
        </span>
      </div>

      <div className="asset-filters">
        {FILTERS.map(([value, label]) => (
          <button
            key={value}
            type="button"
            className={`choice ${kind === value ? "choice--active" : ""}`}
            onClick={() => setKind(value)}
          >
            {label}
          </button>
        ))}
      </div>

      {assets.length === 0 ? (
        <article className="card card--pad">
          <span className="tag tag--warning">
            {filtering ? "没有符合条件的资产" : "尚未建立索引"}
          </span>
          <h2 className="section-title section-title--spaced">
            {filtering ? "尝试其他关键词或类型" : "扫描 Workspace 建立逻辑资产视图"}
          </h2>
          <p className="muted">
            {filtering
              ? "当前关键词或类型下没有结果。清除筛选或换一个关键词。"
              : "扫描会读取文件名、类型、大小、修改时间与可解析的文本内容，并建立可搜索的索引。原始文件不会因扫描被移动、复制或覆盖。"}
          </p>
          {!filtering && (
            <button
              type="button"
              className="btn btn--compact"
              disabled={scanning}
              onClick={() => void handleScan()}
            >
              {scanning ? "扫描中…" : "开始扫描"}
            </button>
          )}
        </article>
      ) : (
        <div className="asset-list">
          {assets.map((asset) => (
            <button
              key={asset.id}
              type="button"
              className="asset-row"
              onClick={() => setSelected(asset)}
            >
              <span className="asset-icon" aria-hidden="true">
                {KIND_ICONS[asset.kind] ?? "◌"}
              </span>
              <span className="asset-main">
                <span className="asset-name">{asset.name}</span>
                <span className="asset-path">{asset.relativePath}</span>
                {asset.snippet && (
                  <span className="asset-snippet">
                    <HighlightedSnippet text={stripPageMarkers(asset.snippet)} />
                  </span>
                )}
              </span>
              <span className="asset-tags">
                <span className="tag">{KIND_LABELS[asset.kind] ?? asset.kind}</span>
                {asset.page != null && (
                  <span className="tag tag--muted">第 {asset.page} 页</span>
                )}
                <span
                  className={`tag ${
                    asset.status === "missing" ? "tag--warning" : "tag--success"
                  }`}
                >
                  {STATUS_LABELS[asset.status] ?? asset.status}
                </span>
              </span>
              <span className="asset-meta">
                <span>{formatBytes(asset.sizeBytes)}</span>
                <span>{formatDate(asset.modifiedAt)}</span>
              </span>
            </button>
          ))}
        </div>
      )}

      {selected && (
        <Modal
          title={selected.name}
          description="资产是真实文件的稳定引用。这里的索引不会复制或移动原始文件。"
          onClose={() => setSelected(null)}
          footer={
            <button type="button" className="btn" onClick={() => setSelected(null)}>
              关闭
            </button>
          }
        >
          <dl className="settings-list">
            <DetailRow label="类型" value={KIND_LABELS[selected.kind] ?? selected.kind} />
            <DetailRow
              label="状态"
              value={STATUS_LABELS[selected.status] ?? selected.status}
            />
            <DetailRow label="相对路径" value={selected.relativePath} mono />
            <DetailRow label="原始位置" value={selected.absolutePath} mono />
            <DetailRow
              label="来源"
              value={selected.origin === "output" ? "系统输出" : "Workspace 文件"}
            />
            <DetailRow label="大小" value={formatBytes(selected.sizeBytes)} />
            <DetailRow label="修改时间" value={formatDate(selected.modifiedAt)} />
            <DetailRow label="资产 ID" value={selected.id} mono />
          </dl>

          <h3 className="node-workspace-section">内容预览</h3>
          {previewLoading ? (
            <p className="muted">正在读取正文…</p>
          ) : preview ? (
            <AssetPreview text={preview} initialPage={selected.page} />
          ) : (
            <p className="hint">
              该文件暂无正文索引。扫描版 PDF 需要先 OCR，暂不能建立可检索文本；其他不支持的类型只做文件级管理。
            </p>
          )}

          {selected.status === "missing" && (
            <div className="hint">
              原始文件已不可访问。索引与关联关系仍保留；把文件放回 Workspace 后重新扫描，
              内容一致的文件会自动重定位并保留原资产 ID。
            </div>
          )}
        </Modal>
      )}
    </section>
  );
}

function AssetPreview({
  text,
  initialPage,
}: {
  text: string;
  initialPage: number | null;
}) {
  const pages: Array<{ page: number; body: string }> = [];
  const parts = text.split(/〔澄思第 (\d+) 页〕/g);
  for (let index = 1; index < parts.length; index += 2) {
    pages.push({ page: Number(parts[index]), body: parts[index + 1]?.trim() ?? "" });
  }
  const [active, setActive] = useState<number | null>(initialPage);
  useEffect(() => {
    setActive(initialPage);
  }, [text, initialPage]);

  if (pages.length === 0) {
    return <pre className="asset-preview">{text}</pre>;
  }
  const current = pages.find((item) => item.page === active) ?? pages[0];

  return (
    <div className="asset-page-view">
      <div className="asset-page-nav">
        <span className="muted">共 {pages.length} 页 · 只显示 PDF 文本层</span>
        <select
          aria-label="选择 PDF 页码"
          value={current.page}
          onChange={(event) => setActive(Number(event.target.value))}
        >
          {pages.map((item) => (
            <option key={item.page} value={item.page}>
              第 {item.page} 页
            </option>
          ))}
        </select>
      </div>
      <pre className="asset-preview">
        {current.body || "（本页没有可提取的文本）"}
      </pre>
    </div>
  );
}

function stripPageMarkers(text: string): string {
  return text.replace(/〔澄思第 \d+ 页〕/g, "").trim();
}

function HighlightedSnippet({ text }: { text: string }) {
  const segments = text.split(/(\[\[\[.*?\]\]\])/g);
  return (
    <>
      {segments.map((segment, index) => {
        if (segment.startsWith("[[[") && segment.endsWith("]]]")) {
          return <mark key={index}>{segment.slice(3, -3)}</mark>;
        }
        return <span key={index}>{segment}</span>;
      })}
    </>
  );
}

function DetailRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="settings-row">
      <dt>{label}</dt>
      <dd className={mono ? "mono" : undefined}>{value}</dd>
    </div>
  );
}

function formatBytes(value: number | null): string {
  if (value === null || value === undefined) {
    return "—";
  }
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  if (value < 1024 * 1024 * 1024) {
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(value / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatDate(value: string | null): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "操作失败，请稍后重试。";
}
