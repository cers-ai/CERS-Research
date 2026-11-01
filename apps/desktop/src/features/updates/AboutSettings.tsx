import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  checkForUpdates,
  setAutoUpdateCheck,
  type GlobalSettings,
  type RuntimeInfo,
  type UpdateInfo,
} from "../../lib/ipc";

interface AboutSettingsProps {
  runtime: RuntimeInfo | null;
  settings: GlobalSettings;
  onSettingsSaved: () => void;
}

export function AboutSettings({ runtime, settings, onSettingsSaved }: AboutSettingsProps) {
  const [autoCheck, setAutoCheck] = useState(settings.updates?.autoCheck ?? true);
  const [checking, setChecking] = useState(false);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  useEffect(() => {
    setAutoCheck(settings.updates?.autoCheck ?? true);
  }, [settings.updates?.autoCheck]);

  const toggleAutoCheck = async (next: boolean) => {
    setAutoCheck(next);
    setError(null);
    try {
      await setAutoUpdateCheck(next);
      onSettingsSaved();
    } catch (cause) {
      setAutoCheck(!next);
      setError(toMessage(cause));
    }
  };

  const check = async () => {
    setChecking(true);
    setError(null);
    try {
      setInfo(await checkForUpdates(true));
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setChecking(false);
    }
  };

  const download = async () => {
    if (!info?.releaseUrl) {
      return;
    }
    setOpening(true);
    try {
      await openUrl(info.releaseUrl);
    } catch {
      window.open(info.releaseUrl, "_blank", "noopener,noreferrer");
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="about-settings">
      <article className="card card--pad">
        <div className="section-head">
          <h2 className="section-title">版本与更新</h2>
          <span className="tag tag--muted">v{runtime?.appVersion ?? "0.0.1"}</span>
        </div>
        <p className="muted">
          启动时会在后台检查 GitHub 正式发布版本，不阻塞使用。检查只读取版本号、更新说明与发布页链接；
          不会下载或安装任何文件。
        </p>

        <label className="form-field form-field--inline">
          <span>启动时自动检查更新</span>
          <input
            type="checkbox"
            checked={autoCheck}
            onChange={(event) => void toggleAutoCheck(event.target.checked)}
          />
        </label>

        <div className="row">
          <button
            type="button"
            className="btn btn--primary btn--compact"
            disabled={checking}
            onClick={() => void check()}
          >
            {checking ? "检查中…" : "立即检查更新"}
          </button>
          {info?.releaseUrl && (
            <button
              type="button"
              className="btn btn--ghost btn--compact"
              disabled={opening}
              onClick={() => void download()}
            >
              {opening ? "打开中…" : "打开发布页"}
            </button>
          )}
        </div>

        {error && <div className="canvas-error">{error}</div>}

        {info && (
          <div className="update-result">
            <div className="row">
              <span className={`tag ${info.hasUpdate ? "tag--warning" : "tag--success"}`}>
                {info.hasUpdate ? `可更新到 v${info.latestVersion}` : "已是最新版本"}
              </span>
              {info.skipped && <span className="tag tag--muted">已跳过该版本</span>}
              <span className="muted">
                当前 v{info.currentVersion} · 检查于 {formatDate(info.checkedAt)}
              </span>
            </div>
            {info.message && <p className="muted">{info.message}</p>}
            {info.hasUpdate && info.notes && (
              <>
                <h3 className="node-workspace-section">更新说明</h3>
                <pre className="update-notes">{info.notes}</pre>
              </>
            )}
          </div>
        )}
      </article>

      <article className="card card--pad">
        <h2 className="section-title">数据与隐私</h2>
        <p className="muted">
          项目文件保存在你选择的本地文件夹；模型密钥保存在操作系统凭据库。更新检查只访问公开的
          GitHub Release 接口，不会上传项目内容。自动检查可在上方关闭。
        </p>
      </article>
    </div>
  );
}

function formatDate(value: string): string {
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
  return "更新检查失败，请稍后重试。";
}
