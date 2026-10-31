import { useCallback, useEffect, useRef, useState } from "react";

import { Modal } from "../../components/Modal";
import {
  applyReportChanges,
  exportReportDocument,
  exportReportMarkdown,
  getReport,
  listReportRevisions,
  listSavedSources,
  saveReport,
  suggestReportChanges,
  suggestReportOutline,
  type ReportChangeProposal,
  type ReportOutline,
  type ReportDocument,
  type ReportRevision,
  type SavedSource,
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
  const [exporting, setExporting] = useState<string | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [instruction, setInstruction] = useState("");
  const [aiLoading, setAiLoading] = useState(false);
  const [applying, setApplying] = useState(false);
  const [proposal, setProposal] = useState<ReportChangeProposal | null>(null);
  const [accepted, setAccepted] = useState<Set<string>>(new Set());
  const [aiError, setAiError] = useState<string | null>(null);
  const [citeOpen, setCiteOpen] = useState(false);
  const [sources, setSources] = useState<SavedSource[]>([]);
  const [citeLoading, setCiteLoading] = useState(false);
  const [citeError, setCiteError] = useState<string | null>(null);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const [outlineLoading, setOutlineLoading] = useState(false);
  const [outline, setOutline] = useState<ReportOutline | null>(null);
  const [outlineError, setOutlineError] = useState<string | null>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);

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

  const exportReport = async (format: "md" | "docx" | "pdf") => {
    setExporting(format);
    try {
      const path =
        format === "md"
          ? await exportReportMarkdown(nodeId)
          : await exportReportDocument(nodeId, format);
      setNotice(`已导出：${path}`);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setExporting(null);
    }
  };

  const openCitations = async () => {
    setCiteOpen(true);
    setCiteError(null);
    setCiteLoading(true);
    try {
      setSources(await listSavedSources(nodeId));
    } catch (cause) {
      setCiteError(toMessage(cause));
    } finally {
      setCiteLoading(false);
    }
  };

  const insertAtCursor = (text: string) => {
    const editor = editorRef.current;
    const start = editor?.selectionStart ?? content.length;
    const end = editor?.selectionEnd ?? start;
    const next = `${content.slice(0, start)}${text}${content.slice(end)}`;
    setContent(next);
    setDirty(true);
    setNotice("已插入引用");
    window.requestAnimationFrame(() => {
      if (!editor) {
        return;
      }
      editor.focus();
      const cursor = start + text.length;
      editor.setSelectionRange(cursor, cursor);
    });
  };

  const insertReferences = () => {
    if (sources.length === 0) {
      return;
    }
    const entries = sources
      .map((source, index) => referenceEntry(source, index + 1))
      .join("\n");
    const section = `## 参考文献\n\n${entries}\n`;
    const marker = "## 参考文献";
    const existing = content.indexOf(marker);
    const next =
      existing >= 0
        ? `${content.slice(0, existing).trimEnd()}\n\n${section}`
        : `${content.trimEnd()}\n\n${section}`;
    setContent(next);
    setDirty(true);
    setNotice(`已整理 ${sources.length} 条参考文献`);
    setCiteOpen(false);
  };

  const openOutline = async () => {
    setOutlineOpen(true);
    setOutline(null);
    setOutlineError(null);
    setOutlineLoading(true);
    try {
      setOutline(await suggestReportOutline(nodeId));
    } catch (cause) {
      setOutlineError(toMessage(cause));
    } finally {
      setOutlineLoading(false);
    }
  };

  const applyOutline = () => {
    if (!outline) {
      return;
    }
    const markdown = `# ${outline.title}\n\n${outline.sections
      .map(
        (section, index) =>
          `## ${index + 1}. ${section.heading}\n\n${section.points
            .map((point) => `- ${point}`)
            .join("\n")}`,
      )
      .join("\n\n")}\n`;
    setContent(markdown);
    setDirty(true);
    setOutlineOpen(false);
    setNotice("提纲已写入正文，可继续编辑或让 AI 修订");
    window.requestAnimationFrame(() => editorRef.current?.focus());
  };

  const openAi = () => {
    setInstruction("");
    setProposal(null);
    setAccepted(new Set());
    setAiError(null);
    setAiOpen(true);
  };

  const generateChanges = async () => {
    if (!instruction.trim() || !report) {
      return;
    }
    setAiLoading(true);
    setAiError(null);
    try {
      if (dirty) {
        const saved = await saveReport(nodeId, content, report.revision);
        setReport(saved);
        setContent(saved.contentMd);
        setDirty(false);
        void loadRevisions();
      }
      const next = await suggestReportChanges(nodeId, instruction);
      setProposal(next);
      setAccepted(new Set());
    } catch (cause) {
      setAiError(toMessage(cause));
    } finally {
      setAiLoading(false);
    }
  };

  const toggleChange = (id: string) => {
    setAccepted((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const applySelected = async () => {
    if (!proposal || accepted.size === 0) {
      return;
    }
    setApplying(true);
    setAiError(null);
    try {
      const result = await applyReportChanges(
        nodeId,
        proposal.id,
        Array.from(accepted),
      );
      setReport(result.document);
      setContent(result.document.contentMd);
      setDirty(false);
      setProposal(null);
      setAiOpen(false);
      setNotice(`已应用 ${accepted.size} 项修改，报告更新为 v${result.document.revision}`);
      void loadRevisions();
    } catch (cause) {
      setAiError(toMessage(cause));
    } finally {
      setApplying(false);
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
        {content.trim().length === 0 && (
          <button
            type="button"
            className="btn btn--compact"
            disabled={!report || outlineLoading}
            onClick={() => void openOutline()}
          >
            {outlineLoading ? "生成中…" : "生成提纲"}
          </button>
        )}
        <button
          type="button"
          className="btn btn--compact"
          onClick={() => void openCitations()}
        >
          引用来源
        </button>
        <button
          type="button"
          className="btn btn--compact"
          disabled={!report || saving}
          onClick={openAi}
        >
          AI 修订
        </button>
        <button
          type="button"
          className="btn btn--compact"
          disabled={saving || !dirty}
          onClick={() => void save()}
        >
          保存
        </button>
        {(["md", "docx", "pdf"] as const).map((format) => (
          <button
            key={format}
            type="button"
            className="btn btn--compact"
            disabled={Boolean(exporting) || !report}
            onClick={() => void exportReport(format)}
          >
            {exporting === format
              ? "导出中…"
              : format === "md"
                ? "导出 MD"
                : `导出 ${format.toUpperCase()}`}
          </button>
        ))}
      </div>

      {error && <div className="canvas-error">{error}</div>}
      {notice && <div className="settings-notice">{notice}</div>}

      <textarea
        ref={editorRef}
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
        正文自动保存并保留版本。可导出 Markdown、DOCX 与 PDF，文件保存在 Research Outputs/reports。
      </p>

      {outlineOpen && (
        <Modal
          title="报告提纲"
          description="提纲根据研究目标、已收藏来源与已引用资料生成；确认后才会写入正文。"
          onClose={() => setOutlineOpen(false)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setOutlineOpen(false)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={!outline}
                onClick={applyOutline}
              >
                采用提纲
              </button>
            </>
          }
        >
          {outlineError && <div className="canvas-error">{outlineError}</div>}
          {outlineLoading ? (
            <p className="muted">正在生成提纲…</p>
          ) : outline ? (
            <>
              <h3 className="node-workspace-section">{outline.title}</h3>
              {outline.sections.map((section, index) => (
                <div className="outline-section" key={`${section.heading}-${index}`}>
                  <strong>
                    {index + 1}. {section.heading}
                  </strong>
                  {section.points.length > 0 && (
                    <ul>
                      {section.points.map((point) => (
                        <li key={point}>{point}</li>
                      ))}
                    </ul>
                  )}
                </div>
              ))}
            </>
          ) : (
            <p className="hint">尚未获得提纲。可以关闭后重试。</p>
          )}
        </Modal>
      )}

      {citeOpen && (
        <Modal
          title="引用已收藏来源"
          description="引用会写成 Markdown 链接；仅题录或仅摘要的来源会在参考文献中标注，不冒充已读全文。"
          onClose={() => setCiteOpen(false)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setCiteOpen(false)}>
                关闭
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={sources.length === 0}
                onClick={insertReferences}
              >
                插入参考文献
              </button>
            </>
          }
        >
          {citeError && <div className="canvas-error">{citeError}</div>}
          {citeLoading ? (
            <p className="muted">正在读取已收藏来源…</p>
          ) : sources.length === 0 ? (
            <p className="hint">
              当前节点还没有收藏来源。先在下方“文献检索”中检索并收藏，再回到这里插入引用。
            </p>
          ) : (
            <div className="citation-list">
              {sources.map((source) => (
                <div className="citation-row" key={source.id}>
                  <div className="citation-body">
                    <strong>{source.title}</strong>
                    <span className="muted">
                      {citationLabel(source)} · {source.providerName}
                    </span>
                    <span className="tag tag--muted">
                      {COVERAGE_LABELS[source.coverage] ?? source.coverage}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="btn btn--compact"
                    onClick={() => insertAtCursor(citationMarkdown(source))}
                  >
                    插入
                  </button>
                </div>
              ))}
            </div>
          )}
        </Modal>
      )}

      {aiOpen && (
        <Modal
          title="AI 修订报告"
          description={
            proposal
              ? "AI 只提供局部替换建议；勾选后才会写入正文，未选中的内容保持原样。"
              : "说明希望如何修订。AI 会返回逐项局部修改，而不是重写整篇。"
          }
          onClose={() => setAiOpen(false)}
          footer={
            proposal ? (
              <>
                <button type="button" className="btn" onClick={() => setAiOpen(false)}>
                  取消
                </button>
                <button
                  type="button"
                  className="btn btn--primary"
                  disabled={applying || accepted.size === 0}
                  onClick={() => void applySelected()}
                >
                  {applying ? "应用中…" : `应用选中修改（${accepted.size}）`}
                </button>
              </>
            ) : (
              <>
                <button type="button" className="btn" onClick={() => setAiOpen(false)}>
                  取消
                </button>
                <button
                  type="button"
                  className="btn btn--primary"
                  disabled={aiLoading || !instruction.trim()}
                  onClick={() => void generateChanges()}
                >
                  {aiLoading ? "生成中…" : "生成修改建议"}
                </button>
              </>
            )
          }
        >
          {aiError && <div className="canvas-error">{aiError}</div>}

          {!proposal ? (
            <label className="form-field">
              <span>修订要求</span>
              <textarea
                value={instruction}
                placeholder="例如：把结论写得更谨慎，明确区分摘要级证据与已核实全文。"
                onChange={(event) => setInstruction(event.target.value)}
              />
              <span className="hint">
                建议先保存当前版本。生成提案后，如果正文又被修改，应用时会提示版本冲突。
              </span>
            </label>
          ) : (
            <>
              <p className="muted">{proposal.summary}</p>
              <div className="report-change-list">
                {proposal.changes.map((change) => (
                  <div className="report-change-row" key={change.id}>
                    <input
                      type="checkbox"
                      checked={accepted.has(change.id)}
                      aria-label={`接受修改 ${change.id}`}
                      onChange={() => toggleChange(change.id)}
                    />
                    <div className="report-change-body">
                      <p className="report-change-label">原段落</p>
                      <p className="oldtext">{change.oldText}</p>
                      <p className="report-change-label">建议修改</p>
                      <p className="newtext">{change.newText}</p>
                      {change.reason && (
                        <p className="muted">理由：{change.reason}</p>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </Modal>
      )}
    </div>
  );
}

const COVERAGE_LABELS: Record<string, string> = {
  metadata_only: "仅题录",
  abstract_only: "仅摘要",
  fulltext: "已获全文",
  unavailable: "不可获取",
};

function citationLabel(source: SavedSource): string {
  const first = source.authors[0]?.trim();
  const authors = first ? (source.authors.length > 1 ? `${first} 等` : first) : "佚名";
  return `${authors}, ${source.year ?? "年份未知"}`;
}

function citationLink(source: SavedSource): string | null {
  if (source.doi) {
    return source.doi.startsWith("http") ? source.doi : `https://doi.org/${source.doi}`;
  }
  return source.url;
}

function citationMarkdown(source: SavedSource): string {
  const label = citationLabel(source);
  const link = citationLink(source);
  return link ? `[${label}](${link})` : label;
}

function referenceEntry(source: SavedSource, index: number): string {
  const authors = source.authors.length > 0 ? source.authors.join(", ") : "佚名";
  const venue = source.venue ? `. ${source.venue}` : "";
  const year = source.year ?? "年份未知";
  const link = citationLink(source);
  const coverage = source.coverage === "fulltext"
    ? ""
    : `（${COVERAGE_LABELS[source.coverage] ?? source.coverage}）`;
  return `${index}. ${authors}. ${source.title}${venue}, ${year}. ${link ?? ""}${coverage}`;
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
