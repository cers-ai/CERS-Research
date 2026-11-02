import { useCallback, useEffect, useState } from "react";

import {
  listSkills,
  setSkillEnabled,
  testMcpServer,
  type GlobalSettings,
  type McpServerConfig,
  type McpServerStatus,
  type SkillInfo,
} from "../../lib/ipc";

interface AgentSettingsProps {
  settings: GlobalSettings;
  onPersist: (next: GlobalSettings) => void | Promise<void>;
}

export function AgentSettings({ settings, onPersist }: AgentSettingsProps) {
  const [servers, setServers] = useState<McpServerConfig[]>(
    settings.mcpServers ?? [],
  );
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [statuses, setStatuses] = useState<Record<string, McpServerStatus>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    setServers(settings.mcpServers ?? []);
  }, [settings.mcpServers]);

  const loadSkills = useCallback(async () => {
    try {
      setSkills(await listSkills());
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, []);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  useEffect(() => {
    if (!notice) {
      return;
    }
    const timer = window.setTimeout(() => setNotice(null), 2600);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const update = (index: number, patch: Partial<McpServerConfig>) => {
    setServers((current) =>
      current.map((server, position) =>
        position === index ? { ...server, ...patch } : server,
      ),
    );
  };

  const addServer = () => {
    const id = `mcp-${servers.length + 1}`;
    setServers((current) => [
      ...current,
      {
        id,
        name: "新的 MCP 服务",
        command: "",
        args: [],
        env: {},
        enabled: true,
        description: "",
      },
    ]);
  };

  const saveServers = async () => {
    setSaving(true);
    setError(null);
    try {
      await onPersist({ ...settings, mcpServers: servers });
      setNotice("MCP 服务配置已保存");
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setSaving(false);
    }
  };

  const test = async (server: McpServerConfig) => {
    setTesting(server.id);
    setError(null);
    try {
      await onPersist({ ...settings, mcpServers: servers });
      const status = await testMcpServer(server.id);
      setStatuses((current) => ({ ...current, [server.id]: status }));
    } catch (cause) {
      setStatuses((current) => ({
        ...current,
        [server.id]: {
          id: server.id,
          name: server.name,
          connected: false,
          tools: [],
          error: toMessage(cause),
        },
      }));
    } finally {
      setTesting(null);
    }
  };

  const toggleSkill = async (skill: SkillInfo) => {
    setError(null);
    try {
      setSkills(await setSkillEnabled(skill.id, !skill.enabled));
    } catch (cause) {
      setError(toMessage(cause));
    }
  };

  return (
    <div className="agent-settings">
      <article className="card card--pad">
        <div className="section-head">
          <h2 className="section-title">MCP 服务</h2>
          <span className="tag tag--muted">{servers.length}</span>
        </div>
        <p className="muted">
          连接本地 stdio MCP 服务。服务由你明确配置的命令启动，仅在对话需要工具时调用；
          工具结果会作为外部资料返回，不会自动执行写操作。
        </p>

        {error && <div className="canvas-error">{error}</div>}
        {notice && <div className="settings-notice">{notice}</div>}

        {servers.length === 0 && (
          <p className="hint">
            内置网页与 RSS 读取工具已随安装包提供，不需要另行配置 MCP。
            如需连接自定义 stdio MCP 服务，再在这里添加；其运行环境由该服务自身提供。
          </p>
        )}

        {servers.map((server, index) => {
          const status = statuses[server.id];
          return (
            <div className="mcp-card" key={`${server.id}-${index}`}>
              <div className="mcp-grid">
                <label className="form-field">
                  <span>ID</span>
                  <input
                    value={server.id}
                    onChange={(event) => update(index, { id: event.target.value })}
                  />
                </label>
                <label className="form-field">
                  <span>名称</span>
                  <input
                    value={server.name}
                    onChange={(event) => update(index, { name: event.target.value })}
                  />
                </label>
                <label className="form-field">
                  <span>启动命令</span>
                  <input
                    value={
                      server.command === "__cers_bundled_mcp__"
                        ? "内置服务（随安装包提供）"
                        : server.command
                    }
                    disabled={server.command === "__cers_bundled_mcp__"}
                    placeholder="uv / npx / python"
                    onChange={(event) => update(index, { command: event.target.value })}
                  />
                </label>
                <label className="form-field">
                  <span>启用</span>
                  <input
                    type="checkbox"
                    checked={server.enabled}
                    onChange={(event) =>
                      update(index, { enabled: event.target.checked })
                    }
                  />
                </label>
              </div>

              <label className="form-field">
                <span>参数（每行一个）</span>
                <textarea
                  rows={3}
                  value={server.args.join("\n")}
                  onChange={(event) =>
                    update(index, {
                      args: event.target.value
                        .split("\n")
                        .map((value) => value.trim())
                        .filter(Boolean),
                    })
                  }
                />
              </label>

              <label className="form-field">
                <span>环境变量（每行 KEY=VALUE，可选）</span>
                <textarea
                  rows={2}
                  value={Object.entries(server.env)
                    .map(([key, value]) => `${key}=${value}`)
                    .join("\n")}
                  onChange={(event) =>
                    update(index, { env: parseEnv(event.target.value) })
                  }
                />
              </label>

              <label className="form-field">
                <span>说明</span>
                <input
                  value={server.description}
                  onChange={(event) =>
                    update(index, { description: event.target.value })
                  }
                />
              </label>

              <div className="row">
                <button
                  type="button"
                  className="btn btn--compact"
                  disabled={saving}
                  onClick={() => void test(server)}
                >
                  {testing === server.id ? "连接中…" : "保存并测试"}
                </button>
                <button
                  type="button"
                  className="btn btn--ghost btn--compact"
                  onClick={() =>
                    setServers((current) =>
                      current.filter((_, position) => position !== index),
                    )
                  }
                >
                  移除
                </button>
              </div>

              {status && (
                <div className={status.connected ? "hint" : "canvas-error"}>
                  {status.connected
                    ? `已连接，发现 ${status.tools.length} 个工具：${status.tools
                        .map((tool) => tool.name)
                        .join("、")}`
                    : `连接失败：${status.error ?? "未知错误"}`}
                </div>
              )}
            </div>
          );
        })}

        <div className="row">
          <button type="button" className="btn btn--compact" onClick={addServer}>
            添加 MCP 服务
          </button>
          <button
            type="button"
            className="btn btn--primary btn--compact"
            disabled={saving}
            onClick={() => void saveServers()}
          >
            {saving ? "保存中…" : "保存 MCP 配置"}
          </button>
        </div>
      </article>

      <article className="card card--pad">
        <div className="section-head">
          <h2 className="section-title">技能</h2>
          <span className="tag tag--muted">{skills.filter((skill) => skill.enabled).length}</span>
        </div>
        <p className="muted">
          技能是存放在本机的可审阅指令文件。启用后，其步骤会注入当前对话的研究提示词，
          但不能绕过来源与证据规则。示例技能位于应用配置目录的 skills 文件夹。
        </p>

        {skills.length === 0 ? (
          <p className="hint">尚未发现技能文件。</p>
        ) : (
          <div className="skill-list">
            {skills.map((skill) => (
              <div className="skill-row" key={skill.id}>
                <div className="skill-main">
                  <div className="section-head">
                    <strong>{skill.name}</strong>
                    <span className="tag tag--muted">v{skill.version}</span>
                    {skill.enabled && <span className="tag tag--success">已启用</span>}
                  </div>
                  <p className="muted">{skill.description || "未填写说明"}</p>
                  {skill.permissions.length > 0 && (
                    <p className="hint">
                      权限：{skill.permissions.join("、")} · 输入：{skill.inputs || "—"} ·
                      输出：{skill.outputs || "—"}
                    </p>
                  )}
                  <p className="mono skill-source">{skill.source}</p>
                </div>
                <button
                  type="button"
                  className={`btn btn--compact ${skill.enabled ? "btn--ghost" : "btn--primary"}`}
                  onClick={() => void toggleSkill(skill)}
                >
                  {skill.enabled ? "停用" : "启用"}
                </button>
              </div>
            ))}
          </div>
        )}
      </article>
    </div>
  );
}

function parseEnv(value: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of value.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const index = trimmed.indexOf("=");
    if (index <= 0) {
      continue;
    }
    result[trimmed.slice(0, index).trim()] = trimmed.slice(index + 1).trim();
  }
  return result;
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "智能体配置操作失败，请稍后重试。";
}
