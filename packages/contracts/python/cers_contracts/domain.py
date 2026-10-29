"""跨进程契约 v1 的 Pydantic 实现（镜像 JSON Schema / TypeScript）。"""

from __future__ import annotations

from typing import Annotated, Any, Literal, Union

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel

CONTRACT_VERSION = 1

NodeType = Literal["root", "research", "experiment", "report"]
EdgeKind = Literal["decomposition", "association"]
NodeState = Literal[
    "ready", "running", "waiting_approval", "attention", "result_available"
]
ProposalStatus = Literal["draft", "awaiting_approval", "approved", "rejected", "applied"]
JobStatus = Literal[
    "queued",
    "running",
    "waiting_approval",
    "paused",
    "retry_pending",
    "completed",
    "failed",
    "interrupted",
    "cancelled",
]
AssetStatus = Literal[
    "indexed", "parsing", "unsupported", "missing", "relocated", "error"
]
SourceCoverage = Literal["metadata_only", "abstract_only", "fulltext", "unavailable"]
ClaimStatus = Literal[
    "unverified",
    "inferred",
    "insufficient_evidence",
    "partially_supported",
    "supported",
    "disputed",
    "refuted",
]
ContentOrigin = Literal[
    "retrieved_metadata",
    "extracted_text",
    "model_summary",
    "hypothesis",
    "experiment_result",
]
ReportProvenance = Literal["user", "ai_accepted", "ai_edited", "external"]
ReportChangeReview = Literal["pending", "accepted", "rejected", "conflict"]
ReportChangeProposalStatus = Literal[
    "pending", "partially_applied", "applied", "rejected", "conflict"
]
AutonomyLevel = Literal["cautious", "balanced", "autonomous"]
DataSensitivity = Literal["public", "internal", "private"]


class WireModel(BaseModel):
    """线协议基类：入参允许 snake_case，出参统一 camelCase。"""

    model_config = ConfigDict(
        alias_generator=to_camel,
        populate_by_name=True,
        extra="forbid",
    )


class Vec2(WireModel):
    x: float
    y: float


class ProjectRef(WireModel):
    id: str
    name: str
    alias: str | None = None
    workspace_path: str
    created_at: str
    schema_version: int = Field(ge=1)


class ResearchDefinitionContent(WireModel):
    question: str | None = None
    goal: str | None = None
    scope: str | None = None
    approach: str | None = None
    expected_outputs: list[str] | None = None


class ResearchDefinition(WireModel):
    project_id: str
    content: ResearchDefinitionContent
    confirmed_fields: list[str]
    revision: int = Field(ge=1)
    updated_at: str


class GraphNode(WireModel):
    id: str
    project_id: str
    type: NodeType
    title: str
    goal: str
    state: NodeState
    position: Vec2
    revision: int = Field(ge=1)


class GraphEdge(WireModel):
    id: str
    project_id: str
    source: str
    target: str
    kind: EdgeKind
    label: str | None = None


class ProposedNode(WireModel):
    client_id: str
    type: Literal["research", "experiment", "report"]
    title: str
    goal: str


class ProposedEdge(WireModel):
    source_id: str
    target_id: str
    kind: EdgeKind


class GraphProposal(WireModel):
    id: str
    project_id: str
    anchor_node_id: str
    base_graph_revision: int = Field(ge=1)
    proposed_nodes: list[ProposedNode]
    proposed_edges: list[ProposedEdge]
    status: ProposalStatus
    rationale: str | None = None
    created_at: str
    updated_at: str


class ConfirmProposalRequest(WireModel):
    proposal_id: str
    edited_proposal_revision: int = Field(ge=1)
    idempotency_key: str = Field(min_length=16)


class RiskProfile(WireModel):
    network_egress: bool
    data_export: bool
    file_write: bool
    code_execution: bool
    irreversible: bool
    data_sensitivity: DataSensitivity


class PlanStep(WireModel):
    id: str
    title: str
    expected_output: str | None = None
    requires_approval: bool | None = None


class AnalysisOption(WireModel):
    id: str
    label: str
    evidence_refs: list[str]
    tradeoffs: str | None = None


class ClarificationCard(WireModel):
    kind: Literal["clarification"]
    question: str
    options: list[str] | None = None
    allow_free_text: bool


class PlanCard(WireModel):
    kind: Literal["plan"]
    steps: list[PlanStep]
    editable: Literal[True]
    requires_approval: bool


class AnalysisCard(WireModel):
    kind: Literal["analysis"]
    title: str
    options: list[AnalysisOption]
    evidence_refs: list[str]


class ResultReviewCard(WireModel):
    kind: Literal["result_review"]
    result_ref: str
    findings: list[str]
    caveats: list[str]


InteractionCard = Annotated[
    Union[ClarificationCard, PlanCard, AnalysisCard, ResultReviewCard],
    Field(discriminator="kind"),
]


class ResearchAction(WireModel):
    id: str
    node_id: str
    intent: str
    rationale: str
    evidence_refs: list[str]
    expected_outputs: list[str]
    risk: RiskProfile
    requires_approval: bool
    card: InteractionCard | None = None


class ReasonRef(WireModel):
    kind: Literal["job", "result", "evidence_gap", "report_change"]
    id: str


class SuggestionAction(WireModel):
    kind: str
    target_id: str | None = None


class NextActionSuggestion(WireModel):
    id: str
    project_id: str
    node_id: str | None = None
    title: str
    reason_refs: list[ReasonRef]
    action: SuggestionAction
    created_at: str


class EvidenceLocatorDetail(WireModel):
    page: int | None = Field(default=None, ge=1)
    section: str | None = None
    span_id: str | None = None


class EvidenceLocator(WireModel):
    source_id: str
    asset_id: str | None = None
    asset_revision: str | None = None
    coverage: SourceCoverage
    locator: EvidenceLocatorDetail | None = None
    excerpt: str | None = None
    retrieved_at: str


class ReportChange(WireModel):
    id: str
    block_id: str
    before_revision: int = Field(ge=1)
    before_hash: str
    replacement_text: str
    evidence_refs: list[str]
    review: ReportChangeReview


class ReportChangeProposal(WireModel):
    id: str
    report_id: str
    base_revision: int = Field(ge=1)
    changes: list[ReportChange]
    status: ReportChangeProposalStatus


class JobEvent(WireModel):
    event_id: str
    job_id: str
    node_id: str | None = None
    seq: int = Field(ge=0)
    type: Literal[
        "progress",
        "message_delta",
        "proposal",
        "artifact",
        "completed",
        "failed",
        "waiting_approval",
    ]
    timestamp: str
    payload: Any
    schema_version: int = Field(ge=1)


class TaskStepCheckpoint(WireModel):
    job_id: str
    step_key: str
    input_hash: str
    output_refs: list[str]
    side_effect_state: Literal["none", "pending", "committed", "outcome_unknown"]
    attempt: int = Field(ge=1)
    occurred_at: str
