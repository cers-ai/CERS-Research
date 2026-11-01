import { useCallback, useEffect, useState } from "react";

import { Modal } from "../../components/Modal";
import type {
  ConnectionTestResult,
  GlobalSettings,
  RuntimeInfo,
  SettingsView,
  WorkspaceInfo,
} from "../../lib/ipc";
import {
  getGlobalSettings,
  saveGlobalSettings,
  setSecret,
  testProvider,
  testSource,
} from "../../lib/ipc";
import { AgentSettings } from "./AgentSettings";
import { ProviderSettings } from "./ProviderSettings";
import { SourceSettings } from "./SourceSettings";
import { toMessage } from "./settingsTypes";

type SettingsTab = "models" | "academic" | "news" | "agent" | "network" | "runtime";

interface SettingsPageProps {
  runtime: RuntimeInfo | null;
  workspace: WorkspaceInfo | null;
  busy: boolean;
  onRefreshSidecar: () => void;
  onCloseWorkspace: () => void;
}

interface KeyTarget {
  kind: "model" | "source";
  id: string;
  name: string;
  hasKey: boolean;
}

const TABS: Array<[SettingsTab, string]> = [
  ["models", "模型服务"],
  ["academic", "学术来源"],
  ["news", "新闻资讯"],
  ["agent", "MCP 与技能"],
  ["network", "网络与隐私"],
  ["runtime", "运行时"],
];

export function SettingsPage({
  runtime,
  workspace,
  busy,
  onRefreshSidecar,
  onCloseWorkspace,
}: SettingsPageProps) {
  const [tab, setTab] = useState<SettingsTab>("models");
  const [view, setView] = useState<SettingsView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<
    Record<string, ConnectionTestResult>
  >({});
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [keyTarget, setKeyTarget] = useState<KeyTarget | null>(null);
  const [keyValue, setKeyValue] = useState("");
  const [keyBusy, setKeyBusy] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setView(await getGlobalSettings());
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!notice) {
      return;
    }
    const timer = window.setTimeout(() => setNotice(null), 2600);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const persist = useCallback(async (next: GlobalSettings) => {
    try {
      const updated = await saveGlobalSettings(next);
      setView(updated);
      setError(null);
      setNotice("设置已保存");
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, []);

  const runTest = useCallback(
    async (kind: "model" | "source", id: string) => {
      const key = `${kind}:${id}`;
      setTestingKey(key);
      try {
        const result =
          kind === "model" ? await testProvider(id) : await testSource(id);
        setTestResults((current) => ({ ...current, [key]: result }));
      } catch (cause) {
        setTestResults((current) => ({
          ...current,
          [key]: {
            ok: false,
            statusCode: null,
            latencyMs: 0,
            message: toMessage(cause),
            detail: null,
          },
        }));
      } finally {
        setTestingKey(null);
      }
    },
    [],
  );

  const submitKey = useCallback(async () => {
    if (!keyTarget) {
      return;
    }
    setKeyBusy(true);
    try {
      const hasKey = await setSecret(keyTarget.kind, keyTarget.id, keyValue);
      await load();
      setKeyTarget(null);
      setKeyValue("");
      setNotice(hasKey ? "密钥已保存到系统凭据库" : "密钥已清除");
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setKeyBusy(false);
    }
  }, [keyTarget, keyValue, load]);

  const handlers = {
    onPersist: (next: GlobalSettings) => persist(next),
    onTest: (kind: "model" | "source", id: string) => void runTest(kind, id),
    onSetKey: (
      kind: "model" | "source",
      id: string,
      name: string,
      hasKey: boolean,
    ) => {
      setKeyValue("");
      setKeyTarget({ kind, id, name, hasKey });
    },
    testingKey,
    testResults,
  };

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <div className="eyebrow">设置</div>
          <h1>设置与连接</h1>
          <p className="page-subtitle">
            模型服务、学术与新闻来源、网络策略和本地运行时。密钥只写入操作系统凭据库。
          </p>
        </div>
      </header>

      {error && (
        <div className="canvas-error" role="alert">
          <span>{error}</span>
          <button
            type="button"
            className="btn btn--ghost btn--compact"
            onClick={() => setError(null)}
          >
            关闭
          </button>
        </div>
      )}

      {notice && <div className="settings-notice">{notice}</div>}

      <nav className="settings-tabs" aria-label="设置分类">
        {TABS.map(([value, label]) => (
          <button
            key={value}
            type="button"
            className={`settings-tab ${tab === value ? "is-active" : ""}`}
            onClick={() => setTab(value)}
          >
            {label}
          </button>
        ))}
      </nav>

      {!view && loading && <p className="muted">正在读取设置…</p>}

      {view && tab === "models" && <ProviderSettings view={view} {...handlers} />}
      {view && tab === "academic" && (
        <SourceSettings view={view} category="academic" {...handlers} />
      )}
      {view && tab === "news" && (
        <SourceSettings view={view} category="news" {...handlers} />
      )}
      {view && tab === "agent" && (
        <AgentSettings settings={view.settings} onPersist={handlers.onPersist} />
      )}
      {view && tab === "network" && (
        <NetworkPanel settings={view.settings} onSave={handlers.onPersist} />
      )}
      {tab === "runtime" && (
        <RuntimePanel
          runtime={runtime}
          workspace={workspace}
          busy={busy}
          onRefreshSidecar={onRefreshSidecar}
          onCloseWorkspace={onCloseWorkspace}
        />
      )}

      {keyTarget && (
        <Modal
          title={`配置密钥 · ${keyTarget.name}`}
          description="密钥写入操作系统凭据库，不会保存到 Workspace、设置文件或日志中。"
          onClose={() => setKeyTarget(null)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setKeyTarget(null)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={keyBusy}
                onClick={() => void submitKey()}
              >
                {keyBusy ? "保存中…" : "保存密钥"}
              </button>
            </>
          }
        >
          <label className="form-field">
            <span>API Key</span>
            <input
              type="password"
              value={keyValue}
              autoFocus
              placeholder={keyTarget.hasKey ? "已配置；留空保存将清除" : "粘贴 API Key"}
              onChange={(event) => setKeyValue(event.target.value)}
            />
          </label>
          <p className="hint">
            当前状态：{keyTarget.hasKey ? "已配置" : "未配置"}。保存后页面只显示“已配置”，不会回显密钥。
          </p>
        </Modal>
      )}
    </section>
  );
}

function NetworkPanel({
  settings,
  onSave,
}: {
  settings: GlobalSettings;
  onSave: (next: GlobalSettings) => void;
}) {
  const [proxyUrl, setProxyUrl] = useState(settings.network.proxyUrl ?? "");
  const [timeoutSeconds, setTimeoutSeconds] = useState(
    String(settings.network.timeoutSeconds),
  );
  const [allowRemoteEgress, setAllowRemoteEgress] = useState(
    settings.privacy.allowRemoteEgress,
  );
  const [confirmBeforeEgress, setConfirmBeforeEgress] = useState(
    settings.privacy.confirmBeforeEgress,
  );
  const [autonomy, setAutonomy] = useState(settings.autonomy);

  useEffect(() => {
    setProxyUrl(settings.network.proxyUrl ?? "");
    setTimeoutSeconds(String(settings.network.timeoutSeconds));
    setAllowRemoteEgress(settings.privacy.allowRemoteEgress);
    setConfirmBeforeEgress(settings.privacy.confirmBeforeEgress);
    setAutonomy(settings.autonomy);
  }, [settings]);

  return (
    <div className="settings-section">
      <article className="card card--pad">
        <h2 className="section-title">网络代理</h2>
        <p className="muted">
          代理用于模型服务与外部来源请求。留空表示直连。Ollama 等本机服务不会被代理强制接管。
        </p>
        <div className="form-grid">
          <label className="form-field">
            <span>代理地址（可选）</span>
            <input
              value={proxyUrl}
              placeholder="http://127.0.0.1:7890"
              onChange={(event) => setProxyUrl(event.target.value)}
            />
          </label>
          <label className="form-field">
            <span>请求超时（秒）</span>
            <input
              type="number"
              min={5}
              max={120}
              value={timeoutSeconds}
              onChange={(event) => setTimeoutSeconds(event.target.value)}
            />
          </label>
        </div>
      </article>

      <article className="card card--pad">
        <h2 className="section-title">隐私与外发</h2>
        <label className="toggle-label">
          <input
            type="checkbox"
            checked={allowRemoteEgress}
            onChange={(event) => setAllowRemoteEgress(event.target.checked)}
          />
          允许把研究文本/文件发送给已配置的远程模型
        </label>
        <label className="toggle-label">
          <input
            type="checkbox"
            checked={confirmBeforeEgress}
            onChange={(event) => setConfirmBeforeEgress(event.target.checked)}
          />
          每次外发前再次确认
        </label>
        <p className="hint">
          当前配置为允许远程外发、不逐次确认。密钥仍只保存在系统凭据库；来源许可与版权限制不受此开关影响。
        </p>
      </article>

      <article className="card card--pad">
        <h2 className="section-title">智能体自主档位</h2>
        <div className="form-field">
          <select value={autonomy} onChange={(event) => setAutonomy(event.target.value)}>
            <option value="cautious">谨慎</option>
            <option value="balanced">均衡（默认）</option>
            <option value="autonomous">自主</option>
          </select>
        </div>
        <p className="hint">
          智能分解、删除原件、覆盖用户报告正文等高风险动作在任何档位都需要确认。
        </p>
      </article>

      <div className="row">
        <button
          type="button"
          className="btn btn--primary"
          onClick={() =>
            onSave({
              ...settings,
              network: {
                proxyUrl: proxyUrl.trim() || null,
                timeoutSeconds: Number(timeoutSeconds) || 30,
              },
              privacy: { allowRemoteEgress, confirmBeforeEgress },
              autonomy,
            })
          }
        >
          保存网络与隐私设置
        </button>
      </div>
    </div>
  );
}

function RuntimePanel({
  runtime,
  workspace,
  busy,
  onRefreshSidecar,
  onCloseWorkspace,
}: {
  runtime: RuntimeInfo | null;
  workspace: WorkspaceInfo | null;
  busy: boolean;
  onRefreshSidecar: () => void;
  onCloseWorkspace: () => void;
}) {
  return (
    <div className="settings-grid">
      <article className="card card--pad">
        <div className="section-head">
          <h2 className="section-title">本地运行时</h2>
          {runtime && (
            <span className={`tag ${runtime.sidecar.ok ? "tag--success" : "tag--warning"}`}>
              {runtime.sidecar.ok ? "可用" : "未就绪"}
            </span>
          )}
        </div>

        {runtime ? (
          <>
            <dl className="settings-list">
              <Row label="应用版本" value={runtime.appVersion} />
              <Row label="Tauri" value={runtime.tauriVersion} />
              <Row label="Python 要求" value={runtime.pythonRequired} />
              <Row
                label="Sidecar"
                value={
                  runtime.sidecar.ok
                    ? `Python ${runtime.sidecar.python ?? "未知"}`
                    : "未就绪"
                }
              />
              <Row
                label="运行模式"
                value={
                  runtime.sidecar.mode === "supervised" ? "受监管常驻进程" : "一次性自检"
                }
              />
              <Row
                label="监听地址"
                value={
                  runtime.sidecar.port ? `127.0.0.1:${runtime.sidecar.port}` : "尚未监听"
                }
                mono
              />
              <Row
                label="进程 PID"
                value={runtime.sidecar.pid ? String(runtime.sidecar.pid) : "—"}
                mono
              />
              <Row label="自动重启" value={`${runtime.sidecar.restarts} 次`} />
              <Row label="最近检测" value={formatTimestamp(runtime.sidecar.checkedAt)} />
            </dl>
            <p className={`inline-status ${runtime.sidecar.ok ? "is-ok" : "is-warning"}`}>
              {runtime.sidecar.message}
            </p>
            <button
              type="button"
              className="btn btn--compact"
              disabled={busy}
              onClick={onRefreshSidecar}
            >
              {busy ? "检测中…" : "重新检测 Sidecar"}
            </button>
          </>
        ) : (
          <p className="muted">尚未读取到运行时信息。请确认应用已在桌面窗口中启动。</p>
        )}
      </article>

      <article className="card card--pad">
        <h2 className="section-title">当前项目</h2>
        {workspace ? (
          <>
            <dl className="settings-list">
              <Row label="项目名称" value={workspace.alias || workspace.name} />
              <Row label="Workspace" value={workspace.workspacePath} mono />
              <Row label="项目 ID" value={workspace.projectId} mono />
              <Row label="根节点" value={workspace.rootNodeId} mono />
              <Row label="数据版本" value={`schema ${workspace.schemaVersion}`} />
            </dl>
            <div className="hint">
              项目配置保存在 Workspace 根目录的 <code>.cers</code>，内部状态保存在
              <code>.cers-data/</code>；两者都不包含 API Key。
            </div>
            <button
              type="button"
              className="btn btn--danger-ghost btn--compact"
              disabled={busy}
              onClick={onCloseWorkspace}
            >
              关闭当前项目
            </button>
          </>
        ) : (
          <p className="muted">尚未打开项目。回到项目页选择一个本地文件夹。</p>
        )}
      </article>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="settings-row">
      <dt>{label}</dt>
      <dd className={mono ? "mono" : undefined}>{value}</dd>
    </div>
  );
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}
