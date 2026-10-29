import type { WorkspaceInfo } from "../../lib/ipc";

interface TopicPageProps {
  workspace: WorkspaceInfo | null;
  onOpenFolder: () => void;
  onOpenSettings: () => void;
}

export function TopicPage({ workspace, onOpenFolder, onOpenSettings }: TopicPageProps) {
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

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <div className="eyebrow">选题研究</div>
          <h1>锤炼研究方向</h1>
          <p className="page-subtitle">项目概况、选题工作台与总课题节点共享同一份研究定义。</p>
        </div>
      </header>

      <div className="topic-layout">
        <article className="card card--pad topic-chat">
          <div className="chat-placeholder">
            <span className="tag tag--warning">模型服务尚未配置</span>
            <h2>对话式选题需要先连接一个模型服务</h2>
            <p className="muted">
              配置完成后，你可以从一句模糊想法开始，通过澄清卡逐步形成研究问题、目标、边界与预期成果。
              未配置模型时，不会产生任何模拟回复。
            </p>
            <button type="button" className="btn btn--primary" onClick={onOpenSettings}>
              前往模型设置
            </button>
          </div>
        </article>

        <aside className="card card--pad definition-panel">
          <div className="section-head">
            <h2 className="section-title">研究定义</h2>
            <span className="tag tag--muted">尚未形成</span>
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
          <p className="hint">研究定义默认按需展开，不要求一次填写完整。</p>
        </aside>
      </div>
    </section>
  );
}
