import { invoke } from "@tauri-apps/api/core";

export interface SidecarStatus {
  ok: boolean;
  mode: string;
  python: string | null;
  version: string | null;
  port: number | null;
  pid: number | null;
  restarts: number;
  message: string;
  checkedAt: string;
}

export interface WorkspaceInfo {
  projectId: string;
  name: string;
  alias?: string;
  workspacePath: string;
  rootNodeId: string;
  createdAt: string;
  schemaVersion: number;
}

export interface RuntimeInfo {
  appVersion: string;
  tauriVersion: string;
  pythonRequired: string;
  sidecar: SidecarStatus;
  workspace: WorkspaceInfo | null;
}

export type NodeType = "root" | "research" | "experiment" | "report";
export type EdgeKind = "decomposition" | "association";

export interface GraphNodeDto {
  id: string;
  type: NodeType;
  title: string;
  goal: string;
  state: string;
  x: number;
  y: number;
  revision: number;
}

export interface GraphEdgeDto {
  id: string;
  source: string;
  target: string;
  kind: EdgeKind;
  label: string | null;
}

export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

export interface GraphSnapshot {
  graphRevision: number;
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  viewport: Viewport | null;
}

export interface UpdateNodeInput {
  nodeId: string;
  title: string;
  goal: string;
}

export interface CreateNodeInput {
  nodeType: Exclude<NodeType, "root">;
  title: string;
  goal: string;
  x: number;
  y: number;
  parentId?: string | null;
  edgeKind?: EdgeKind | null;
}

export interface AssetItem {
  id: string;
  name: string;
  relativePath: string;
  absolutePath: string;
  kind: string;
  status: string;
  sizeBytes: number | null;
  modifiedAt: string | null;
  origin: string;
  createdAt: string;
  snippet: string | null;
  page: number | null;
}

export interface LinkedAsset {
  id: string;
  name: string;
  relativePath: string;
  kind: string;
  status: string;
  hasText: boolean;
}

export interface ScanSummary {
  scanned: number;
  added: number;
  updated: number;
  relinked: number;
  missing: number;
  skipped: number;
  truncated: boolean;
  durationMs: number;
}

export interface ModelConfig {
  id: string;
  modelId: string;
  displayName: string;
  enabled: boolean;
  capabilities: string[];
}

export interface ProviderConfig {
  id: string;
  name: string;
  protocol: string;
  baseUrl: string;
  enabled: boolean;
  proxy: string | null;
  timeoutSeconds: number;
  models: ModelConfig[];
}

export interface SourceConfig {
  id: string;
  category: "academic" | "news" | "web";
  name: string;
  driver: string;
  enabled: boolean;
  url: string | null;
  contactEmail: string | null;
  language: string | null;
  region: string | null;
  apiKeyRequired: boolean;
  notes: string | null;
}

export interface RoleRouting {
  conversation: string | null;
  research: string | null;
  planning: string | null;
  code: string | null;
  report: string | null;
}

export interface NetworkSettings {
  proxyUrl: string | null;
  timeoutSeconds: number;
}

export interface PrivacySettings {
  allowRemoteEgress: boolean;
  confirmBeforeEgress: boolean;
}

export interface McpServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  description: string;
}

export interface SkillSettings {
  enabled: string[];
}

export interface UpdateSettings {
  autoCheck: boolean;
  skippedVersion: string | null;
  lastCheckedAt: string | null;
}

export interface GlobalSettings {
  providers: ProviderConfig[];
  sources: SourceConfig[];
  roles: RoleRouting;
  network: NetworkSettings;
  privacy: PrivacySettings;
  autonomy: string;
  mcpServers: McpServerConfig[];
  skills: SkillSettings;
  updates: UpdateSettings;
}

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  skipped: boolean;
  releaseUrl: string | null;
  notes: string | null;
  publishedAt: string | null;
  checkedAt: string;
  message: string | null;
}

export interface ModelReadiness {
  configured: boolean;
  role: string;
  providerId: string | null;
  providerName: string | null;
  modelId: string | null;
  modelName: string | null;
  message: string;
}

export interface SettingsView {
  settings: GlobalSettings;
  providerKeys: Record<string, boolean>;
  sourceKeys: Record<string, boolean>;
}

export interface ConnectionTestResult {
  ok: boolean;
  statusCode: number | null;
  latencyMs: number;
  message: string;
  detail: string | null;
}

export interface ToolEvent {
  name: string;
  server: string;
  ok: boolean;
  summary: string;
  durationMs: number;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  body: string;
  meta?: { toolTrace?: ToolEvent[] } | null;
  createdAt: string;
}

export interface ConversationReply {
  messages: ChatMessage[];
  model: string;
  usage: Record<string, unknown> | null;
  toolTrace: ToolEvent[];
}

export interface SkillInfo {
  id: string;
  name: string;
  description: string;
  version: string;
  source: string;
  permissions: string[];
  inputs: string;
  outputs: string;
  enabled: boolean;
  chars: number;
}

export interface McpTool {
  serverId: string;
  serverName: string;
  name: string;
  qualifiedName: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface McpServerStatus {
  id: string;
  name: string;
  connected: boolean;
  tools: McpTool[];
  error: string | null;
}

export interface AcademicSourceStatus {
  id: string;
  name: string;
  ok: boolean;
  count: number;
  error: string | null;
  latencyMs: number;
}

export interface AcademicRecord {
  id: string;
  externalId: string | null;
  provider: string;
  providerName: string;
  title: string;
  authors: string[];
  year: number | null;
  venue: string | null;
  doi: string | null;
  url: string | null;
  abstract: string | null;
  coverage: "metadata_only" | "abstract_only" | "fulltext" | "unavailable";
  type: string;
  alsoFoundIn: string[];
  retrievedAt: string;
  publishedAt?: string | null;
}

export interface AcademicSearchResponse {
  ok: boolean;
  query: string;
  results: AcademicRecord[];
  sourceStatuses: AcademicSourceStatus[];
  retrievedAt: string;
  message: string | null;
}

export interface SavedSource {
  id: string;
  externalId: string | null;
  provider: string;
  providerName: string;
  doi: string | null;
  url: string | null;
  title: string;
  authors: string[];
  year: number | null;
  venue: string | null;
  abstract: string | null;
  coverage: string;
  recordType: string | null;
  retrievedAt: string | null;
  createdAt: string;
}

export interface ResearchDefinitionContent {
  question: string | null;
  goal: string | null;
  scope: string | null;
  approach: string | null;
  expectedOutputs: string[];
}

export interface ExperimentColumnStats {
  name: string;
  type: "number" | "text";
  missing: number;
  count: number;
  mean?: number | null;
  std?: number | null;
  min?: number | null;
  max?: number | null;
  median?: number | null;
  topValues?: Array<{ value: string; count: number }>;
}

export interface ExperimentCorrelation {
  x: string;
  y: string;
  r: number;
  n: number;
}

export interface ExperimentOutput {
  ok: boolean;
  goal?: string;
  message?: string;
  summary?: {
    rows: number;
    columns: number;
    numericColumns: number;
    missingCells: number;
    truncated: boolean;
  };
  columns?: ExperimentColumnStats[];
  correlations?: ExperimentCorrelation[];
  preview?: {
    headers: string[];
    rows: string[][];
  };
}

export interface ExperimentRun {
  id: string;
  nodeId: string;
  assetId: string | null;
  goal: string;
  state: "completed" | "failed";
  outputs: ExperimentOutput;
  error: string | null;
  createdAt: string;
  finishedAt: string | null;
}

export interface ResearchDefinition {
  content: ResearchDefinitionContent;
  confirmedFields: string[];
  revision: number;
  updatedAt: string | null;
}

export interface ProposalNode {
  clientId: string;
  nodeType: "research" | "experiment" | "report";
  title: string;
  goal: string;
}

export interface ProposalEdge {
  sourceId: string;
  targetId: string;
  kind: "decomposition" | "association";
}

export interface ProposalView {
  id: string;
  anchorNodeId: string;
  baseGraphRevision: number;
  status: string;
  rationale: string;
  nodes: ProposalNode[];
  edges: ProposalEdge[];
}

export interface ReportDocument {
  id: string;
  nodeId: string;
  contentMd: string;
  revision: number;
  updatedAt: string;
}

export interface ReportChange {
  id: string;
  oldText: string;
  newText: string;
  reason: string;
  review: string;
}

export interface ReportChangeProposal {
  id: string;
  nodeId: string;
  baseRevision: number;
  status: string;
  summary: string;
  changes: ReportChange[];
}

export interface ReportOutlineSection {
  heading: string;
  points: string[];
}

export interface ReportOutline {
  title: string;
  sections: ReportOutlineSection[];
}

export interface ReportChangesApplied {
  document: ReportDocument;
  proposal: ReportChangeProposal;
}

export interface ReportRevision {
  id: string;
  revision: number;
  contentMd: string;
  createdAt: string;
}

export interface SaveSourceInput {
  provider: string;
  providerName: string;
  externalId?: string | null;
  doi?: string | null;
  url?: string | null;
  title: string;
  authors: string[];
  year?: number | null;
  venue?: string | null;
  abstract?: string | null;
  coverage: string;
  recordType?: string | null;
  retrievedAt?: string | null;
  nodeId?: string | null;
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getRuntimeInfo(): Promise<RuntimeInfo> {
  return invoke<RuntimeInfo>("runtime_info");
}

export async function openWorkspace(path: string): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("open_workspace", { path });
}

export async function closeWorkspace(): Promise<void> {
  return invoke<void>("close_workspace");
}

export async function createProjectBackup(): Promise<string> {
  return invoke<string>("create_project_backup");
}

export async function restoreProjectBackup(
  archivePath: string,
  destination: string,
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("restore_project_backup", {
    archivePath,
    destination,
  });
}

export async function runSidecarSelfCheck(): Promise<SidecarStatus> {
  return invoke<SidecarStatus>("sidecar_self_check");
}

export async function getGraph(): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("get_graph");
}

export async function createNode(input: CreateNodeInput): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("create_node", { input });
}

export async function moveNode(nodeId: string, x: number, y: number): Promise<void> {
  return invoke<void>("move_node", { nodeId, x, y });
}

export async function createEdge(
  source: string,
  target: string,
  kind: EdgeKind,
): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("create_edge", { source, target, kind });
}

export async function updateNode(input: UpdateNodeInput): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("update_node", { input });
}

export async function deleteNode(nodeId: string): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("delete_node", { nodeId });
}

export async function deleteEdge(edgeId: string): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("delete_edge", { edgeId });
}

export async function saveViewport(
  x: number,
  y: number,
  zoom: number,
): Promise<void> {
  return invoke<void>("save_viewport", { x, y, zoom });
}

export async function scanWorkspace(): Promise<ScanSummary> {
  return invoke<ScanSummary>("scan_workspace");
}

export async function listAssets(query?: string, kind?: string): Promise<AssetItem[]> {
  return invoke<AssetItem[]>("list_assets", {
    query: query ?? null,
    kind: kind ?? null,
  });
}

export async function linkAssetToNode(nodeId: string, assetId: string): Promise<void> {
  return invoke<void>("link_asset_to_node", { nodeId, assetId });
}

export async function unlinkAssetFromNode(nodeId: string, assetId: string): Promise<void> {
  return invoke<void>("unlink_asset_from_node", { nodeId, assetId });
}

export async function listNodeAssets(nodeId: string): Promise<LinkedAsset[]> {
  return invoke<LinkedAsset[]>("list_node_assets", { nodeId });
}

export async function getAssetText(
  assetId: string,
  limit?: number,
): Promise<string | null> {
  return invoke<string | null>("get_asset_text", {
    assetId,
    limit: limit ?? null,
  });
}

export async function listSkills(): Promise<SkillInfo[]> {
  return invoke<SkillInfo[]>("list_skills");
}

export async function setSkillEnabled(
  skillId: string,
  enabled: boolean,
): Promise<SkillInfo[]> {
  return invoke<SkillInfo[]>("set_skill_enabled", { skillId, enabled });
}

export async function listMcpTools(): Promise<McpTool[]> {
  return invoke<McpTool[]>("list_mcp_tools");
}

export async function testMcpServer(serverId: string): Promise<McpServerStatus> {
  return invoke<McpServerStatus>("test_mcp_server", { serverId });
}

export async function callMcpTool(
  serverId: string,
  toolName: string,
  args: Record<string, unknown>,
): Promise<string> {
  return invoke<string>("call_mcp_tool", {
    serverId,
    toolName,
    arguments: args,
  });
}

export async function shutdownMcpServer(serverId: string): Promise<void> {
  return invoke<void>("shutdown_mcp_server", { serverId });
}

export async function getModelReadiness(): Promise<ModelReadiness> {
  return invoke<ModelReadiness>("model_readiness");
}

export async function checkForUpdates(force = false): Promise<UpdateInfo> {
  return invoke<UpdateInfo>("check_for_updates", { force });
}

export async function skipUpdateVersion(version: string): Promise<void> {
  return invoke<void>("skip_update_version", { version });
}

export async function setAutoUpdateCheck(autoCheck: boolean): Promise<GlobalSettings> {
  return invoke<GlobalSettings>("set_auto_update_check", { autoCheck });
}

export async function getGlobalSettings(): Promise<SettingsView> {
  return invoke<SettingsView>("get_global_settings");
}

export async function saveGlobalSettings(
  settings: GlobalSettings,
): Promise<SettingsView> {
  return invoke<SettingsView>("save_global_settings", { settings });
}

export async function setSecret(
  kind: "model" | "source",
  id: string,
  secret: string,
): Promise<boolean> {
  return invoke<boolean>("set_secret", { kind, id, secret });
}

export async function testProvider(
  providerId: string,
): Promise<ConnectionTestResult> {
  return invoke<ConnectionTestResult>("test_provider", { providerId });
}

export async function testSource(
  sourceId: string,
): Promise<ConnectionTestResult> {
  return invoke<ConnectionTestResult>("test_source", { sourceId });
}

export async function getConversation(nodeId: string): Promise<ChatMessage[]> {
  return invoke<ChatMessage[]>("get_conversation", { nodeId });
}

export async function sendNodeMessage(
  nodeId: string,
  message: string,
): Promise<ConversationReply> {
  return invoke<ConversationReply>("send_node_message", {
    input: { nodeId, message },
  });
}

export async function searchAcademicSources(
  query: string,
  yearFrom?: number,
  yearTo?: number,
): Promise<AcademicSearchResponse> {
  return invoke<AcademicSearchResponse>("search_academic_sources", {
    query,
    yearFrom: yearFrom ?? null,
    yearTo: yearTo ?? null,
  });
}

export async function searchNews(
  query?: string,
  maxResults?: number,
): Promise<AcademicSearchResponse> {
  return invoke<AcademicSearchResponse>("search_news", {
    query: query?.trim() || null,
    maxResults: maxResults ?? null,
  });
}

export async function proposeDecomposition(
  nodeId: string,
): Promise<ProposalView> {
  return invoke<ProposalView>("propose_decomposition", { nodeId });
}

export async function confirmProposal(
  proposalId: string,
  nodes: ProposalNode[],
  edges: ProposalEdge[],
): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("confirm_proposal", {
    proposalId,
    nodes,
    edges,
  });
}

export async function getReport(nodeId: string): Promise<ReportDocument> {
  return invoke<ReportDocument>("get_report", { nodeId });
}

export async function saveReport(
  nodeId: string,
  contentMd: string,
  baseRevision: number,
): Promise<ReportDocument> {
  return invoke<ReportDocument>("save_report", {
    nodeId,
    contentMd,
    baseRevision,
  });
}

export async function listReportRevisions(
  nodeId: string,
): Promise<ReportRevision[]> {
  return invoke<ReportRevision[]>("list_report_revisions", { nodeId });
}

export async function exportReportMarkdown(nodeId: string): Promise<string> {
  return invoke<string>("export_report_markdown", { nodeId });
}

export async function suggestReportOutline(nodeId: string): Promise<ReportOutline> {
  return invoke<ReportOutline>("suggest_report_outline", { nodeId });
}

export async function suggestReportChanges(
  nodeId: string,
  instruction: string,
): Promise<ReportChangeProposal> {
  return invoke<ReportChangeProposal>("suggest_report_changes", {
    nodeId,
    instruction,
  });
}

export async function listPendingReportChanges(
  nodeId: string,
): Promise<ReportChangeProposal[]> {
  return invoke<ReportChangeProposal[]>("list_pending_report_changes", { nodeId });
}

export async function applyReportChanges(
  nodeId: string,
  proposalId: string,
  acceptedIds: string[],
): Promise<ReportChangesApplied> {
  return invoke<ReportChangesApplied>("apply_report_changes", {
    nodeId,
    proposalId,
    acceptedIds,
  });
}

export async function exportReportDocument(
  nodeId: string,
  format: "docx" | "pdf",
): Promise<string> {
  return invoke<string>("export_report_document", { nodeId, format });
}

export async function runExperiment(
  nodeId: string,
  assetId: string,
  goal: string,
): Promise<ExperimentRun> {
  return invoke<ExperimentRun>("run_experiment", { nodeId, assetId, goal });
}

export async function listExperimentRuns(
  nodeId: string,
): Promise<ExperimentRun[]> {
  return invoke<ExperimentRun[]>("list_experiment_runs", { nodeId });
}

export async function getResearchDefinition(): Promise<ResearchDefinition> {
  return invoke<ResearchDefinition>("get_research_definition");
}

export async function saveResearchDefinition(
  content: ResearchDefinitionContent,
  confirmedFields: string[],
  baseRevision: number,
): Promise<ResearchDefinition> {
  return invoke<ResearchDefinition>("save_research_definition", {
    content,
    confirmedFields,
    baseRevision,
  });
}

export async function saveAcademicSource(
  input: SaveSourceInput,
): Promise<SavedSource> {
  return invoke<SavedSource>("save_academic_source", { input });
}

export async function listSavedSources(
  nodeId?: string,
  query?: string,
): Promise<SavedSource[]> {
  return invoke<SavedSource[]>("list_saved_sources", {
    nodeId: nodeId ?? null,
    query: query ?? null,
  });
}

export async function removeSavedSource(
  nodeId: string,
  sourceId: string,
): Promise<void> {
  return invoke<void>("remove_saved_source", { nodeId, sourceId });
}

export async function deleteSavedSource(sourceId: string): Promise<void> {
  return invoke<void>("delete_saved_source", { sourceId });
}
