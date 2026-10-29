import { useCallback, useEffect, useState } from "react";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type NodeProps,
  type NodeTypes,
  type Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { Modal } from "../../components/Modal";
import {
  createEdge,
  createNode,
  deleteEdge,
  deleteNode,
  getGraph,
  moveNode,
  saveViewport,
  updateNode,
  type EdgeKind,
  type GraphNodeDto,
  type GraphSnapshot,
  type NodeType,
  type WorkspaceInfo,
} from "../../lib/ipc";
import { NodeWorkspace } from "./NodeWorkspace";
import { NODE_STATE_LABEL, NODE_TYPE_ICON, NODE_TYPE_LABEL } from "./nodeMeta";

type CanvasNodeData = Record<string, unknown> & {
  title: string;
  goal: string;
  nodeType: NodeType;
  state: string;
  onOpen: () => void;
  onAdd: () => void;
  onEdit: () => void;
  onDelete: () => void;
};

type CanvasNode = Node<CanvasNodeData, "cers">;

type UndoAction =
  | { kind: "node"; id: string }
  | { kind: "edge"; id: string };

interface CanvasPageProps {
  workspace: WorkspaceInfo | null;
  onOpenFolder: () => void;
}

const nodeTypes: NodeTypes = { cers: CersNode };

export function CanvasPage({ workspace, onOpenFolder }: CanvasPageProps) {
  const [snapshot, setSnapshot] = useState<GraphSnapshot | null>(null);
  const [nodes, setNodes] = useState<CanvasNode[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [workspaceNode, setWorkspaceNode] = useState<GraphNodeDto | null>(null);
  const [pendingConnection, setPendingConnection] = useState<Connection | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [addParentId, setAddParentId] = useState<string | null>(null);
  const [addTitle, setAddTitle] = useState("");
  const [addType, setAddType] = useState<Exclude<NodeType, "root">>("research");
  const [addGoal, setAddGoal] = useState("");
  const [addRelation, setAddRelation] = useState<EdgeKind | "none">("association");
  const [editingNode, setEditingNode] = useState<GraphNodeDto | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editGoal, setEditGoal] = useState("");
  const [deletingNode, setDeletingNode] = useState<GraphNodeDto | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const [deletingEdgeId, setDeletingEdgeId] = useState<string | null>(null);
  const [undoAction, setUndoAction] = useState<UndoAction | null>(null);
  const [savedViewport, setSavedViewport] = useState<Viewport | null>(null);

  const buildNodes = useCallback((graph: GraphSnapshot): CanvasNode[] => {
    return graph.nodes.map((node) => ({
      id: node.id,
      type: "cers",
      position: { x: node.x, y: node.y },
      data: {
        title: node.title,
        goal: node.goal,
        nodeType: node.type,
        state: node.state,
        onOpen: () => setWorkspaceNode(node),
        onAdd: () => {
          setAddParentId(node.id);
          resetAddForm(node.id);
        },
        onEdit: () => {
          setEditingNode(node);
          setEditTitle(node.title);
          setEditGoal(node.goal);
        },
        onDelete: () => setDeletingNode(node),
      },
    }));
  }, []);

  const applySnapshot = useCallback(
    (graph: GraphSnapshot) => {
      setSnapshot(graph);
      setNodes(buildNodes(graph));
      setEdges(toFlowEdges(graph));
      setSelectedEdgeId((current) =>
        current && graph.edges.some((edge) => edge.id === current) ? current : null,
      );
    },
    [buildNodes],
  );

  const refresh = useCallback(async () => {
    if (!workspace) {
      setSnapshot(null);
      setNodes([]);
      setEdges([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const graph = await getGraph();
      applySnapshot(graph);
      setSavedViewport(graph.viewport);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [workspace, applySnapshot]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    setSavedViewport(null);
    setUndoAction(null);
  }, [workspace?.projectId]);

  const handleNodesChange = useCallback((changes: NodeChange<CanvasNode>[]) => {
    setNodes((current) => applyNodeChanges(changes, current));
  }, []);

  const handleEdgesChange = useCallback((changes: EdgeChange<Edge>[]) => {
    setEdges((current) => applyEdgeChanges(changes, current));
  }, []);

  const handleDragStop = useCallback(
    async (_event: MouseEvent | TouchEvent, node: CanvasNode) => {
      try {
        await moveNode(node.id, node.position.x, node.position.y);
      } catch (cause) {
        setError(toMessage(cause));
        void refresh();
      }
    },
    [refresh],
  );

  const handleConnect = useCallback((connection: Connection) => {
    if (connection.source && connection.target && connection.source !== connection.target) {
      setPendingConnection(connection);
    }
  }, []);

  const resolveConnection = useCallback(
    async (kind: EdgeKind) => {
      const connection = pendingConnection;
      if (!connection?.source || !connection.target) {
        return;
      }
      try {
        const knownEdges = new Set(snapshot?.edges.map((edge) => edge.id) ?? []);
        const graph = await createEdge(connection.source, connection.target, kind);
        applySnapshot(graph);
        const created = graph.edges.find((edge) => !knownEdges.has(edge.id));
        if (created) {
          setUndoAction({ kind: "edge", id: created.id });
        }
        setPendingConnection(null);
        setError(null);
      } catch (cause) {
        setError(toMessage(cause));
      }
    },
    [pendingConnection, snapshot, applySnapshot],
  );

  const resetAddForm = (parentId: string | null) => {
    setAddParentId(parentId);
    setAddTitle("");
    setAddType("research");
    setAddGoal("");
    setAddRelation(parentId ? "association" : "none");
    setAddOpen(true);
  };

  const submitAddNode = useCallback(async () => {
    const title = addTitle.trim();
    if (!title) {
      setError("请输入节点名称或一句话研究目标。");
      return;
    }

    const parent = snapshot?.nodes.find((node) => node.id === addParentId) ?? null;
    const relation = parent ? addRelation : "none";
    const x = parent ? parent.x + 340 : 120;
    const y = parent ? parent.y : 220;

    try {
      const knownNodes = new Set(snapshot?.nodes.map((node) => node.id) ?? []);
      const graph = await createNode({
        nodeType: addType,
        title,
        goal: addGoal.trim(),
        x,
        y,
        parentId: relation === "none" ? null : parent?.id ?? null,
        edgeKind: relation === "none" ? null : relation,
      });
      applySnapshot(graph);
      const created = graph.nodes.find((node) => !knownNodes.has(node.id));
      if (created) {
        setUndoAction({ kind: "node", id: created.id });
      }
      setAddOpen(false);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [addTitle, addParentId, addType, addGoal, addRelation, snapshot, applySnapshot]);

  const handleParentChange = (value: string) => {
    if (value === "none") {
      setAddParentId(null);
      setAddRelation("none");
      return;
    }
    setAddParentId(value);
    if (addRelation === "none") {
      setAddRelation("association");
    }
  };

  const submitEditNode = useCallback(async () => {
    if (!editingNode) {
      return;
    }
    const title = editTitle.trim();
    if (!title) {
      setError("节点名称不能为空。");
      return;
    }
    try {
      const graph = await updateNode({
        nodeId: editingNode.id,
        title,
        goal: editGoal.trim(),
      });
      applySnapshot(graph);
      setEditingNode(null);
      setUndoAction(null);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [editingNode, editTitle, editGoal, applySnapshot]);

  const confirmDeleteNode = useCallback(async () => {
    if (!deletingNode) {
      return;
    }
    try {
      const graph = await deleteNode(deletingNode.id);
      applySnapshot(graph);
      if (workspaceNode?.id === deletingNode.id) {
        setWorkspaceNode(null);
      }
      setDeletingNode(null);
      setUndoAction(null);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [deletingNode, workspaceNode, applySnapshot]);

  const confirmDeleteEdge = useCallback(async () => {
    if (!deletingEdgeId) {
      return;
    }
    try {
      const graph = await deleteEdge(deletingEdgeId);
      applySnapshot(graph);
      setDeletingEdgeId(null);
      setSelectedEdgeId(null);
      setUndoAction(null);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [deletingEdgeId, applySnapshot]);

  const handleUndo = useCallback(async () => {
    if (!undoAction) {
      return;
    }
    try {
      const graph =
        undoAction.kind === "node"
          ? await deleteNode(undoAction.id)
          : await deleteEdge(undoAction.id);
      applySnapshot(graph);
      if (workspaceNode?.id === undoAction.id) {
        setWorkspaceNode(null);
      }
      setUndoAction(null);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
      setUndoAction(null);
    }
  }, [undoAction, workspaceNode, applySnapshot]);

  if (!workspace) {
    return (
      <section className="page page--centered">
        <div className="empty-state">
          <div className="empty-icon" aria-hidden="true">
            ⬡
          </div>
          <h1>科研图需要一个项目</h1>
          <p className="empty-lead">打开本地文件夹后，系统会创建唯一的总课题根节点。</p>
          <button type="button" className="btn btn--primary" onClick={onOpenFolder}>
            打开本地文件夹
          </button>
        </div>
      </section>
    );
  }

  const name = workspace.alias || workspace.name;

  return (
    <section className="page page--canvas">
      <header className="page-head page-head--compact">
        <div>
          <div className="eyebrow">科研图</div>
          <h1>{name}</h1>
          <p className="page-subtitle">
            双击节点打开浮动工作区；从节点右侧连接点拖出可建立关系。
          </p>
        </div>
        <div className="page-head-actions">
          <span className="tag tag--muted">{snapshot?.nodes.length ?? 0} 个节点</span>
          <span className="tag tag--muted">v{snapshot?.graphRevision ?? 0}</span>
          {undoAction && (
            <button
              type="button"
              className="btn btn--compact"
              onClick={() => void handleUndo()}
            >
              撤销上一步
            </button>
          )}
          <button
            type="button"
            className="btn btn--compact"
            onClick={() => resetAddForm(selectedId)}
          >
            ＋ 添加节点
          </button>
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

      <div className="canvas-surface canvas-surface--flow">
        <div className="flow-wrap">
          <ReactFlowProvider>
            <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={handleNodesChange}
            onEdgesChange={handleEdgesChange}
            onNodeDragStop={handleDragStop}
            onNodeDoubleClick={(_event, node) => {
              const target = snapshot?.nodes.find((item) => item.id === node.id);
              if (target) {
                setWorkspaceNode(target);
              }
            }}
            onSelectionChange={(params) => setSelectedId(params.nodes[0]?.id ?? null)}
            onEdgeClick={(_event, edge) => setSelectedEdgeId(edge.id)}
            onPaneClick={() => setSelectedEdgeId(null)}
            onConnect={handleConnect}
            onMoveEnd={(_event, viewport) => {
              void saveViewport(viewport.x, viewport.y, viewport.zoom).catch(
                () => undefined,
              );
            }}
            isValidConnection={(connection) =>
              Boolean(connection.source && connection.target) &&
              connection.source !== connection.target
            }
            defaultViewport={{ x: 120, y: 80, zoom: 1 }}
            minZoom={0.2}
            maxZoom={2}
            connectionLineStyle={{ stroke: "#3668d8", strokeWidth: 1.8 }}
          >
              <Background
                variant={BackgroundVariant.Dots}
                gap={24}
                size={1}
                color="#dce4ef"
              />
              <Controls showInteractive={false} />
              <ViewportSync viewport={savedViewport} />
            </ReactFlow>
          </ReactFlowProvider>
        </div>

        {selectedEdgeId && (
          <div className="canvas-selection-bar">
            <span>已选择一条科研关系</span>
            <button
              type="button"
              className="btn btn--danger-ghost btn--compact"
              onClick={() => setDeletingEdgeId(selectedEdgeId)}
            >
              删除关系
            </button>
            <button
              type="button"
              className="btn btn--ghost btn--compact"
              onClick={() => setSelectedEdgeId(null)}
            >
              取消选择
            </button>
          </div>
        )}

        {loading && <div className="canvas-loading">正在读取科研图…</div>}

        <div className="canvas-hint">
          实线箭头 = 分解关系；虚线 = 研究关联。拖动节点会自动保存位置。
        </div>
      </div>

      {workspaceNode && (
        <NodeWorkspace node={workspaceNode} onClose={() => setWorkspaceNode(null)} />
      )}

      {pendingConnection && (
        <Modal
          title="建立科研关系"
          description="关系类型决定这条线在画布上的含义。确认后会立即写入科研图。"
          onClose={() => setPendingConnection(null)}
          footer={
            <button
              type="button"
              className="btn"
              onClick={() => setPendingConnection(null)}
            >
              取消
            </button>
          }
        >
          <div className="relation-choices">
            <button
              type="button"
              className="relation-choice"
              onClick={() => void resolveConnection("decomposition")}
            >
              <strong>分解关系</strong>
              <span>父课题分解出一个独立研究工作，使用实线箭头。</span>
            </button>
            <button
              type="button"
              className="relation-choice"
              onClick={() => void resolveConnection("association")}
            >
              <strong>研究关联</strong>
              <span>两个节点存在研究联系，但不是父子分解，使用虚线。</span>
            </button>
          </div>
        </Modal>
      )}

      {addOpen && (
        <Modal
          title="手工新增节点"
          description="最少只需要一句话目标与节点类型。新增节点不会自动触发智能分解。"
          onClose={() => setAddOpen(false)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setAddOpen(false)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void submitAddNode()}
              >
                创建节点
              </button>
            </>
          }
        >
          <label className="form-field">
            <span>节点名称 / 一句话研究目标</span>
            <input
              value={addTitle}
              maxLength={120}
              autoFocus
              placeholder="例如：验证不同季节条件下的绿地降温效应"
              onChange={(event) => setAddTitle(event.target.value)}
            />
          </label>

          <div className="form-grid">
            <label className="form-field">
              <span>节点类型</span>
              <select
                value={addType}
                onChange={(event) =>
                  setAddType(event.target.value as Exclude<NodeType, "root">)
                }
              >
                <option value="research">研究节点</option>
                <option value="experiment">实验节点</option>
                <option value="report">报告节点</option>
              </select>
            </label>

            <label className="form-field">
              <span>与所选节点的关系</span>
              <select
                value={addRelation}
                disabled={!addParentId}
                onChange={(event) =>
                  setAddRelation(event.target.value as EdgeKind | "none")
                }
              >
                <option value="association">研究关联（虚线）</option>
                <option value="decomposition">分解关系（实线箭头）</option>
                <option value="none">暂不关联</option>
              </select>
            </label>
          </div>

          <label className="form-field">
            <span>关联节点</span>
            <select
              value={addParentId ?? "none"}
              onChange={(event) => handleParentChange(event.target.value)}
            >
              <option value="none">不关联任何节点（独立节点）</option>
              {snapshot?.nodes.map((node) => (
                <option key={node.id} value={node.id}>
                  {NODE_TYPE_LABEL[node.type]} · {node.title}
                </option>
              ))}
            </select>
          </label>

          <label className="form-field">
            <span>补充说明（可选）</span>
            <textarea
              value={addGoal}
              placeholder="写下这个节点要回答的问题、边界或预期产出。"
              onChange={(event) => setAddGoal(event.target.value)}
            />
          </label>
        </Modal>
      )}

      {editingNode && (
        <Modal
          title="编辑节点"
          description="修改节点名称与研究目标。节点类型与已有关系不会改变。"
          onClose={() => setEditingNode(null)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setEditingNode(null)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void submitEditNode()}
              >
                保存修改
              </button>
            </>
          }
        >
          <label className="form-field">
            <span>节点名称</span>
            <input
              value={editTitle}
              maxLength={120}
              disabled={editingNode.type === "root"}
              onChange={(event) => setEditTitle(event.target.value)}
            />
          </label>
          {editingNode.type === "root" && (
            <p className="hint">总课题名称与项目名绑定；此处可修改研究目标，名称请在项目设置中调整。</p>
          )}
          <label className="form-field">
            <span>研究目标</span>
            <textarea
              value={editGoal}
              placeholder="写下这个节点要回答的问题、边界或预期产出。"
              onChange={(event) => setEditGoal(event.target.value)}
            />
          </label>
        </Modal>
      )}

      {deletingNode && (
        <Modal
          title="删除节点"
          description={`将删除“${deletingNode.title}”以及与之相连的所有关系。真实文件不会被删除。`}
          onClose={() => setDeletingNode(null)}
          footer={
            <>
              <button type="button" className="btn" onClick={() => setDeletingNode(null)}>
                取消
              </button>
              <button
                type="button"
                className="btn btn--danger"
                onClick={() => void confirmDeleteNode()}
              >
                确认删除
              </button>
            </>
          }
        >
          <p className="muted">
            节点删除后无法通过“撤销上一步”恢复。请确认该节点不再需要。
          </p>
        </Modal>
      )}

      {deletingEdgeId && (
        <Modal
          title="删除科研关系"
          description="删除后该关系不再出现在科研图中。节点与真实文件不会被删除。"
          onClose={() => setDeletingEdgeId(null)}
          footer={
            <>
              <button
                type="button"
                className="btn"
                onClick={() => setDeletingEdgeId(null)}
              >
                取消
              </button>
              <button
                type="button"
                className="btn btn--danger"
                onClick={() => void confirmDeleteEdge()}
              >
                确认删除
              </button>
            </>
          }
        >
          <p className="muted">删除关系不会影响两个节点本身及其研究内容。</p>
        </Modal>
      )}
    </section>
  );
}

function ViewportSync({ viewport }: { viewport: Viewport | null }) {
  const { setViewport } = useReactFlow();

  useEffect(() => {
    if (!viewport) {
      return;
    }
    const timer = window.setTimeout(() => {
      try {
        setViewport(viewport);
      } catch {
        // React Flow 实例尚未完成初始化时忽略一次恢复。
      }
    }, 60);
    return () => window.clearTimeout(timer);
  }, [viewport, setViewport]);

  return null;
}

function CersNode({ data, selected }: NodeProps<CanvasNode>) {
  const node = data as CanvasNodeData;

  return (
    <div
      className={`flow-node flow-node--${node.nodeType} ${selected ? "is-selected" : ""}`}
    >
      {selected && (
        <div className="flow-node-tools nodrag" onPointerDown={(event) => event.stopPropagation()}>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              node.onOpen();
            }}
          >
            打开
          </button>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              node.onEdit();
            }}
          >
            编辑
          </button>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              node.onAdd();
            }}
          >
            ＋ 添加
          </button>
          {node.nodeType !== "root" && (
            <button
              type="button"
              className="is-danger"
              onClick={(event) => {
                event.stopPropagation();
                node.onDelete();
              }}
            >
              删除
            </button>
          )}
        </div>
      )}

      <Handle type="target" position={Position.Left} className="flow-handle" />

      <div className="flow-node-type">
        <span className="flow-node-bar" />
        <span className="flow-node-icon">{NODE_TYPE_ICON[node.nodeType]}</span>
        <span>{NODE_TYPE_LABEL[node.nodeType]}</span>
      </div>
      <h3 className="flow-node-title">{node.title}</h3>
      <p className="flow-node-goal">{node.goal || "尚未填写研究目标"}</p>
      <div className="flow-node-footer">
        <span>{NODE_STATE_LABEL[node.state] ?? node.state}</span>
        <span>{node.nodeType === "root" ? "唯一根节点" : "双击打开"}</span>
      </div>

      <Handle type="source" position={Position.Right} className="flow-handle" />
    </div>
  );
}

function toFlowEdges(graph: GraphSnapshot): Edge[] {
  return graph.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: "smoothstep",
    markerEnd:
      edge.kind === "decomposition"
        ? { type: MarkerType.ArrowClosed, color: "#7f91ad", width: 16, height: 16 }
        : undefined,
    style:
      edge.kind === "decomposition"
        ? { stroke: "#7f91ad", strokeWidth: 1.8 }
        : { stroke: "#a3afbf", strokeWidth: 1.6, strokeDasharray: "6 5" },
  }));
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "操作失败，请稍后重试。";
}
