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
}

export interface ScanSummary {
  scanned: number;
  added: number;
  updated: number;
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

export interface GlobalSettings {
  providers: ProviderConfig[];
  sources: SourceConfig[];
  roles: RoleRouting;
  network: NetworkSettings;
  privacy: PrivacySettings;
  autonomy: string;
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

export async function getAssetText(
  assetId: string,
  limit?: number,
): Promise<string | null> {
  return invoke<string | null>("get_asset_text", {
    assetId,
    limit: limit ?? null,
  });
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
