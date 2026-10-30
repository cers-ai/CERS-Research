import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { createPortal } from "react-dom";

import { ChatPanel } from "../../components/ChatPanel";
import { LiteraturePanel } from "../research/LiteraturePanel";
import { SavedSourcesPanel } from "../research/SavedSourcesPanel";
import type { GraphNodeDto } from "../../lib/ipc";
import { NODE_STATE_LABEL, NODE_TYPE_ICON, NODE_TYPE_LABEL } from "./nodeMeta";

interface NodeWorkspaceProps {
  node: GraphNodeDto;
  onClose: () => void;
}

export function NodeWorkspace({ node, onClose }: NodeWorkspaceProps) {
  const [minimized, setMinimized] = useState(false);
  const [sourcesVersion, setSourcesVersion] = useState(0);
  const [position, setPosition] = useState(() => {
    const width = Math.min(840, window.innerWidth - 96);
    return {
      x: Math.max(24, window.innerWidth - width - 48),
      y: 76,
    };
  });
  const panelRef = useRef<HTMLElement>(null);
  const dragRef = useRef<{ dx: number; dy: number } | null>(null);

  useEffect(() => {
    setMinimized(false);
  }, [node.id]);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  useEffect(() => {
    const handleResize = () => {
      const width = panelRef.current?.offsetWidth ?? 840;
      setPosition((current) => ({
        x: Math.min(current.x, Math.max(8, window.innerWidth - width - 8)),
        y: Math.min(current.y, Math.max(8, window.innerHeight - 56)),
      }));
    };
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  const handlePointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    if ((event.target as HTMLElement).closest("button")) {
      return;
    }
    const rect = panelRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    dragRef.current = {
      dx: event.clientX - rect.left,
      dy: event.clientY - rect.top,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag) {
      return;
    }
    const width = panelRef.current?.offsetWidth ?? 840;
    const x = Math.min(
      Math.max(8, event.clientX - drag.dx),
      Math.max(8, window.innerWidth - width - 8),
    );
    const y = Math.min(
      Math.max(8, event.clientY - drag.dy),
      Math.max(8, window.innerHeight - 56),
    );
    setPosition({ x, y });
  };

  const handlePointerUp = (event: ReactPointerEvent<HTMLElement>) => {
    if (!dragRef.current) {
      return;
    }
    dragRef.current = null;
    try {
      event.currentTarget.releasePointerCapture(event.pointerId);
    } catch {
      // 指针捕获可能已被系统释放，无需处理。
    }
  };

  return createPortal(
    <section
      ref={panelRef}
      className={`node-workspace ${minimized ? "node-workspace--minimized" : ""}`}
      style={{ left: position.x, top: position.y }}
      aria-label={`${NODE_TYPE_LABEL[node.type]}节点工作区`}
    >
      <header
        className="node-workspace-head"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      >
        <span className={`node-workspace-icon type-${node.type}`}>
          {NODE_TYPE_ICON[node.type]}
        </span>
        <div className="node-workspace-titles">
          <strong>{node.title}</strong>
          <small>
            {NODE_TYPE_LABEL[node.type]}节点 · {NODE_STATE_LABEL[node.state] ?? node.state}
          </small>
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--compact"
          onClick={() => setMinimized((value) => !value)}
        >
          {minimized ? "展开" : "收起"}
        </button>
        <button type="button" className="btn btn--ghost btn--compact" onClick={onClose}>
          关闭
        </button>
      </header>

      {!minimized && (
        <div className="node-workspace-body">
          <div className="node-workspace-path">
            <span className="is-done">理解目标</span>
            <span aria-hidden="true">›</span>
            <span className="is-current">当前节点</span>
            <span aria-hidden="true">›</span>
            <span>形成成果</span>
          </div>

          <h3 className="node-workspace-section">节点目标</h3>
          <p className="node-workspace-goal">
            {node.goal || "尚未填写研究目标。你可以在后续编辑节点时补充。"}
          </p>

          <h3 className="node-workspace-section">工作区状态</h3>
          <dl className="node-workspace-meta">
            <div>
              <dt>节点类型</dt>
              <dd>{NODE_TYPE_LABEL[node.type]}</dd>
            </div>
            <div>
              <dt>节点状态</dt>
              <dd>{NODE_STATE_LABEL[node.state] ?? node.state}</dd>
            </div>
            <div>
              <dt>节点 ID</dt>
              <dd className="mono">{node.id}</dd>
            </div>
          </dl>

          <div className="hint">
            文献阅读、证据矩阵、数据分析、报告编辑与实验执行将在对应模块接入后出现在这里。
            当前工作区不展示任何模拟研究内容。
          </div>

          {(node.type === "root" || node.type === "research") && (
            <>
              <h3 className="node-workspace-section">文献检索</h3>
              <LiteraturePanel
                nodeId={node.id}
                refreshKey={sourcesVersion}
                onSaved={() => setSourcesVersion((current) => current + 1)}
              />
              <SavedSourcesPanel
                nodeId={node.id}
                refreshKey={sourcesVersion}
                onChanged={() => setSourcesVersion((current) => current + 1)}
              />
            </>
          )}

          <h3 className="node-workspace-section">科研伙伴</h3>
          <ChatPanel nodeId={node.id} />

          <div className="node-workspace-actions">
            <button type="button" className="btn btn--compact" onClick={onClose}>
              收起工作区
            </button>
          </div>
        </div>
      )}
    </section>,
    document.body,
  );
}
