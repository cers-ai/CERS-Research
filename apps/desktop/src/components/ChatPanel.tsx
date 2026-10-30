import { useEffect, useRef, useState } from "react";

import {
  getConversation,
  sendNodeMessage,
  type ChatMessage,
} from "../lib/ipc";

interface ChatPanelProps {
  nodeId: string;
  placeholder?: string;
  emptyText?: string;
  className?: string;
}

export function ChatPanel({
  nodeId,
  placeholder = "提出研究问题、要求比较方法或继续推进当前节点……",
  emptyText = "围绕当前节点目标提问。模型回复不会伪造文献、DOI、页码或实验结果。",
  className,
}: ChatPanelProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setMessages([]);
    setInput("");
    setError(null);
    setLoaded(false);

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
    const element = logRef.current;
    if (element) {
      element.scrollTop = element.scrollHeight;
    }
  }, [messages, sending]);

  const send = async () => {
    const text = input.trim();
    if (!text || sending) {
      return;
    }

    setSending(true);
    setError(null);
    setInput("");
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
      const reply = await sendNodeMessage(nodeId, text);
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
            <p>{message.body}</p>
          </div>
        ))}
        {sending && <div className="node-chat-typing">科研伙伴正在思考…</div>}
        {error && <div className="canvas-error">{error}</div>}
      </div>

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
          <span className="muted">Enter 发送 · Shift+Enter 换行</span>
          <button
            type="button"
            className="btn btn--primary btn--compact"
            disabled={sending || !input.trim()}
            onClick={() => void send()}
          >
            {sending ? "思考中…" : "发送"}
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
