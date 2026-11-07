import { useEffect, useState } from "react";

import { ChatPanel, type QuickAction } from "../../components/ChatPanel";
import { Modal } from "../../components/Modal";
import { TaskStreamView } from "../../components/TaskStreamView";
import {
  getGlobalSettings,
  getResearchDefinition,
  saveResearchDefinition,
  suggestResearchDefinition,
  type ResearchDefinition,
  type ResearchDefinitionContent,
  type SettingsView,
  type WorkspaceInfo,
} from "../../lib/ipc";
import { useTaskStream } from "../../lib/useTaskStream";
import { resolveConfiguredModel } from "../../lib/modelReadiness";

interface TopicPageProps {
  workspace: WorkspaceInfo | null;
  onOpenFolder: () => void;
  onOpenSettings: () => void;
}

const SCOPE_ACTIONS: Array<{ id: string; title: string; description: string; prompt: string }> = [
  {
    id: "empirical",
    title: "有数据，做实证",
    description: "已有可分析的结构化数据，先锁定对象、核心问题与数据口径。",
    prompt:
      "我已有可分析的结构化数据，研究类型偏向实证检验。请根据当前对话锁定研究对象、一个核心问题和数据口径；只输出一个研究问题、目标、范围与方法、预期成果，不要继续给备选方案。如果缺少一个关键信息，只问我一个问题。",
  },
  {
    id: "method",
    title: "有数据，做方法",
    description: "重点是模型、指标或分析框架，而不是泛泛综述。",
    prompt:
      "我已有数据，研究类型偏向方法建模或指标构建。请根据当前对话收敛出一个方法学问题、现有方法的缺口、可检验的改进点和评估口径；只输出一个可执行方案，不要再扩展备选题目。",
  },
  {
    id: "policy",
    title: "政策文本/文献比较",
    description: "没有一手数据，做规范、政策或文献的结构化比较。",
    prompt:
      "我主要拥有政策文本、制度文件或文献材料，研究类型偏向规范/政策比较。请根据当前对话锁定比较对象、分析维度和可辩护的核心问题；只输出一个研究设计，不要再列新的选题方向。",
  },
  {
    id: "case",
    title: "访谈/问卷，做机制",
    description: "以案例、访谈或问卷为主，强调机制与解释。",
    prompt:
      "我主要使用访谈、问卷或案例材料，研究类型偏向机制解释。请根据当前对话收敛出案例边界、访谈/问卷对象和一个可检验的机制问题；只输出一个研究设计，不要继续扩展备选题目。",
  },
];

const RESEARCH_ACTIONS: QuickAction[] = [
  {
    id: "narrow",
    title: "收敛核心问题",
    description: "只给一个推荐问题，不再列多个备选。",
    prompt:
      "请基于当前对话和已有研究定义，只给出一个推荐的核心研究问题，不要列多个备选；说明为什么这样收敛，以及还缺哪一个关键信息。",
  },
  {
    id: "evaluate",
    title: "评估创新与风险",
    description: "指出创新点、可行性和最可能被质疑的地方。",
    prompt:
      "请评估这个选题的创新性与可行性，列出最可能被评审质疑的 3 个点，并给出对应的补强办法。",
  },
  {
    id: "literature",
    title: "检索关键文献",
    description: "实际检索文献，标注覆盖级别与可复用方法。",
    prompt:
      "请检索该方向近五年的关键文献，列出 5 条，标注 DOI、覆盖级别、核心结论与可复用方法；没有全文时不要假装读过。",
    deep: true,
  },
  {
    id: "design",
    title: "形成研究设计",
    description: "整理成一个可采纳的推荐版本，不再扩展新方案。",
    prompt:
      "请基于当前研究定义，只整理出一个推荐版本的研究问题、目标、范围、方法、预期成果与主要风险，不要新增备选方案；信息不足时只指出一个缺口。",
  },
];

export function TopicPage({
  workspace,
  onOpenFolder,
  onOpenSettings,
}: TopicPageProps) {
  const [settingsView, setSettingsView] = useState<SettingsView | null>(null);
  const [loadingSettings, setLoadingSettings] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [definition, setDefinition] = useState<ResearchDefinition | null>(null);
  const [loadingDefinition, setLoadingDefinition] = useState(false);
  const [definitionError, setDefinitionError] = useState<string | null>(null);
  const [editOpen, setEditOpen] = useState(false);
  const [savingDefinition, setSavingDefinition] = useState(false);
  const [extractOpen, setExtractOpen] = useState(false);
  const [extractLoading, setExtractLoading] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [extractSummary, setExtractSummary] = useState<string | null>(null);
  const [scopePrompt, setScopePrompt] = useState<{
    id: number;
    text: string;
    deep?: boolean;
  } | null>(null);
  const definitionStream = useTaskStream(
    extractOpen && workspace ? `definition:${workspace.rootNodeId}` : null,
  );
  const [form, setForm] = useState({
    question: "",
    goal: "",
    scope: "",
    approach: "",
    expected: "",
  });

  useEffect(() => {
    if (!workspace) {
      return;
    }

    let cancelled = false;
    setLoadingSettings(true);
    getGlobalSettings()
      .then((view) => {
        if (!cancelled) {
          setSettingsView(view);
          setSettingsError(null);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setSettingsError(toMessage(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingSettings(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [workspace?.projectId]);

  useEffect(() => {
    if (!workspace) {
      return;
    }

    let cancelled = false;
    setLoadingDefinition(true);
    getResearchDefinition()
      .then((value) => {
        if (!cancelled) {
          setDefinition(value);
          setDefinitionError(null);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setDefinitionError(toMessage(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingDefinition(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [workspace?.projectId]);

  const openDefinitionEditor = () => {
    const content = definition?.content;
    setForm({
      question: content?.question ?? "",
      goal: content?.goal ?? "",
      scope: content?.scope ?? "",
      approach: content?.approach ?? "",
      expected: (content?.expectedOutputs ?? []).join("\n"),
    });
    setEditOpen(true);
  };

  const extractDefinition = async () => {
    if (!workspace || extractLoading) {
      return;
    }
    setExtractOpen(true);
    setExtractLoading(true);
    setExtractError(null);
    setExtractSummary(null);
    try {
      const draft = await suggestResearchDefinition(workspace.rootNodeId);
      setForm({
        question: draft.content.question ?? "",
        goal: draft.content.goal ?? "",
        scope: draft.content.scope ?? "",
        approach: draft.content.approach ?? "",
        expected: draft.content.expectedOutputs.join("\n"),
      });
      setExtractSummary(draft.summary);
      setExtractOpen(false);
      setEditOpen(true);
    } catch (cause) {
      setExtractError(toMessage(cause));
    } finally {
      setExtractLoading(false);
    }
  };

  const submitDefinition = async () => {
    const expectedOutputs = form.expected
      .split(/\r?\n|,/)
      .map((item) => item.trim())
      .filter(Boolean);
    const content: ResearchDefinitionContent = {
      question: form.question.trim() || null,
      goal: form.goal.trim() || null,
      scope: form.scope.trim() || null,
      approach: form.approach.trim() || null,
      expectedOutputs,
    };
    const confirmedFields: string[] = [];
    if (content.question) confirmedFields.push("question");
    if (content.goal) confirmedFields.push("goal");
    if (content.scope) confirmedFields.push("scope");
    if (content.approach) confirmedFields.push("approach");
    if (expectedOutputs.length) confirmedFields.push("expectedOutputs");

    setSavingDefinition(true);
    try {
      const saved = await saveResearchDefinition(
        content,
        confirmedFields,
        definition?.revision ?? 0,
      );
      setDefinition(saved);
      setDefinitionError(null);
      setEditOpen(false);
    } catch (cause) {
      setDefinitionError(toMessage(cause));
    } finally {
      setSavingDefinition(false);
    }
  };

  if (!workspace) {
    return (
      <section className="page page--centered">
        <div className="empty-state">
          <div className="empty-icon" aria-hidden="true">
            ◇
          </div>
          <h1>先打开一个研究项目</h1>
          <p className="empty-lead">
            选题研究需要一个本地 Workspace 来保存研究定义、对话与后续成果。
          </p>
          <button type="button" className="btn btn--primary" onClick={onOpenFolder}>
            打开本地文件夹
          </button>
        </div>
      </section>
    );
  }

  const readiness = settingsView
    ? resolveConfiguredModel(settingsView, "research")
    : null;

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <div className="eyebrow">选题研究</div>
          <h1>锤炼研究方向</h1>
          <p className="page-subtitle">
            从一个想法开始，科研伙伴通过对话帮助你收敛研究问题、目标与边界。
          </p>
        </div>
        <div className="page-head-actions">
          <button type="button" className="btn btn--compact" onClick={onOpenSettings}>
            模型设置
          </button>
        </div>
      </header>

      <div className="topic-layout">
        <article className="card card--pad topic-chat">
          <div className="section-head">
            <h2 className="section-title">科研伙伴</h2>
            {readiness?.ready && readiness.provider && readiness.model && (
              <span className="tag tag--success">
                {readiness.provider.name} · {readiness.model.displayName}
              </span>
            )}
          </div>

          {loadingSettings && <p className="muted">正在检查模型配置…</p>}

          {!loadingSettings && settingsError && (
            <div className="topic-unavailable">
              <span className="tag tag--warning">无法读取模型配置</span>
              <p className="muted">{settingsError}</p>
              <button type="button" className="btn btn--compact" onClick={onOpenSettings}>
                前往设置
              </button>
            </div>
          )}

          {!loadingSettings && !settingsError && readiness && !readiness.ready && (
            <div className="topic-unavailable">
              <span className="tag tag--warning">模型暂不可用</span>
              <p className="muted">{readiness.reason}</p>
              <button
                type="button"
                className="btn btn--primary btn--compact"
                onClick={onOpenSettings}
              >
                去配置模型
              </button>
            </div>
          )}

          {!loadingSettings && !settingsError && readiness?.ready && (
            <>
              <p className="muted">
                对话使用总课题节点上下文。没有证据时模型会明确说明，不会伪造文献、DOI 或实验结果。
              </p>
              <div className="scope-cards">
                <div className="section-head">
                  <span className="muted">先锁定研究定位，再收敛题目</span>
                </div>
                <div className="quick-actions">
                  {SCOPE_ACTIONS.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      className="quick-action-card"
                      onClick={() =>
                        setScopePrompt({
                          id: Date.now(),
                          text: action.prompt,
                          deep: false,
                        })
                      }
                    >
                      <strong>{action.title}</strong>
                      <span>{action.description}</span>
                    </button>
                  ))}
                </div>
              </div>

              <ChatPanel
                defaultDeepResearch={false}
                externalPrompt={scopePrompt}
                quickActions={RESEARCH_ACTIONS}
                nodeId={workspace.rootNodeId}
                className="topic-chat-panel"
                placeholder="例如：我想比较不同城市绿地规划方式对局地热环境的影响……"
                emptyText="告诉我你关心的研究现象、问题或初步想法，我们从这里开始收敛研究方向。"
              />
            </>
          )}
        </article>

        <aside className="card card--pad definition-panel">
          <div className="section-head">
            <h2 className="section-title">研究定义</h2>
            {definition && definition.revision > 0 && (
              <span className="tag tag--muted">v{definition.revision}</span>
            )}
            <button
              type="button"
              className="btn btn--compact"
              disabled={extractLoading}
              onClick={() => void extractDefinition()}
            >
              {extractLoading ? "提取中…" : "从对话提取"}
            </button>
            <button type="button" className="btn btn--compact" onClick={openDefinitionEditor}>
              编辑
            </button>
          </div>

          {loadingDefinition && <p className="muted">正在读取研究定义…</p>}
          {definitionError && <div className="canvas-error">{definitionError}</div>}

          {!loadingDefinition && (
            <dl className="definition-fields">
              <div>
                <dt>研究问题</dt>
                <dd className={definition?.content.question ? "" : "muted"}>
                  {definition?.content.question || "待确认"}
                </dd>
              </div>
              <div>
                <dt>研究目标</dt>
                <dd className={definition?.content.goal ? "" : "muted"}>
                  {definition?.content.goal || "待确认"}
                </dd>
              </div>
              <div>
                <dt>范围与边界</dt>
                <dd className={definition?.content.scope ? "" : "muted"}>
                  {definition?.content.scope || "待确认"}
                </dd>
              </div>
              <div>
                <dt>方法方向</dt>
                <dd className={definition?.content.approach ? "" : "muted"}>
                  {definition?.content.approach || "待确认"}
                </dd>
              </div>
              <div>
                <dt>预期成果</dt>
                <dd
                  className={
                    definition?.content.expectedOutputs.length ? "" : "muted"
                  }
                >
                  {definition?.content.expectedOutputs.length
                    ? definition.content.expectedOutputs.join("、")
                    : "待确认"}
                </dd>
              </div>
            </dl>
          )}
          <p className="hint">
            研究定义默认按需展开，不要求一次填写完整。保存后会同步到项目概况与总课题节点。
            如果对话已经收敛，可点“从对话提取”把研究问题、目标、范围、方法和预期成果一次填入。
          </p>
        </aside>
      </div>

      {extractOpen && (
        <Modal
          title="从对话提取研究定义"
          description="只提取对话中已经出现的信息；缺失项会保留为空，由你确认后保存。"
          onClose={() => {
            if (!extractLoading) {
              setExtractOpen(false);
            }
          }}
          footer={
            <button
              type="button"
              className="btn"
              disabled={extractLoading}
              onClick={() => setExtractOpen(false)}
            >
              {extractLoading ? "提取中…" : "关闭"}
            </button>
          }
        >
          {extractError && <div className="canvas-error">{extractError}</div>}
          <TaskStreamView
            reasoning={definitionStream.reasoning}
            content={definitionStream.content}
            active={definitionStream.active}
            label="正在从对话中收敛研究定义…"
          />
        </Modal>
      )}

      {editOpen && (
        <Modal
          title="编辑研究定义"
          description="这些内容会作为项目与总课题节点共享的唯一研究定义。"
          onClose={() => setEditOpen(false)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setEditOpen(false)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={savingDefinition}
                onClick={() => void submitDefinition()}
              >
                {savingDefinition ? "保存中…" : "保存定义"}
              </button>
            </>
          }
        >
          {extractSummary && (
            <p className="hint">提取依据：{extractSummary}</p>
          )}
          <label className="form-field">
            <span>研究问题</span>
            <textarea
              value={form.question}
              placeholder="你希望回答的核心问题是什么？"
              onChange={(event) =>
                setForm((current) => ({ ...current, question: event.target.value }))
              }
            />
          </label>
          <label className="form-field">
            <span>研究目标</span>
            <textarea
              value={form.goal}
              placeholder="希望达成什么可检验的目标？"
              onChange={(event) =>
                setForm((current) => ({ ...current, goal: event.target.value }))
              }
            />
          </label>
          <label className="form-field">
            <span>范围与边界</span>
            <textarea
              value={form.scope}
              placeholder="研究对象、时间、空间或数据范围，以及明确不做什么。"
              onChange={(event) =>
                setForm((current) => ({ ...current, scope: event.target.value }))
              }
            />
          </label>
          <label className="form-field">
            <span>方法方向</span>
            <textarea
              value={form.approach}
              placeholder="拟采用的文献、数据、实验或比较方法。"
              onChange={(event) =>
                setForm((current) => ({ ...current, approach: event.target.value }))
              }
            />
          </label>
          <label className="form-field">
            <span>预期成果（每行一项）</span>
            <textarea
              value={form.expected}
              placeholder={"阶段性研究报告\n可复核的数据分析结果"}
              onChange={(event) =>
                setForm((current) => ({ ...current, expected: event.target.value }))
              }
            />
          </label>
        </Modal>
      )}
    </section>
  );
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "读取模型配置失败，请稍后重试。";
}
