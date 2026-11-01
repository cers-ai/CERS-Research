import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { Modal } from "../../components/Modal";
import type { UpdateInfo } from "../../lib/ipc";

interface UpdateBannerProps {
  info: UpdateInfo;
  onDismiss: () => void;
  onSkip: (version: string) => Promise<void>;
}

export function UpdateBanner({ info, onDismiss, onSkip }: UpdateBannerProps) {
  const [open, setOpen] = useState(false);
  const [skipping, setSkipping] = useState(false);
  const [opening, setOpening] = useState(false);

  const download = async () => {
    if (!info.releaseUrl) {
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

  const skip = async () => {
    setSkipping(true);
    try {
      await onSkip(info.latestVersion);
      setOpen(false);
      onDismiss();
    } finally {
      setSkipping(false);
    }
  };

  return (
    <>
      <div className="update-banner" role="status">
        <span className="tag tag--success">新版本 v{info.latestVersion}</span>
        <span className="update-banner-text">
          当前 v{info.currentVersion}，发现可更新版本。
        </span>
        <button type="button" className="btn btn--compact" onClick={() => setOpen(true)}>
          查看更新
        </button>
        <button
          type="button"
          className="btn btn--ghost btn--compact"
          onClick={onDismiss}
          aria-label="稍后提醒"
        >
          稍后
        </button>
      </div>

      {open && (
        <Modal
          title={`版本更新 v${info.latestVersion}`}
          description={`当前版本 v${info.currentVersion}${
            info.publishedAt ? ` · 发布于 ${formatDate(info.publishedAt)}` : ""
          }`}
          onClose={() => setOpen(false)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setOpen(false)}>
                稍后提醒
              </button>
              <button
                type="button"
                className="btn btn--ghost"
                disabled={skipping}
                onClick={() => void skip()}
              >
                {skipping ? "处理中…" : "跳过此版本"}
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={!info.releaseUrl || opening}
                onClick={() => void download()}
              >
                {opening ? "打开中…" : "前往下载"}
              </button>
            </>
          }
        >
          {info.notes ? (
            <pre className="update-notes">{info.notes}</pre>
          ) : (
            <p className="muted">该版本没有提供更新说明。</p>
          )}
          <p className="hint">
            更新包需要手动下载安装；升级前建议在项目概况页创建一次项目备份。
          </p>
        </Modal>
      )}
    </>
  );
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}
