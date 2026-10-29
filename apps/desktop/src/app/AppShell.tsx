import type { ReactNode } from "react";

export type AppView = "overview" | "topic" | "canvas" | "assets" | "settings";

interface AppShellProps {
  view: AppView;
  onViewChange: (view: AppView) => void;
  workspaceName: string | null;
  hasWorkspace: boolean;
  busy: boolean;
  error: string | null;
  onDismissError: () => void;
  children: ReactNode;
}

const NAV_ITEMS: Array<{ id: AppView; label: string; icon: ReactNode }> = [
  { id: "overview", label: "项目", icon: <Icon name="home" /> },
  { id: "topic", label: "选题", icon: <Icon name="topic" /> },
  { id: "canvas", label: "画布", icon: <Icon name="canvas" /> },
  { id: "assets", label: "资产", icon: <Icon name="assets" /> },
];

export function AppShell({
  view,
  onViewChange,
  workspaceName,
  hasWorkspace,
  busy,
  error,
  onDismissError,
  children,
}: AppShellProps) {
  return (
    <div className="app-shell">
      <header className="titlebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            澄
          </span>
          <span className="brand-name">澄思学术</span>
        </div>
        <div className="titlebar-divider" aria-hidden="true" />
        <span className="crumb">{workspaceName ?? "未打开项目"}</span>
        <div className="titlebar-spacer" />
        {busy && <span className="status-pill status-pill--busy">处理中…</span>}
        <span className={`status-pill ${hasWorkspace ? "status-pill--saved" : ""}`}>
          {hasWorkspace ? "本地已保存" : "本地工作区"}
        </span>
      </header>

      {error && (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button type="button" className="btn btn--ghost btn--compact" onClick={onDismissError}>
            关闭
          </button>
        </div>
      )}

      <div className="app-body">
        <nav className="rail" aria-label="全局导航">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`nav-item ${view === item.id ? "nav-item--active" : ""}`}
              aria-current={view === item.id ? "page" : undefined}
              onClick={() => onViewChange(item.id)}
            >
              <span className="nav-icon">{item.icon}</span>
              <span className="nav-label">{item.label}</span>
            </button>
          ))}
          <div className="rail-spacer" />
          <button
            type="button"
            className={`nav-item ${view === "settings" ? "nav-item--active" : ""}`}
            aria-current={view === "settings" ? "page" : undefined}
            onClick={() => onViewChange("settings")}
          >
            <span className="nav-icon">
              <Icon name="settings" />
            </span>
            <span className="nav-label">设置</span>
          </button>
        </nav>

        <main className="main-area">{children}</main>
      </div>
    </div>
  );
}

type IconName = "home" | "topic" | "canvas" | "assets" | "settings";

function Icon({ name }: { name: IconName }) {
  const paths: Record<IconName, ReactNode> = {
    home: <path d="M3 10.5 12 3l9 7.5M5 9.5V21h14V9.5" />,
    topic: <path d="m12 3 7 9-7 9-7-9 7-9Z" />,
    canvas: <path d="M12 3 21 8v8l-9 5-9-5V8l9-5Z" />,
    assets: <path d="M4 7h16M4 12h16M4 17h16" />,
    settings: (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2-1.2L14.2 3h-4.4l-.4 2.7a7 7 0 0 0-2 1.2l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5 12c0 .4 0 .8.1 1.2l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2 1.2l.4 2.7h4.4l.4-2.7a7 7 0 0 0 2-1.2l2.3 1 2-3.4-2-1.5c.1-.4.1-.8.1-1.2Z" />
      </>
    ),
  };

  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {paths[name]}
    </svg>
  );
}
