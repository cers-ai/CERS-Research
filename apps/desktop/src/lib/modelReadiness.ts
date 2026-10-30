import type {
  ModelConfig,
  ProviderConfig,
  RoleRouting,
  SettingsView,
} from "./ipc";

export interface ModelReadiness {
  ready: boolean;
  provider?: ProviderConfig;
  model?: ModelConfig;
  reason?: string;
}

export function resolveConfiguredModel(
  view: SettingsView,
  role: keyof RoleRouting = "research",
): ModelReadiness {
  const settings = view.settings;
  const reference = settings.roles[role] ?? settings.roles.conversation;

  if (reference) {
    const [providerId, modelId] = reference.split("/");
    const provider = settings.providers.find((item) => item.id === providerId);
    const model = provider?.models.find((item) => item.id === modelId);
    if (provider && model) {
      return validate(provider, model, view);
    }
  }

  for (const provider of settings.providers) {
    const model = provider.models.find((item) => item.enabled);
    if (provider.enabled && model) {
      return validate(provider, model, view);
    }
  }

  return { ready: false, reason: "尚未配置可用的模型服务。" };
}

function validate(
  provider: ProviderConfig,
  model: ModelConfig,
  view: SettingsView,
): ModelReadiness {
  if (!provider.enabled) {
    return { ready: false, reason: `模型服务 ${provider.name} 尚未启用。`, provider, model };
  }
  if (!model.enabled) {
    return { ready: false, reason: `模型 ${model.displayName} 尚未启用。`, provider, model };
  }
  if (provider.protocol === "anthropic" || provider.protocol === "gemini") {
    return {
      ready: false,
      reason: "该原生协议适配器尚未接入，请使用 OpenAI-compatible 或 Ollama。",
      provider,
      model,
    };
  }
  if (provider.protocol !== "ollama" && !view.providerKeys[provider.id]) {
    return {
      ready: false,
      reason: `${provider.name} 尚未配置 API Key，请在设置中保存密钥。`,
      provider,
      model,
    };
  }

  return { ready: true, provider, model };
}
