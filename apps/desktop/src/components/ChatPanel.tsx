import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import {
  getConversation,
  sendNodeMessage,
  type ChatMessage,
  type ToolEvent,
} from "../lib/ipc";

export interface QuickAction {
  id: string;
  title: string;
  description: string;
  prompt: string;
  deep?: boolean;
}

interface ChatPanelProps {
  nodeId: string;
  quickActions?: QuickAction[];
  placeholder?: string;
  emptyText?: string;
  className?: string;
  defaultDeepResearch?: boolean;
}

interface LiveTool {
  name: string;
  server: string;
}

export function ChatPanel({
  nodeId,
  placeholder = "提出研究问题、要求比较方法或继续推进当前节点……",
  emptyText = "围绕当前节点目标提问。深度研究模式会调用学术检索、资讯与 MCP 工具，并在对话中保留工具轨迹。",
  className,
  quickActions,
  defaultDeepResearch = false,
}: ChatPanelProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deepResearch, setDeepResearch] = useState(defaultDeepResearch);
  const [liveTool, setLiveTool] = useState<LiveTool | null>(null);
  const [liveEvents, setLiveEvents] = useState<ToolEvent[]>([]);
  const [liveContent, setLiveContent] = useState("");
  const [liveReasoning, setLiveReasoning] = useState("");
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setMessages([]);
    setInput("");
    setError(null);
    setLoaded(false);
    setLiveTool(null);
    setLiveEvents([]);
    setLiveContent("");
    setLiveReasoning("");

    getConversation(nodeId)
      .then((items) => {
        if (!cancelled) {
          setMessages(items);
          setLoaded(true);
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(toMessage(cause));
          setLoaded(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [nodeId]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const register = async () => {
      const start = await listen<{ nodeId: string; name: string; server: string }>(
        "agent-tool-start",
        (event) => {
          if (!disposed && event.payload.nodeId === nodeId) {
            setLiveTool({ name: event.payload.name, server: event.payload.server });
          }
        },
      );
      const finish = await listen<ToolEvent & { nodeId: string }>("agent-tool", (event) => {
        if (!disposed && event.payload.nodeId === nodeId) {
          setLiveTool(null);
          setLiveEvents((current) => [...current, event.payload]);
        }
      });
      const streamStart = await listen<{ nodeId: string }>(
        "agent-stream-start",
        (event) => {
          if (!disposed && event.payload.nodeId === nodeId) {
            setLiveContent("");
            setLiveReasoning("");
            setLiveTool(null);
          }
        },
      );
      const delta = await listen<{ nodeId: string; kind: string; text: string }>(
        "agent-delta",
        (event) => {
          if (!disposed || event.payload.nodeId !== nodeId) {
            return;
          }
          if (event.payload.kind === "reasoning") {
            setLiveReasoning((current) => current + event.payload.text);
          } else {
            setLiveContent((current) => current + event.payload.text);
          }
        },
      );
      const status = await listen<{ nodeId: string; phase: string }>(
        "agent-status",
        (event) => {
          if (!disposed && event.payload.nodeId === nodeId && event.payload.phase === "done") {
            setLiveTool(null);
          }
        },
      );
      unlisteners.push(start, finish, streamStart, delta, status);
    };

    void register();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [nodeId]);

  useEffect(() => {
    const element = logRef.current;
    if (element) {
      element.scrollTop = element.scrollHeight;
    }
  }, [messages, sending, liveTool, liveEvents, liveContent, liveReasoning]);

  const send = async (override?: string, forceDeep?: boolean) => {
    const text = (override ?? input).trim();
    if (!text || sending) {
      return;
    }
    const useDeep = forceDeep ?? deepResearch;
    if (useDeep) {
      setDeepResearch(true);
    }

    setSending(true);
    setError(null);
    if (!override) {
      setInput("");
    }
    setLiveTool(null);
    setLiveEvents([]);
    setLiveContent("");
    setLiveReasoning("");
    setMessages((current) => [
      ...current,
      {
        id: `pending_${Date.now()}`,
        role: "user",
        body: text,
        createdAt: new Date().toISOString(),
      },
    ]);

    try {
      const reply = await sendNodeMessage(nodeId, text, useDeep);
      setMessages(reply.messages);
    } catch (cause) {
      setError(toMessage(cause));
      try {
        setMessages(await getConversation(nodeId));
      } catch {
        // 保留乐观消息，提示用户本次输入已经提交但回复失败。
      }
    } finally {
      setSending(false);
      setLiveTool(null);
      setLiveContent("");
      setLiveReasoning("");
    }
  };

  return (
    <div className={`node-chat chat-panel ${className ?? ""}`}>
      <div className="node-chat-log" ref={logRef}>
        {!loaded && <p className="muted">正在读取对话…</p>}
        {loaded && messages.length === 0 && <p className="muted">{emptyText}</p>}
        {messages.map((message) => (
          <div
            key={message.id}
            className={`node-chat-message ${message.role === "user" ? "is-user" : ""}`}
          >
            <span className="node-chat-author">
              {message.role === "user" ? "你" : "科研伙伴"}
            </span>
            {message.meta?.toolTrace && message.meta.toolTrace.length > 0 && (
              <details className="node-chat-tools">
                <summary>本轮调用 {message.meta.toolTrace.length} 个工具</summary>
                <ul>
                  {message.meta.toolTrace.map((event, index) => (
                    <li key={`${event.name}-${index}`}>
                      <span className={`tag ${event.ok ? "tag--success" : "tag--warning"}`}>
                        {event.server}
                      </span>
                      <span className="mono">{event.name}</span>
                      <span>{event.summary}</span>
                      <span className="muted">{event.durationMs} ms</span>
                    </li>
                  ))}
                </ul>
              </details>
            )}
            {message.role === "assistant" ? (
              <div className="node-chat-markdown markdown-body">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    a: ({ href, children }) => {
                      const safe =
                        href && /^(https?:|mailto:)/i.test(href) ? href : undefined;
                      return (
                        <a href={safe} target="_blank" rel="noreferrer">
                          {children}
                        </a>
                      );
                    },
                  }}
                >
                  {message.body}
                </ReactMarkdown>
              </div>
            ) : (
              <p className="node-chat-user-text">{message.body}</p>
            )}
          </div>
        ))}
        {sending && (liveReasoning || liveContent) && (
          <div className="node-chat-message is-live">
            <span className="node-chat-author">科研伙伴（实时）</span>
            {liveReasoning && (
              <details className="node-chat-reasoning" open>
                <summary>思考过程</summary>
                <pre>{liveReasoning}</pre>
              </details>
            )}
            {liveContent && (
              <div className="node-chat-markdown markdown-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{liveContent}</ReactMarkdown>
              </div>
            )}
          </div>
        )}
        {sending && liveTool && (
          <div className="node-chat-live">
            正在调用 {liveTool.server} · <span className="mono">{liveTool.name}</span>…
          </div>
        )}
        {sending && liveEvents.length > 0 && (
          <div className="node-chat-live-trace">
            {liveEvents.map((event, index) => (
              <div key={`${event.name}-${index}`} className="node-chat-live-row">
                <span className={event.ok ? "is-ok" : "is-failed"}>
                  {event.ok ? "✓" : "!"}
                </span>
                <span className="mono">{event.name}</span>
                <span>{event.summary}</span>
              </div>
            ))}
          </div>
        )}
        {sending && !liveTool && !liveContent && !liveReasoning && (
          <div className="node-chat-typing">科研伙伴正在思考…</div>
        )}
        {error && <div className="canvas-error">{error}</div>}
      </div>

      {quickActions && quickActions.length > 0 && (
        <div className="quick-actions" aria-label="可选研究操作">
          {quickActions.map((action) => (
            <button
              key={action.id}
              type="button"
              className="quick-action-card"
              disabled={sending}
              onClick={() => void send(action.prompt, action.deep)}
            >
              <strong>{action.title}</strong>
              <span>{action.description}</span>
              {action.deep && <span className="tag tag--muted">深度研究</span>}
            </button>
          ))}
        </div>
      )}

      <div className="node-chat-compose">
        <textarea
          value={input}
          rows={3}
          placeholder={placeholder}
          disabled={sending}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void send();
            }
          }}
        />
        <div className="node-chat-compose-actions">
          <label
            className="node-chat-mode"
            title="开启后会调用学术检索、资讯与 MCP 工具，耗时更长"
          >
            <input
              type="checkbox"
              checked={deepResearch}
              disabled={sending}
              onChange={(event) => setDeepResearch(event.target.checked)}
            />
            深度研究
          </label>
          <span className="muted">Enter 发送 · Shift+Enter 换行</span>
          <button
            type="button"
            className="btn btn--primary btn--compact"
            disabled={sending || !input.trim()}
            onClick={() => void send()}
          >
            {sending ? "研究中…" : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "对话失败，请稍后重试。";
}
