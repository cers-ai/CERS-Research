import { useCallback, useEffect, useState } from "react";

import {
  exportReportMarkdown,
  getReport,
  listReportRevisions,
  saveReport,
  type ReportDocument,
  type ReportRevision,
} from "../../lib/ipc";

interface ReportPanelProps {
  nodeId: string;
  title: string;
}

export function ReportPanel({ nodeId, title }: ReportPanelProps) {
  const [report, setReport] = useState<ReportDocument | null>(null);
  const [content, setContent] = useState("");
  const [revisions, setRevisions] = useState<ReportRevision[]>([]);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const loadRevisions = useCallback(async () => {
    try {
      setRevisions(await listReportRevisions(nodeId));
    } catch {
      // 版本列表读取失败不阻塞正文编辑。
    }
  }, [nodeId]);

  useEffect(() => {
    let cancelled = false;
    getReport(nodeId)
      .then((value) => {
        if (!cancelled) {
          setReport(value);
          setContent(value.contentMd);
          setDirty(false);
          setError(null);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(toMessage(cause));
        }
      });
    void loadRevisions();

    return () => {
      cancelled = true;
    };
  }, [nodeId, loadRevisions]);

  const save = useCallback(async () => {
    if (!report || saving) {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveReport(nodeId, content, report.revision);
      setReport(saved);
      setContent(saved.contentMd);
      setDirty(false);
      setError(null);
      setNotice(`已保存 v${saved.revision}`);
      void loadRevisions();
    } catch (cause) {
      setError(toMessage(cause));
      setNotice(null);
    } finally {
      setSaving(false);
    }
  }, [report, saving, nodeId, content, loadRevisions]);

  useEffect(() => {
    if (!dirty || !report) {
      return;
    }
    const timer = window.setTimeout(() => {
      void save();
    }, 1200);
    return () => window.clearTimeout(timer);
  }, [content, dirty, report, save]);

  useEffect(() => {
    if (!notice) {
      return;
    }
    const timer = window.setTimeout(() => setNotice(null), 2200);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const restoreRevision = (revision: number) => {
    const target = revisions.find((item) => item.revision === revision);
    if (!target) {
      return;
    }
    setContent(target.contentMd);
    setDirty(true);
    setNotice(`已载入 v${revision}，保存后生成新版本`);
  };

  const exportMarkdown = async () => {
    setExporting(true);
    try {
      const path = await exportReportMarkdown(nodeId);
      setNotice(`已导出：${path}`);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="report-panel">
      <div className="report-toolbar">
        <span className="tag tag--muted">
          {report ? `v${report.revision}` : "新报告"}
        </span>
        <span className={`report-status ${dirty ? "is-dirty" : ""}`}>
          {saving ? "保存中…" : dirty ? "有未保存修改" : "已自动保存"}
        </span>
        <span className="muted">{countChars(content)} 字符</span>
        <div className="grow" />
        {revisions.length > 0 && (
          <select
            aria-label="恢复报告版本"
            value=""
            onChange={(event) => restoreRevision(Number(event.target.value))}
          >
            <option value="">恢复历史版本…</option>
            {revisions.map((revision) => (
              <option key={revision.id} value={revision.revision}>
                v{revision.revision} · {formatTime(revision.createdAt)}
              </option>
            ))}
          </select>
        )}
        <button
          type="button"
          className="btn btn--compact"
          disabled={saving || !dirty}
          onClick={() => void save()}
        >
          保存
        </button>
        <button
          type="button"
          className="btn btn--compact"
          disabled={exporting || !report}
          onClick={() => void exportMarkdown()}
        >
          {exporting ? "导出中…" : "导出 Markdown"}
        </button>
      </div>

      {error && <div className="canvas-error">{error}</div>}
      {notice && <div className="settings-notice">{notice}</div>}

      <textarea
        className="report-editor"
        value={content}
        spellCheck={false}
        placeholder={`# ${title}\n\n从提纲开始，或直接写下正文。支持 Markdown：标题、列表、表格、引用。`}
        onChange={(event) => {
          setContent(event.target.value);
          setDirty(true);
        }}
      />

      <p className="hint">
        正文自动保存并保留版本。当前版本支持 Markdown 导出；DOCX/PDF 导出将在文档渲染模块接入后开放。
      </p>
    </div>
  );
}

function countChars(value: string): number {
  return value.replace(/\s/g, "").length;
}

function formatTime(value: string): string {
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
  return "报告操作失败，请稍后重试。";
}
