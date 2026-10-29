import { useState } from "react";

import { Modal } from "../../components/Modal";
import type {
  ModelConfig,
  ProviderConfig,
  RoleRouting,
  SettingsView,
} from "../../lib/ipc";
import type { SettingsHandlers } from "./settingsTypes";
import { TestResultView } from "./TestResultView";

const PROTOCOL_LABELS: Record<string, string> = {
  openai_compatible: "OpenAI-compatible",
  ollama: "Ollama",
  anthropic: "Anthropic",
  gemini: "Gemini",
};

const ROLE_LABELS: Array<[keyof RoleRouting, string]> = [
  ["conversation", "对话"],
  ["research", "研究"],
  ["planning", "规划"],
  ["code", "代码"],
  ["report", "报告"],
];

interface ProviderSettingsProps extends SettingsHandlers {
  view: SettingsView;
}

export function ProviderSettings({ view, ...handlers }: ProviderSettingsProps) {
  const [editing, setEditing] = useState<ProviderConfig | null>(null);
  const providers = view.settings.providers;
  const settings = view.settings;

  const toggleProvider = (id: string) => {
    handlers.onPersist({
      ...settings,
      providers: providers.map((provider) =>
        provider.id === id ? { ...provider, enabled: !provider.enabled } : provider,
      ),
    });
  };

  const saveProvider = (provider: ProviderConfig) => {
    const exists = providers.some((item) => item.id === provider.id);
    handlers.onPersist({
      ...settings,
      providers: exists
        ? providers.map((item) => (item.id === provider.id ? provider : item))
        : [...providers, provider],
    });
    setEditing(null);
  };

  const changeRole = (role: keyof RoleRouting, value: string) => {
    handlers.onPersist({
      ...settings,
      roles: { ...settings.roles, [role]: value || null },
    });
  };

  const modelOptions = providers.flatMap((provider) =>
    provider.models
      .filter((model) => model.enabled)
      .map((model) => ({
        value: `${provider.id}/${model.id}`,
        label: `${provider.name} / ${model.displayName || model.modelId}`,
      })),
  );

  return (
    <div className="settings-section">
      <div className="section-head">
        <div>
          <h2 className="section-title">模型服务中心</h2>
          <p className="muted">
            服务（Provider）与具体模型分离。密钥只写入操作系统凭据库，页面只显示是否已配置。
          </p>
        </div>
        <button
          type="button"
          className="btn btn--compact"
          onClick={() =>
            setEditing({
              id: `provider_${Date.now()}`,
              name: "",
              protocol: "openai_compatible",
              baseUrl: "",
              enabled: true,
              proxy: null,
              timeoutSeconds: 90,
              models: [
                {
                  id: `model_${Date.now()}`,
                  modelId: "",
                  displayName: "",
                  enabled: true,
                  capabilities: ["chat", "stream"],
                },
              ],
            })
          }
        >
          ＋ 添加模型服务
        </button>
      </div>

      {providers.length === 0 && (
        <div className="hint">尚未配置模型服务。没有模型时，本地项目、画布和资产仍可用。</div>
      )}

      <div className="settings-card-list">
        {providers.map((provider) => {
          const hasKey = Boolean(view.providerKeys[provider.id]);
          const testingKey = `model:${provider.id}`;
          const test = handlers.testResults[testingKey];
          return (
            <article className="card card--pad" key={provider.id}>
              <div className="section-head">
                <h3 className="section-title">{provider.name || "未命名服务"}</h3>
                <span className="tag">{PROTOCOL_LABELS[provider.protocol] ?? provider.protocol}</span>
                <span className={`tag ${provider.enabled ? "tag--success" : ""}`}>
                  {provider.enabled ? "已启用" : "未启用"}
                </span>
                <span className={`tag ${hasKey ? "tag--success" : "tag--warning"}`}>
                  {hasKey ? "密钥已配置" : "未配置密钥"}
                </span>
                <label className="toggle-label">
                  <input
                    type="checkbox"
                    checked={provider.enabled}
                    onChange={() => toggleProvider(provider.id)}
                  />
                  启用
                </label>
              </div>

              <p className="muted mono settings-url">{provider.baseUrl}</p>
              <div className="model-chip-list">
                {provider.models.map((model) => (
                  <span className="tag tag--muted" key={model.id}>
                    {model.displayName || "未命名模型"} · {model.modelId}
                  </span>
                ))}
              </div>
              {provider.proxy && <p className="muted">代理：{provider.proxy}</p>}

              <div className="row">
                <button
                  type="button"
                  className="btn btn--compact"
                  disabled={handlers.testingKey === testingKey}
                  onClick={() => handlers.onTest("model", provider.id)}
                >
                  {handlers.testingKey === testingKey ? "测试中…" : "测试连接"}
                </button>
                <button
                  type="button"
                  className="btn btn--compact"
                  onClick={() => setEditing(cloneProvider(provider))}
                >
                  编辑
                </button>
                <button
                  type="button"
                  className="btn btn--compact"
                  onClick={() =>
                    handlers.onSetKey("model", provider.id, provider.name, hasKey)
                  }
                >
                  {hasKey ? "更新密钥" : "配置密钥"}
                </button>
              </div>

              <TestResultView result={test} />
            </article>
          );
        })}
      </div>

      <article className="card card--pad">
        <h2 className="section-title">默认角色路由</h2>
        <p className="muted">
          不同任务角色可以使用不同模型。当前默认全部指向 DeepSeek；你可以随时调整。
        </p>
        <div className="role-grid">
          {ROLE_LABELS.map(([role, label]) => {
            const current = settings.roles[role] ?? "";
            const known = modelOptions.some((option) => option.value === current);
            return (
              <label className="form-field" key={role}>
                <span>{label}</span>
                <select
                  value={current}
                  onChange={(event) => changeRole(role, event.target.value)}
                >
                  <option value="">未配置</option>
                  {modelOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                  {current && !known && <option value={current}>{current}（已失效）</option>}
                </select>
              </label>
            );
          })}
        </div>
      </article>

      {editing && (
        <ProviderEditor
          initial={editing}
          onCancel={() => setEditing(null)}
          onSave={saveProvider}
        />
      )}
    </div>
  );
}

function cloneProvider(provider: ProviderConfig): ProviderConfig {
  return {
    ...provider,
    models: provider.models.map((model) => ({
      ...model,
      capabilities: [...model.capabilities],
    })),
  };
}

function ProviderEditor({
  initial,
  onCancel,
  onSave,
}: {
  initial: ProviderConfig;
  onCancel: () => void;
  onSave: (provider: ProviderConfig) => void;
}) {
  const [draft, setDraft] = useState<ProviderConfig>(() => cloneProvider(initial));
  const [error, setError] = useState<string | null>(null);

  const updateModel = (index: number, patch: Partial<ModelConfig>) => {
    setDraft((current) => ({
      ...current,
      models: current.models.map((model, modelIndex) =>
        modelIndex === index ? { ...model, ...patch } : model,
      ),
    }));
  };

  const submit = () => {
    const name = draft.name.trim();
    const baseUrl = draft.baseUrl.trim();
    const models = draft.models
      .map((model) => ({
        ...model,
        modelId: model.modelId.trim(),
        displayName: model.displayName.trim() || model.modelId.trim(),
      }))
      .filter((model) => model.modelId);
    if (!name) {
      setError("请输入服务名称。");
      return;
    }
    if (!baseUrl.startsWith("http://") && !baseUrl.startsWith("https://")) {
      setError("Base URL 必须以 http:// 或 https:// 开头。");
      return;
    }
    if (models.length === 0) {
      setError("至少需要配置一个模型 ID。");
      return;
    }
    onSave({
      ...draft,
      name,
      baseUrl,
      proxy: draft.proxy?.trim() || null,
      models,
    });
  };

  return (
    <Modal
      title={initial.name ? "编辑模型服务" : "添加模型服务"}
      description="模型服务是连接定义；一个服务下可以登记多个模型。"
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="btn btn--primary" onClick={submit}>
            保存服务
          </button>
        </>
      }
    >
      {error && <div className="canvas-error">{error}</div>}

      <div className="form-grid">
        <label className="form-field">
          <span>服务名称</span>
          <input
            value={draft.name}
            placeholder="例如：DeepSeek"
            onChange={(event) =>
              setDraft((current) => ({ ...current, name: event.target.value }))
            }
          />
        </label>
        <label className="form-field">
          <span>协议</span>
          <select
            value={draft.protocol}
            onChange={(event) =>
              setDraft((current) => ({ ...current, protocol: event.target.value }))
            }
          >
            <option value="openai_compatible">OpenAI-compatible</option>
            <option value="ollama">Ollama / 本地兼容</option>
            <option value="anthropic">Anthropic（适配器待接入）</option>
            <option value="gemini">Gemini（适配器待接入）</option>
          </select>
        </label>
      </div>

      <label className="form-field">
        <span>Base URL</span>
        <input
          value={draft.baseUrl}
          placeholder="https://api.deepseek.com"
          onChange={(event) =>
            setDraft((current) => ({ ...current, baseUrl: event.target.value }))
          }
        />
      </label>

      <div className="form-grid">
        <label className="form-field">
          <span>代理（可选）</span>
          <input
            value={draft.proxy ?? ""}
            placeholder="http://127.0.0.1:7890"
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                proxy: event.target.value || null,
              }))
            }
          />
        </label>
        <label className="form-field">
          <span>超时（秒）</span>
          <input
            type="number"
            min={5}
            max={120}
            value={draft.timeoutSeconds}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                timeoutSeconds: Number(event.target.value) || 30,
              }))
            }
          />
        </label>
      </div>

      <label className="toggle-label">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(event) =>
            setDraft((current) => ({ ...current, enabled: event.target.checked }))
          }
        />
        启用这个服务
      </label>

      <div className="model-editor">
        <div className="section-head">
          <strong>模型列表</strong>
          <button
            type="button"
            className="btn btn--compact"
            onClick={() =>
              setDraft((current) => ({
                ...current,
                models: [
                  ...current.models,
                  {
                    id: `model_${Date.now()}`,
                    modelId: "",
                    displayName: "",
                    enabled: true,
                    capabilities: ["chat", "stream"],
                  },
                ],
              }))
            }
          >
            ＋ 添加模型
          </button>
        </div>
        {draft.models.map((model, index) => (
          <div className="model-row" key={model.id}>
            <input
              value={model.modelId}
              placeholder="模型 ID，例如 deepseek-v4.1-flash"
              onChange={(event) => updateModel(index, { modelId: event.target.value })}
            />
            <input
              value={model.displayName}
              placeholder="显示名"
              onChange={(event) =>
                updateModel(index, { displayName: event.target.value })
              }
            />
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={model.enabled}
                onChange={(event) =>
                  updateModel(index, { enabled: event.target.checked })
                }
              />
              启用
            </label>
            <button
              type="button"
              className="btn btn--danger-ghost btn--compact"
              disabled={draft.models.length === 1}
              onClick={() =>
                setDraft((current) => ({
                  ...current,
                  models: current.models.filter((item) => item.id !== model.id),
                }))
              }
            >
              删除
            </button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
