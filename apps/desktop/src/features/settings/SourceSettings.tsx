import { useState } from "react";

import { Modal } from "../../components/Modal";
import type { SettingsView, SourceConfig } from "../../lib/ipc";
import type { SettingsHandlers } from "./settingsTypes";
import { TestResultView } from "./TestResultView";

const DRIVER_LABELS: Record<string, string> = {
  openalex: "OpenAlex",
  crossref: "Crossref",
  arxiv: "arXiv",
  semantic_scholar: "Semantic Scholar",
  pubmed: "PubMed",
  europe_pmc: "Europe PMC",
  rss: "RSS / Atom",
  serpapi: "SerpAPI",
  newsapi: "NewsAPI",
  web: "网页",
};

interface SourceSettingsProps extends SettingsHandlers {
  view: SettingsView;
  category: "academic" | "news";
}

export function SourceSettings({ view, category, ...handlers }: SourceSettingsProps) {
  const [editing, setEditing] = useState<SourceConfig | null>(null);
  const sources = view.settings.sources.filter((source) => source.category === category);

  const toggleSource = (id: string) => {
    handlers.onPersist({
      ...view.settings,
      sources: view.settings.sources.map((source) =>
        source.id === id ? { ...source, enabled: !source.enabled } : source,
      ),
    });
  };

  const saveSource = (source: SourceConfig) => {
    const exists = view.settings.sources.some((item) => item.id === source.id);
    handlers.onPersist({
      ...view.settings,
      sources: exists
        ? view.settings.sources.map((item) => (item.id === source.id ? source : item))
        : [...view.settings.sources, source],
    });
    setEditing(null);
  };

  return (
    <div className="settings-section">
      <div className="section-head">
        <div>
          <h2 className="section-title">
            {category === "academic" ? "学术来源" : "新闻与资讯来源"}
          </h2>
          <p className="muted">
            {category === "academic"
              ? "用于文献检索、元数据与 DOI 核验。来源返回题录不等于已阅读全文。"
              : "RSS/Atom 用于资讯订阅；新闻不等于经过同行评议的学术证据。"}
          </p>
        </div>
        {category === "news" && (
          <button
            type="button"
            className="btn btn--compact"
            onClick={() =>
              setEditing({
                id: `rss_${Date.now()}`,
                category: "news",
                name: "新的 RSS 来源",
                driver: "rss",
                enabled: true,
                url: "",
                contactEmail: null,
                language: "zh-CN",
                region: null,
                apiKeyRequired: false,
                notes: null,
              })
            }
          >
            ＋ 添加 RSS 来源
          </button>
        )}
      </div>

      <div className="settings-card-list">
        {sources.map((source) => {
          const hasKey = Boolean(view.sourceKeys[source.id]);
          const keyCapable = supportsKey(source);
          const testingKey = `source:${source.id}`;
          const test = handlers.testResults[testingKey];
          return (
            <article className="card card--pad" key={source.id}>
              <div className="section-head">
                <h3 className="section-title">{source.name}</h3>
                <span className="tag">{DRIVER_LABELS[source.driver] ?? source.driver}</span>
                {source.language && <span className="tag tag--muted">{source.language}</span>}
                <span className={`tag ${source.enabled ? "tag--success" : ""}`}>
                  {source.enabled ? "已启用" : "未启用"}
                </span>
                {keyCapable && (
                  <span className={`tag ${hasKey ? "tag--success" : "tag--warning"}`}>
                    {hasKey ? "密钥已配置" : "未配置密钥"}
                  </span>
                )}
                <label className="toggle-label">
                  <input
                    type="checkbox"
                    checked={source.enabled}
                    onChange={() => toggleSource(source.id)}
                  />
                  启用
                </label>
              </div>

              <p className="muted mono settings-url">{source.url || "尚未配置 URL"}</p>
              {source.contactEmail && (
                <p className="muted">联系邮箱：{source.contactEmail}</p>
              )}
              {source.notes && <p className="muted">{source.notes}</p>}

              <div className="row">
                <button
                  type="button"
                  className="btn btn--compact"
                  disabled={handlers.testingKey === testingKey}
                  onClick={() => handlers.onTest("source", source.id)}
                >
                  {handlers.testingKey === testingKey ? "测试中…" : "测试来源"}
                </button>
                <button
                  type="button"
                  className="btn btn--compact"
                  onClick={() => setEditing({ ...source })}
                >
                  编辑
                </button>
                {keyCapable && (
                  <button
                    type="button"
                    className="btn btn--compact"
                    onClick={() =>
                      handlers.onSetKey("source", source.id, source.name, hasKey)
                    }
                  >
                    {hasKey ? "更新密钥" : "配置密钥"}
                  </button>
                )}
              </div>

              <TestResultView result={test} />
            </article>
          );
        })}
      </div>

      {editing && (
        <SourceEditor
          initial={editing}
          onCancel={() => setEditing(null)}
          onSave={saveSource}
        />
      )}
    </div>
  );
}

function supportsKey(source: SourceConfig): boolean {
  return (
    source.apiKeyRequired ||
    ["openalex", "semantic_scholar", "pubmed", "serpapi", "newsapi"].includes(
      source.driver,
    )
  );
}

function SourceEditor({
  initial,
  onCancel,
  onSave,
}: {
  initial: SourceConfig;
  onCancel: () => void;
  onSave: (source: SourceConfig) => void;
}) {
  const [draft, setDraft] = useState<SourceConfig>({ ...initial });
  const [error, setError] = useState<string | null>(null);

  const submit = () => {
    const name = draft.name.trim();
    const url = draft.url?.trim() || null;
    if (!name) {
      setError("请输入来源名称。");
      return;
    }
    if (
      (draft.driver === "rss" || draft.driver === "web") &&
      url &&
      !url.startsWith("http://") &&
      !url.startsWith("https://")
    ) {
      setError("Feed URL 必须以 http:// 或 https:// 开头。");
      return;
    }
    onSave({
      ...draft,
      name,
      url,
      contactEmail: draft.contactEmail?.trim() || null,
      language: draft.language?.trim() || null,
      region: draft.region?.trim() || null,
      notes: draft.notes?.trim() || null,
    });
  };

  return (
    <Modal
      title="编辑来源"
      description="来源配置用于外部检索，不等同于已经保存的研究资产。"
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="btn btn--primary" onClick={submit}>
            保存来源
          </button>
        </>
      }
    >
      {error && <div className="canvas-error">{error}</div>}

      <div className="form-grid">
        <label className="form-field">
          <span>名称</span>
          <input
            value={draft.name}
            onChange={(event) =>
              setDraft((current) => ({ ...current, name: event.target.value }))
            }
          />
        </label>
        <label className="form-field">
          <span>驱动</span>
          <input value={DRIVER_LABELS[draft.driver] ?? draft.driver} disabled />
        </label>
      </div>

      <label className="form-field">
        <span>{draft.driver === "rss" ? "Feed URL" : "API / Base URL"}</span>
        <input
          value={draft.url ?? ""}
          placeholder={
            draft.driver === "rss"
              ? "https://example.org/feed.xml"
              : "https://api.example.org"
          }
          onChange={(event) =>
            setDraft((current) => ({ ...current, url: event.target.value }))
          }
        />
      </label>

      <div className="form-grid">
        <label className="form-field">
          <span>联系邮箱（可选）</span>
          <input
            value={draft.contactEmail ?? ""}
            placeholder="用于 Crossref / PubMed polite pool"
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                contactEmail: event.target.value || null,
              }))
            }
          />
        </label>
        <label className="form-field">
          <span>语言</span>
          <input
            value={draft.language ?? ""}
            placeholder="zh-CN / en"
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                language: event.target.value || null,
              }))
            }
          />
        </label>
      </div>

      <label className="form-field">
        <span>地区（可选）</span>
        <input
          value={draft.region ?? ""}
          placeholder="CN / US"
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              region: event.target.value || null,
            }))
          }
        />
      </label>

      <label className="form-field">
        <span>备注</span>
        <textarea
          value={draft.notes ?? ""}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              notes: event.target.value || null,
            }))
          }
        />
      </label>

      <label className="toggle-label">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(event) =>
            setDraft((current) => ({ ...current, enabled: event.target.checked }))
          }
        />
        启用这个来源
      </label>
    </Modal>
  );
}
