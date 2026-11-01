import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  createProjectBackup,
  getResearchDefinition,
  restoreProjectBackup,
  type ResearchDefinition,
  type RuntimeInfo,
  type WorkspaceInfo,
} from "../../lib/ipc";

interface OverviewPageProps {
  runtime: RuntimeInfo | null;
  workspace: WorkspaceInfo | null;
  busy: boolean;
  onOpenFolder: () => void;
  onOpenSettings: () => void;
  onOpenCanvas: () => void;
  onOpenTopic: () => void;
  onOpenAssets: () => void;
  onCloseWorkspace: () => void;
  onRestored: (workspace: WorkspaceInfo) => void;
}

export function OverviewPage({
  runtime,
  workspace,
  busy,
  onOpenFolder,
  onOpenSettings,
  onOpenCanvas,
  onOpenTopic,
  onOpenAssets,
  onCloseWorkspace,
  onRestored,
}: OverviewPageProps) {
  const [definition, setDefinition] = useState<ResearchDefinition | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [backupNotice, setBackupNotice] = useState<string | null>(null);

  useEffect(() => {
    if (!workspace) {
      return;
    }
    let cancelled = false;
    getResearchDefinition()
      .then((value) => {
        if (!cancelled) {
          setDefinition(value);
        }
      })
      .catch(() => {
        // 概况页读取失败时保留空态，不阻塞其他项目信息。
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
            ◈
          </div>
          <h1>从一个本地文件夹开始</h1>
          <p className="empty-lead">
            选择一个研究文件夹作为 Workspace。原始文件保留在原位置，澄思只读取并为它们建立逻辑索引。
          </p>
          <div className="empty-actions">
            <button type="button" className="btn btn--primary" disabled={busy} onClick={onOpenFolder}>
              {busy ? "正在打开…" : "打开本地文件夹"}
            </button>
            <button type="button" className="btn" onClick={onOpenSettings}>
              查看运行时状态
            </button>
          </div>
          <ul className="empty-points">
            <li>文件夹名默认为项目名，之后可以设置别名。</li>
            <li>不需要先整理目录，也不需要填写项目表单。</li>
            <li>没有配置模型时，本地项目、画布与文档编辑仍可用。</li>
          </ul>
          {runtime && <SidecarLine runtime={runtime} />}
        </div>
      </section>
    );
  }

  const displayName = workspace.alias || workspace.name;

  const handleBackup = async () => {
    setBackupBusy(true);
    setBackupNotice(null);
    try {
      const path = await createProjectBackup();
      setBackupNotice(`备份已生成：${path}`);
    } catch (cause) {
      setBackupNotice(toMessage(cause));
    } finally {
      setBackupBusy(false);
    }
  };

  const handleRestore = async () => {
    setBackupBusy(true);
    setBackupNotice(null);
    try {
      const archive = await open({
        title: "选择项目备份",
        multiple: false,
        directory: false,
        filters: [{ name: "项目备份", extensions: ["zip"] }],
      });
      const archivePath = Array.isArray(archive) ? archive[0] : archive;
      if (!archivePath) {
        return;
      }
      const folder = await open({
        title: "选择空的恢复文件夹",
        multiple: false,
        directory: true,
      });
      const destination = Array.isArray(folder) ? folder[0] : folder;
      if (!destination) {
        return;
      }
      const restored = await restoreProjectBackup(String(archivePath), String(destination));
      onRestored(restored);
      setBackupNotice(`已恢复到 ${restored.workspacePath}`);
    } catch (cause) {
      setBackupNotice(toMessage(cause));
    } finally {
      setBackupBusy(false);
    }
  };

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <div className="eyebrow">项目概况</div>
          <h1>{displayName}</h1>
          <p className="page-subtitle">{workspace.workspacePath}</p>
        </div>
        <div className="page-head-actions">
          <button type="button" className="btn" onClick={onOpenTopic}>
            进入选题
          </button>
          <button type="button" className="btn btn--primary" onClick={onOpenCanvas}>
            打开科研图
          </button>
          <button type="button" className="btn" disabled={busy} onClick={onOpenFolder}>
            切换文件夹
          </button>
          <button type="button" className="btn btn--ghost" disabled={busy} onClick={onCloseWorkspace}>
            关闭项目
          </button>
        </div>
      </header>

      <div className="overview-grid">
        <article className="card card--pad">
          <h2 className="section-title">项目理解与当前目标</h2>
          <p className="muted">
            这个项目已经建立本地工作区与总课题节点。研究定义尚未形成，需要你在选题对话中确认研究问题、目标与边界。
          </p>
          <div className="definition-placeholder">
            {definition && (definition.content.goal || definition.content.question) ? (
              <>
                <span className="tag tag--success">
                  研究定义 v{definition.revision}
                </span>
                {definition.content.question && (
                  <p>
                    <strong>研究问题：</strong>
                    {definition.content.question}
                  </p>
                )}
                {definition.content.goal && (
                  <p>
                    <strong>研究目标：</strong>
                    {definition.content.goal}
                  </p>
                )}
              </>
            ) : (
              <>
                <span className="tag tag--muted">尚未形成研究定义</span>
                <p>
                  项目概况、选题工作台与总课题节点读取同一份研究定义，不会出现三个不同版本。
                </p>
              </>
            )}
          </div>
          <div className="row">
            <button type="button" className="btn" onClick={onOpenTopic}>
              开始锤炼方向
            </button>
            <button type="button" className="btn btn--ghost" onClick={onOpenCanvas}>
              先查看画布
            </button>
          </div>
        </article>

        <div className="stack">
          <article className="card card--pad">
            <div className="section-head">
              <h2 className="section-title">继续上次研究</h2>
              <span className="tag tag--warning">尚未确定推荐</span>
            </div>
            <p className="muted">
              项目刚建立，还没有真实任务、成果或证据缺口可以作为推荐依据。系统不会虚构研究进度。
            </p>
            <button type="button" className="btn btn--compact" onClick={onOpenTopic}>
              先明确研究目标
            </button>
          </article>

          <article className="card card--pad">
            <h2 className="section-title">工作区</h2>
            <p className="muted">
              当前项目：{workspace.workspacePath}。关闭项目只会释放当前工作区，不会删除任何文件；
              之后的扫描、对话和报告仍保存在原文件夹的 <code>.cers-data</code> 中。
            </p>
            <div className="row">
              <button type="button" className="btn btn--compact" onClick={onOpenFolder}>
                打开其他文件夹
              </button>
              <button type="button" className="btn btn--ghost btn--compact" onClick={onCloseWorkspace}>
                关闭当前项目
              </button>
            </div>
          </article>

          <article className="card card--pad">
            <h2 className="section-title">已有资料</h2>
            <p className="muted">
              在研究资产库中扫描 Workspace。扫描只读取元数据并建立逻辑引用，不会移动或复制原始文件。
            </p>
            <button type="button" className="btn btn--compact" onClick={onOpenAssets}>
              打开研究资产库
            </button>
            <div className="meta-list">
              <div>
                <span className="meta-label">总课题节点</span>
                <span className="mono">{workspace.rootNodeId}</span>
              </div>
              <div>
                <span className="meta-label">项目创建时间</span>
                <span>{formatTimestamp(workspace.createdAt)}</span>
              </div>
              <div>
                <span className="meta-label">项目数据版本</span>
                <span>schema {workspace.schemaVersion}</span>
              </div>
            </div>
          </article>

          <article className="card card--pad">
            <h2 className="section-title">备份与恢复</h2>
            <p className="muted">
              备份包含项目配置、数据库快照、研究输出与 Workspace 文件。恢复时需要选择一个空的
              新文件夹；恢复后资产、节点与报告引用保持一致。
            </p>
            <div className="row">
              <button
                type="button"
                className="btn btn--compact"
                disabled={backupBusy}
                onClick={() => void handleBackup()}
              >
                {backupBusy ? "处理中…" : "创建项目备份"}
              </button>
              <button
                type="button"
                className="btn btn--ghost btn--compact"
                disabled={backupBusy}
                onClick={() => void handleRestore()}
              >
                从备份恢复…
              </button>
            </div>
            {backupNotice && <p className="hint">{backupNotice}</p>}
          </article>

          {runtime && (
            <article className="card card--pad">
              <div className="section-head">
                <h2 className="section-title">本地运行时</h2>
                <SidecarTag ok={runtime.sidecar.ok} />
              </div>
              <p className="muted">{runtime.sidecar.message}</p>
              <button type="button" className="btn btn--compact" onClick={onOpenSettings}>
                查看设置
              </button>
            </article>
          )}
        </div>
      </div>
    </section>
  );
}

function SidecarLine({ runtime }: { runtime: RuntimeInfo }) {
  return (
    <div className="runtime-line">
      <SidecarTag ok={runtime.sidecar.ok} />
      <span className="muted">{runtime.sidecar.message}</span>
    </div>
  );
}

function SidecarTag({ ok }: { ok: boolean }) {
  return (
    <span className={`tag ${ok ? "tag--success" : "tag--warning"}`}>
      {ok ? "Python 运行时可用" : "Python 运行时未就绪"}
    </span>
  );
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "备份操作失败，请稍后重试。";
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}
