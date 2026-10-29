import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import type { RuntimeInfo } from "../lib/ipc";
import {
  closeWorkspace,
  getRuntimeInfo,
  isTauriRuntime,
  openWorkspace,
  runSidecarSelfCheck,
} from "../lib/ipc";
import { AppShell, type AppView } from "./AppShell";
import { OverviewPage } from "../features/project/OverviewPage";
import { TopicPage } from "../features/topic/TopicPage";
import { CanvasPage } from "../features/canvas/CanvasPage";
import { AssetsPage } from "../features/assets/AssetsPage";
import { SettingsPage } from "../features/settings/SettingsPage";

export function App() {
  const [view, setView] = useState<AppView>("overview");
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) {
      setError("当前在浏览器中运行。请使用 `npm run dev` 启动桌面应用。");
      return;
    }
    try {
      setRuntime(await getRuntimeInfo());
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleOpenFolder = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择研究文件夹",
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) {
        return;
      }
      const workspace = await openWorkspace(path);
      setRuntime((current) => (current ? { ...current, workspace } : current));
      setView("overview");
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const handleCloseWorkspace = useCallback(async () => {
    setBusy(true);
    try {
      await closeWorkspace();
      setRuntime((current) => (current ? { ...current, workspace: null } : current));
      setView("overview");
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const handleSidecarCheck = useCallback(async () => {
    setBusy(true);
    try {
      const sidecar = await runSidecarSelfCheck();
      setRuntime((current) => (current ? { ...current, sidecar } : current));
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const workspace = runtime?.workspace ?? null;

  return (
    <AppShell
      view={view}
      onViewChange={setView}
      workspaceName={workspace ? workspace.alias || workspace.name : null}
      hasWorkspace={workspace !== null}
      busy={busy}
      error={error}
      onDismissError={() => setError(null)}
    >
      {view === "overview" && (
        <OverviewPage
          runtime={runtime}
          workspace={workspace}
          busy={busy}
          onOpenFolder={() => void handleOpenFolder()}
          onOpenSettings={() => setView("settings")}
          onOpenCanvas={() => setView("canvas")}
          onOpenTopic={() => setView("topic")}
          onOpenAssets={() => setView("assets")}
        />
      )}
      {view === "topic" && (
        <TopicPage
          workspace={workspace}
          onOpenFolder={() => void handleOpenFolder()}
          onOpenSettings={() => setView("settings")}
        />
      )}
      {view === "canvas" && (
        <CanvasPage
          workspace={workspace}
          onOpenFolder={() => void handleOpenFolder()}
        />
      )}
      {view === "assets" && <AssetsPage workspace={workspace} />}
      {view === "settings" && (
        <SettingsPage
          runtime={runtime}
          workspace={workspace}
          busy={busy}
          onRefreshSidecar={() => void handleSidecarCheck()}
          onCloseWorkspace={() => void handleCloseWorkspace()}
        />
      )}
    </AppShell>
  );
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "发生未知错误，请查看应用日志。";
}
