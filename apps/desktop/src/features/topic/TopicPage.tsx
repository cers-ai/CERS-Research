import { useEffect, useState } from "react";

import { ChatPanel } from "../../components/ChatPanel";
import {
  getGlobalSettings,
  type SettingsView,
  type WorkspaceInfo,
} from "../../lib/ipc";
import { resolveConfiguredModel } from "../../lib/modelReadiness";

interface TopicPageProps {
  workspace: WorkspaceInfo | null;
  onOpenFolder: () => void;
  onOpenSettings: () => void;
}

export function TopicPage({
  workspace,
  onOpenFolder,
  onOpenSettings,
}: TopicPageProps) {
  const [settingsView, setSettingsView] = useState<SettingsView | null>(null);
  const [loadingSettings, setLoadingSettings] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);

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
              <ChatPanel
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
            <span className="tag tag--muted">持续完善</span>
          </div>
          <dl className="definition-fields">
            <div>
              <dt>研究问题</dt>
              <dd className="muted">待确认</dd>
            </div>
            <div>
              <dt>研究目标</dt>
              <dd className="muted">待确认</dd>
            </div>
            <div>
              <dt>范围与边界</dt>
              <dd className="muted">待确认</dd>
            </div>
            <div>
              <dt>预期成果</dt>
              <dd className="muted">待确认</dd>
            </div>
          </dl>
          <p className="hint">
            研究定义默认按需展开，不要求一次填写完整。后续确认的内容会同步到项目概况与总课题节点。
          </p>
        </aside>
      </div>
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
