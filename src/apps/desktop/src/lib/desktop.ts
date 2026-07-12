import { invoke } from "@tauri-apps/api/core";

export type PublisherLifecycle = "stopped" | "running" | "paused" | "failed" | string;

export interface PublisherSnapshot {
  id: string;
  status: PublisherLifecycle;
  message?: string | null;
}

export interface ControlSnapshot {
  workspaceId: string;
  workspaceName: string;
  recoveredFromBackup: boolean;
  requiredSecretCount: number;
  missingSecretCount: number;
  providerCount: number;
  publisherCount: number;
  supervisor: { publishers: PublisherSnapshot[] };
  notifications: Array<{ id?: string; message: string; createdAt?: number }>;
}

export interface DesktopSnapshot {
  initialized: boolean;
  startupError: string | null;
  control: ControlSnapshot | null;
  gateway: { running: boolean; baseUrl: string; entryCount: number; error: string | null };
}

export const getDesktopSnapshot = () => invoke<DesktopSnapshot>("desktop_snapshot");

export const initializeWorkspace = (name: string) =>
  invoke<DesktopSnapshot>("initialize_workspace", { name });

export const changePublisherState = (
  action: "start" | "pause" | "stop",
  publisherId: string,
) => invoke<DesktopSnapshot>(`${action}_publisher`, { publisherId });

export interface ProviderCatalogItem {
  id: string;
  name: string;
  category: string;
  adapter: string;
  defaultBaseUrl: string;
  editableEndpoint: boolean;
  authFields: Array<{ id: string; label: string; required: boolean; secret: boolean }>;
  probeCount: number;
}

export interface ProviderInstanceItem {
  id: string;
  providerId: string;
  name: string;
  endpointOverride: string | null;
  enabled: boolean;
  secretFields: string[];
  inspectionAvailable: boolean;
}

export interface InspectionReport {
  schema_version: number;
  provider_id: string;
  generated_at_unix_ms: number;
  overall: "healthy" | "degraded" | "unavailable" | "unknown";
  findings: Record<string, {
    dimension: string;
    status: string;
    evidence: string;
    latency_ms?: number;
    safe_summary?: string;
    declared_value?: string;
    verified_value?: string;
  }>;
  last_success_at_unix_ms?: number;
  consecutive_failures: number;
}

export interface CodeTemplate {
  language: string;
  title: string;
  code: string;
}

export interface WalletCard {
  id: string;
  providerId: string;
  providerInstanceId: string | null;
  name: string;
  endpointOverride: string | null;
  configured: boolean;
  enabled: boolean;
  source: "catalog" | "asset" | string;
  budgetMicros: number | null;
  currency: string | null;
  requestCount: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostMicros: number | null;
  referenceCount: number;
}

export interface DirectEndpointItem {
  id: string; name: string; alias: string; assetId: string; assetName: string;
  enabled: boolean; tokenConfigured: boolean; baseUrl: string; publicModels: string[]; requestCount: number;
}

export interface WalletAssetImpact { directEndpoints: string[]; canvases: string[]; }

export interface ProjectTree {
  projects: Record<string, {
    id: string;
    name: string;
    folders: Record<string, { id: string; name: string; canvasIds: string[] }>;
    canvases: Record<string, { id: string; name: string; folderId: string | null; graph?: WorkflowGraph; appliedGraph?: WorkflowGraph | null; draftRevision: number; appliedRevision: number; publisherId: string | null; legacyMultiOutput: boolean }>;
  }>;
}

export interface WorkflowGraph {
  schema_version: number;
  id: string;
  nodes: Array<{
    id: string;
    name: string;
    kind: "provider" | "composer" | "middleware" | "probe" | "publisher" | "group";
    enabled: boolean;
    inputs: Array<{ id: string; data_type: string }>;
    outputs: Array<{ id: string; data_type: string }>;
    config: Record<string, unknown>;
  }>;
  edges: Array<{ id: string; from: { node: string; port: string }; to: { node: string; port: string } }>;
}

export type CanvasTab = "overview" | "workflow" | "routes" | "publisher" | "docs" | "runs";

export interface WorkspaceUiState {
  schemaVersion: 4;
  themePreference: ThemePreference;
  lastPage: string;
  workspaceIntent: WorkspaceIntent;
  shell: {
    leftSidebarCollapsed: boolean;
    rightInspectorOpen: boolean;
    rightInspectorPinned: boolean;
    leftWidth: number;
    rightWidth: number;
  };
  workflows: Record<string, {
    viewport: { x: number; y: number; zoom: number };
    nodePositions: Record<string, { x: number; y: number }>;
    collapsedGroups: string[];
  }>;
  selectedProjectId: string | null;
  selectedCanvasId: string | null;
  expandedProjectIds: string[];
  expandedFolderIds: string[];
  canvasTabs: Record<string, CanvasTab>;
}

export interface CanvasSnapshot {
  projectId: string;
  projectName: string;
  canvas: {
    id: string;
    name: string;
    folderId: string | null;
    graph: WorkflowGraph;
    appliedGraph: WorkflowGraph | null;
    draftRevision: number;
    appliedRevision: number;
    publisherId: string | null;
    legacyMultiOutput: boolean;
  };
  publisher: null | { id: string; status: PublisherLifecycle; message: string | null; baseUrl: string; publicModels: string[] };
  providerInstanceIds: string[];
  missingSecretCount: number;
}

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type WorkspaceIntent = "manage_apis" | "unified_endpoint" | "reliability" | "import";

export interface PublisherConnectionTest {
  reachable: boolean;
  latencyMs: number;
  modelCount: number;
  safeSummary: string;
}

export interface WorkflowValidationResult {
  valid: boolean;
  summary?: {
    node_count: number;
    edge_count: number;
    enabled_node_count: number;
    publisher_count: number;
    topological_order: string[];
  } | null;
  errors: string[];
}

export interface WorkflowNodeImpact {
  node_id: string;
  downstream_nodes: string[];
  affected_publishers: string[];
}

export interface ExecutionTrace {
  correlation_id: string;
  publisher_id: string;
  public_model: string;
  streaming: boolean;
  started_at_unix_ms: number;
  total_latency_ms: number;
  first_byte_latency_ms?: number;
  result: "success" | "failure" | "client_disconnected";
  final_error?: string;
  retry_count: number;
  failover_count: number;
  input_tokens?: number;
  output_tokens?: number;
  cached_input_tokens?: number;
  cache_hit?: boolean;
  attempts: Array<{ upstream_id: string; provider_instance: string; latency_ms: number; result: string; error?: string }>;
}

export const getProviderCatalog = () => invoke<ProviderCatalogItem[]>("provider_catalog");
export const getWalletGallery = () => invoke<WalletCard[]>("wallet_gallery");
export const createWalletAsset = (input: { providerId: string; name: string; endpointOverride?: string; monthlyBudgetMicros?: number; currency?: string; apiKey?: string }) => invoke<WalletCard>("create_wallet_asset", { input });
export const updateWalletAsset = (input: { assetId: string; name: string; endpointOverride?: string; enabled: boolean; apiKey?: string; monthlyBudgetMicros?: number; currency?: string }) => invoke<WalletCard[]>("update_wallet_asset", { input });
export const deleteWalletAsset = (assetId: string) => invoke<WalletCard[]>("delete_wallet_asset", { assetId });
export const getWalletAssetImpact = (assetId: string) => invoke<WalletAssetImpact>("wallet_asset_impact", { assetId });
export const probeWalletAsset = (assetId: string) => invoke<InspectionReport>("probe_wallet_asset", { assetId });
export const getDirectEndpoints = () => invoke<DirectEndpointItem[]>("direct_endpoints");
export const createDirectEndpoint = (input: { endpointId?: string; assetId: string; name: string; alias: string; token: string; publicModel: string; upstreamModel: string; timeoutMs?: number; maxRetries?: number; monthlyBudgetMicros?: number; currency?: string }) => invoke<DirectEndpointItem[]>("create_direct_endpoint", { input });
export const updateDirectEndpoint = (input: { endpointId: string; assetId: string; name: string; alias: string; token: string; publicModel: string; upstreamModel: string; timeoutMs?: number; maxRetries?: number; monthlyBudgetMicros?: number; currency?: string }) => invoke<DirectEndpointItem[]>("update_direct_endpoint", { input });
export const deleteDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("delete_direct_endpoint", { input: { endpointId } });
export const startDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("start_direct_endpoint", { input: { endpointId } });
export const pauseDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("pause_direct_endpoint", { input: { endpointId } });
export const testDirectEndpoint = (endpointId: string) => invoke<PublisherConnectionTest>("test_direct_endpoint", { endpointId });
export const getDirectEndpointTemplates = (endpointId: string) => invoke<CodeTemplate[]>("direct_endpoint_templates", { endpointId });
export const getProjectTree = () => invoke<{ projects: ProjectTree }>("project_tree").then((result) => result.projects);
export const createProject = (name: string) => invoke<{ projects: ProjectTree }>("create_project", { input: { name } }).then((result) => result.projects);
export const renameProject = (projectId: string, name: string) => invoke<{ projects: ProjectTree }>("rename_project", { input: { projectId, name } }).then((result) => result.projects);
export const deleteProject = (projectId: string) => invoke<{ projects: ProjectTree }>("delete_project", { input: { projectId } }).then((result) => result.projects);
export const createFolder = (projectId: string, name: string) => invoke<{ projects: ProjectTree }>("create_folder", { input: { projectId, name } }).then((result) => result.projects);
export const renameFolder = (projectId: string, folderId: string, name: string) => invoke<{ projects: ProjectTree }>("rename_folder", { input: { projectId, folderId, name } }).then((result) => result.projects);
export const deleteFolder = (projectId: string, folderId: string) => invoke<{ projects: ProjectTree }>("delete_folder", { input: { projectId, folderId } }).then((result) => result.projects);
export const createCanvas = (projectId: string, folderId: string | undefined, name: string) => invoke<{ projects: ProjectTree }>("create_canvas", { input: { projectId, folderId, name } }).then((result) => result.projects);
export const renameCanvas = (projectId: string, canvasId: string, name: string) => invoke<{ projects: ProjectTree }>("rename_canvas", { input: { projectId, canvasId, name } }).then((result) => result.projects);
export const moveCanvas = (projectId: string, canvasId: string, folderId?: string) => invoke<{ projects: ProjectTree }>("move_canvas", { input: { projectId, canvasId, folderId } }).then((result) => result.projects);
export const duplicateCanvas = (projectId: string, canvasId: string) => invoke<{ projects: ProjectTree }>("duplicate_canvas", { input: { projectId, canvasId } }).then((result) => result.projects);
export const getCanvasGraph = (projectId: string, canvasId: string) => invoke<WorkflowGraph>("canvas_graph", { input: { projectId, canvasId } });
export const getCanvasSnapshot = (projectId: string, canvasId: string) => invoke<CanvasSnapshot>("canvas_snapshot", { input: { projectId, canvasId } });
export const saveCanvasGraph = (projectId: string, canvasId: string, graph: WorkflowGraph) => invoke<CanvasSnapshot>("save_canvas_graph", { input: { projectId, canvasId, graph } });
export const getCanvasNodeImpact = (projectId: string, canvasId: string, nodeId: string) => invoke<WorkflowNodeImpact>("canvas_node_impact", { input: { projectId, canvasId }, nodeId });
export const commitWalletPlacement = (projectId: string, canvasId: string, assetId: string) => invoke<{ projects: ProjectTree }>("commit_wallet_placement", { input: { projectId, canvasId, assetId } }).then((result) => result.projects);
export const runCanvas = (projectId: string, canvasId: string) => invoke<DesktopSnapshot>("run_canvas", { input: { projectId, canvasId } });
export const pauseCanvas = (projectId: string, canvasId: string) => invoke<DesktopSnapshot>("pause_canvas", { input: { projectId, canvasId } });
export const stopCanvas = (projectId: string, canvasId: string) => invoke<DesktopSnapshot>("stop_canvas", { input: { projectId, canvasId } });
export const refreshCanvas = (projectId: string, canvasId: string) => invoke<{ projects: ProjectTree }>("refresh_canvas", { input: { projectId, canvasId } }).then((result) => result.projects);
export const deleteCanvas = (projectId: string, canvasId: string) => invoke<{ projects: ProjectTree }>("delete_canvas", { input: { projectId, canvasId } }).then((result) => result.projects);
export const getProviderInstances = () => invoke<ProviderInstanceItem[]>("provider_instances");
export const validateProviderYaml = (yaml: string) =>
  invoke<{ valid: boolean; providerId?: string; name?: string; adapter?: string; error?: string }>("validate_provider_yaml", { input: { yaml } });
export const upsertProviderInstance = (input: {
  providerId: string;
  instanceId: string;
  endpointOverride?: string;
  enabled: boolean;
  secretFields: Record<string, string>;
}) => invoke<ProviderInstanceItem[]>("upsert_provider_instance", { input });
export const upsertCustomProviderYaml = (input: {
  yaml: string;
  instanceId: string;
  endpointOverride?: string;
  secretFields: Record<string, string>;
}) => invoke<ProviderInstanceItem[]>("upsert_custom_provider_yaml", { input });
export const removeProviderInstance = (providerId: string) =>
  invoke<ProviderInstanceItem[]>("delete_provider_instance", { providerId });
export const storeSecret = (reference: string, value: string) =>
  invoke<void>("store_secret", { input: { reference, value } });
export const removeSecret = (reference: string) => invoke<void>("delete_secret", { reference });
export const runProviderProbe = (providerId: string, allowBillable = false) =>
  invoke<InspectionReport>("run_provider_probe", { input: { providerId, allowBillable } });
export const getInspectionReport = (providerId: string) =>
  invoke<InspectionReport | null>("inspection_report", { providerId });
export const pauseProviderProbe = (providerId: string, paused: boolean) =>
  invoke<void>("pause_provider_probe", { input: { providerId, paused } });
export const createCanvasPublisher = (input: {
  projectId: string;
  canvasId: string;
  id: string;
  name: string;
  port: number;
  basePath?: string;
  token: string;
}) => invoke<DesktopSnapshot>("create_canvas_publisher", { input });
export const compileCanvasGraph = (projectId: string, canvasId: string) => invoke<{ valid: boolean; publicModels: string[]; candidateCount: number; warnings: string[]; errors: string[] }>("compile_canvas_graph", { input: { projectId, canvasId } });
export const removePublisher = (publisherId: string) => invoke<DesktopSnapshot>("delete_publisher", { publisherId });
export const getPublisherPreview = (publisherId: string) => invoke<{ id: string; baseUrl: string; loopbackOnly: boolean; authenticationEnabled: boolean }>("publisher_preview", { publisherId });
export const getPublisherTemplates = (publisherId: string) => invoke<CodeTemplate[]>("publisher_templates", { publisherId });
export const testPublisherConnection = (publisherId: string) => invoke<PublisherConnectionTest>("test_publisher_connection", { publisherId });
export const setWindowMaterialTheme = (dark: boolean) => invoke<void>("set_window_material_theme", { dark });
export const getWorkflowGraph = () => invoke<WorkflowGraph>("workflow_graph");
export const saveWorkflowGraph = (graph: WorkflowGraph) => invoke<WorkflowGraph>("save_workflow_graph", { graph });
export const getWorkspaceUiState = () => invoke<WorkspaceUiState>("workspace_ui_state");
export const saveWorkspaceUiState = (uiState: WorkspaceUiState) =>
  invoke<WorkspaceUiState>("save_workspace_ui_state", { uiState });
export const validateWorkflowGraph = (graph: WorkflowGraph) =>
  invoke<WorkflowValidationResult>("validate_workflow_graph", { graph });
export const getWorkflowNodeImpact = (nodeId: string) =>
  invoke<WorkflowNodeImpact>("workflow_node_impact", { nodeId });
export const getAuditRecords = (limit = 100) => invoke<ExecutionTrace[]>("audit_records", { query: { limit } });
export const exportWorkspace = () => invoke<string>("export_workspace");
export const importWorkspace = (json: string) => invoke<DesktopSnapshot>("import_workspace", { input: { json } });
