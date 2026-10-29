/**
 * 澄思学术跨进程契约 v1（冻结于 2026-10-28）
 *
 * 唯一事实源：packages/contracts/schemas/domain.schema.json
 * 变更规则：docs/契约冻结.md
 *
 * 线协议约定：
 * - IPC / Tauri command / Sidecar HTTP 使用 camelCase。
 * - 磁盘上的 `.cers` 与 SQLite 使用 snake_case，不经过本文件的类型。
 * - 时间戳统一 ISO 8601 UTC（`2026-10-28T00:00:00Z`）。
 * - ID 统一 `<前缀>_<26 位小写 Crockford ULID>`，例如 `node_01j9...`。
 */

export const CONTRACT_VERSION = 1 as const;

/* ------------------------------------------------------------------ */
/* 枚举（任何一端不得自行新增同义值）                                   */
/* ------------------------------------------------------------------ */

export type NodeType = "root" | "research" | "experiment" | "report";

export type EdgeKind = "decomposition" | "association";

export type NodeState =
  | "ready"
  | "running"
  | "waiting_approval"
  | "attention"
  | "result_available";

export type ProposalStatus =
  | "draft"
  | "awaiting_approval"
  | "approved"
  | "rejected"
  | "applied";

export type JobStatus =
  | "queued"
  | "running"
  | "waiting_approval"
  | "paused"
  | "retry_pending"
  | "completed"
  | "failed"
  | "interrupted"
  | "cancelled";

export type AssetStatus =
  | "indexed"
  | "parsing"
  | "unsupported"
  | "missing"
  | "relocated"
  | "error";

/** 实际取得了多少内容；与结论是否被支持完全正交。 */
export type SourceCoverage =
  | "metadata_only"
  | "abstract_only"
  | "fulltext"
  | "unavailable";

/** 证据对一项主张的支持程度；不得由 coverage 推导。 */
export type ClaimStatus =
  | "unverified"
  | "inferred"
  | "insufficient_evidence"
  | "partially_supported"
  | "supported"
  | "disputed"
  | "refuted";

/** 内容来源类型：AI 总结不能伪装成原文或实验结果。 */
export type ContentOrigin =
  | "retrieved_metadata"
  | "extracted_text"
  | "model_summary"
  | "hypothesis"
  | "experiment_result";

export type ReportProvenance = "user" | "ai_accepted" | "ai_edited" | "external";

export type ReportChangeReview = "pending" | "accepted" | "rejected" | "conflict";

export type ReportChangeProposalStatus =
  | "pending"
  | "partially_applied"
  | "applied"
  | "rejected"
  | "conflict";

export type AutonomyLevel = "cautious" | "balanced" | "autonomous";

export type DataSensitivity = "public" | "internal" | "private";

/* ------------------------------------------------------------------ */
/* 基础对象                                                             */
/* ------------------------------------------------------------------ */

export interface Vec2 {
  x: number;
  y: number;
}

export interface ProjectRef {
  id: string;
  name: string;
  alias?: string;
  workspacePath: string;
  createdAt: string;
  schemaVersion: number;
}

export interface ResearchDefinition {
  projectId: string;
  content: {
    question?: string;
    goal?: string;
    scope?: string;
    approach?: string;
    expectedOutputs?: string[];
  };
  confirmedFields: string[];
  revision: number;
  updatedAt: string;
}

export interface GraphNode {
  id: string;
  projectId: string;
  type: NodeType;
  title: string;
  goal: string;
  state: NodeState;
  position: Vec2;
  revision: number;
}

export interface GraphEdge {
  id: string;
  projectId: string;
  source: string;
  target: string;
  kind: EdgeKind;
  label?: string;
}

/* ------------------------------------------------------------------ */
/* 智能分解提案                                                         */
/* ------------------------------------------------------------------ */

export interface ProposedNode {
  /** 提案内临时 ID；确认前不得作为正式 node id 使用。 */
  clientId: string;
  type: Exclude<NodeType, "root">;
  title: string;
  goal: string;
}

export interface ProposedEdge {
  sourceId: string;
  targetId: string;
  kind: EdgeKind;
}

export interface GraphProposal {
  id: string;
  projectId: string;
  anchorNodeId: string;
  baseGraphRevision: number;
  proposedNodes: ProposedNode[];
  proposedEdges: ProposedEdge[];
  status: ProposalStatus;
  rationale?: string;
  createdAt: string;
  updatedAt: string;
}

export interface ConfirmProposalRequest {
  proposalId: string;
  editedProposalRevision: number;
  idempotencyKey: string;
}

/* ------------------------------------------------------------------ */
/* 研究行动与四类卡片                                                   */
/* ------------------------------------------------------------------ */

export interface RiskProfile {
  networkEgress: boolean;
  dataExport: boolean;
  fileWrite: boolean;
  codeExecution: boolean;
  irreversible: boolean;
  dataSensitivity: DataSensitivity;
}

export interface ActionOption {
  id: string;
  label: string;
  rationale?: string;
}

export interface PlanStep {
  id: string;
  title: string;
  expectedOutput?: string;
  requiresApproval?: boolean;
}

export interface AnalysisOption {
  id: string;
  label: string;
  evidenceRefs: string[];
  tradeoffs?: string;
}

export type InteractionCard =
  | {
      kind: "clarification";
      question: string;
      options?: string[];
      allowFreeText: boolean;
    }
  | {
      kind: "plan";
      steps: PlanStep[];
      editable: true;
      requiresApproval: boolean;
    }
  | {
      kind: "analysis";
      title: string;
      options: AnalysisOption[];
      evidenceRefs: string[];
    }
  | {
      kind: "result_review";
      resultRef: string;
      findings: string[];
      caveats: string[];
    };

export interface ResearchAction {
  id: string;
  nodeId: string;
  intent: string;
  rationale: string;
  evidenceRefs: string[];
  expectedOutputs: string[];
  risk: RiskProfile;
  requiresApproval: boolean;
  card?: InteractionCard;
}

export interface NextActionSuggestion {
  id: string;
  projectId: string;
  nodeId?: string;
  title: string;
  reasonRefs: Array<{
    kind: "job" | "result" | "evidence_gap" | "report_change";
    id: string;
  }>;
  action: { kind: string; targetId?: string };
  createdAt: string;
}

/* ------------------------------------------------------------------ */
/* 证据与报告                                                           */
/* ------------------------------------------------------------------ */

export interface EvidenceLocator {
  sourceId: string;
  assetId?: string;
  assetRevision?: string;
  coverage: SourceCoverage;
  locator?: { page?: number; section?: string; spanId?: string };
  /** 真实抽取片段；禁止用 AI 补写原文。 */
  excerpt?: string;
  retrievedAt: string;
}

export interface ReportChange {
  id: string;
  blockId: string;
  beforeRevision: number;
  beforeHash: string;
  replacementText: string;
  evidenceRefs: string[];
  review: ReportChangeReview;
}

export interface ReportChangeProposal {
  id: string;
  reportId: string;
  baseRevision: number;
  changes: ReportChange[];
  status: ReportChangeProposalStatus;
}

/* ------------------------------------------------------------------ */
/* 任务与事件                                                           */
/* ------------------------------------------------------------------ */

export interface JobEvent {
  eventId: string;
  jobId: string;
  nodeId?: string;
  seq: number;
  type:
    | "progress"
    | "message_delta"
    | "proposal"
    | "artifact"
    | "completed"
    | "failed"
    | "waiting_approval";
  timestamp: string;
  payload: unknown;
  schemaVersion: number;
}

export interface TaskStepCheckpoint {
  jobId: string;
  stepKey: string;
  inputHash: string;
  outputRefs: string[];
  sideEffectState: "none" | "pending" | "committed" | "outcome_unknown";
  attempt: number;
  occurredAt: string;
}
