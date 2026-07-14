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
  notifications: Array<{ id: string; message: string; createdAt: number; level: string; objectId: string; occurrenceCount: number; read: boolean }>;
}

export interface DesktopSnapshot {
  initialized: boolean;
  startupError: string | null;
  control: ControlSnapshot | null;
  gateway: { running: boolean; baseUrl: string; entryCount: number; error: string | null };
}

export const getDesktopSnapshot = () => invoke<DesktopSnapshot>("desktop_snapshot");
export interface ApplicationVersion { version: string; channel: string; displayVersion: string }
export const getApplicationVersion = () => invoke<ApplicationVersion>("application_version");

export const initializeWorkspace = (name: string) =>
  invoke<DesktopSnapshot>("initialize_workspace", { name });

export const resetWorkspaceForGraphV3 = (confirmation: string, backupFirst: boolean) =>
  invoke<DesktopSnapshot>("reset_workspace_for_graph_v3", {
    input: { confirmation, backupFirst },
  });

export interface RouteSimulationResult {
  publicModel: string;
  strategy: SelectionStrategy;
  selectedProviderNodeId: string | null;
  selectedUpstreamModel: string | null;
  orderedCandidates: string[];
  explanation: string;
}

export const simulateCanvasRoute = (projectId: string, canvasId: string, publicModel: string) =>
  invoke<RouteSimulationResult>("simulate_canvas_route", {
    input: { projectId, canvasId, publicModel },
  });

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

export type TemplateLanguage = "curl" | "python" | "javascript_typescript" | "go" | "rust" | "java" | "csharp" | "cpp";
export interface LiveDocumentFact { label: string; value: string }
export type LiveDocumentSection =
  | { kind: "paragraph"; title: string; body: string }
  | { kind: "facts"; title: string; items: LiveDocumentFact[] }
  | { kind: "steps"; title: string; items: string[] }
  | { kind: "code"; title: string; block: { language: TemplateLanguage; title: string; code: string } }
  | { kind: "note"; title: string; body: string; tone: "info" | "warning" | "security" };
export interface LiveDocument { schema_version: number; title: string; summary: string; language: TemplateLanguage; endpoint_status: string; sections: LiveDocumentSection[]; markdown: string }

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

export type ManagedInstanceKind = "direct_endpoint" | "canvas";
export type ManagedInstanceStatus = "running" | "stopped" | "starting" | "stopping" | "blocked" | "failed" | "unpublished";
export interface ManagedInstance {
  id: string;
  kind: ManagedInstanceKind;
  name: string;
  ownership: string;
  projectId: string | null;
  projectName: string | null;
  canvasId: string | null;
  directEndpointId: string | null;
  auditPublisherId: string | null;
  assetNames: string[];
  baseUrl: string | null;
  publicModels: string[];
  tokenReady: boolean;
  secretsReady: boolean;
  desiredRunning: boolean;
  status: ManagedInstanceStatus;
  blockingReasons: string[];
  repairTarget: string | null;
  draftRevision: number | null;
  appliedRevision: number | null;
  hasUnappliedChanges: boolean;
  requestCount: number;
  lastCallAtMs: number | null;
  lastLatencyMs: number | null;
  retryCount: number;
  failoverCount: number;
  lastError: string | null;
}
export interface ControlCenterSnapshot {
  gateway: DesktopSnapshot["gateway"];
  instances: ManagedInstance[];
  runningCount: number;
  stoppedCount: number;
  blockedCount: number;
  failedCount: number;
}
export interface GatewayEntryStatus {
  prefix: string;
  publisherId: string;
  mounted: boolean;
  blocked: boolean;
  reason: string | null;
}
export interface GatewayStatus {
  configuredAddress: string;
  configuredPort: number;
  boundAddress: string | null;
  boundPort: number | null;
  running: boolean;
  entries: GatewayEntryStatus[];
  blockedEntries: GatewayEntryStatus[];
  duplicateRoutes: string[];
  error: string | null;
}
export interface ManagedInstanceActionResult {
  instanceId: string;
  previousStatus: ManagedInstanceStatus;
  nextStatus: ManagedInstanceStatus;
  success: boolean;
  skipped: boolean;
  message: string;
  repairTarget: string | null;
}
export interface InstanceBatchResult {
  total: number;
  succeeded: number;
  failed: number;
  skipped: number;
  gatewayRefreshed: boolean;
  results: ManagedInstanceActionResult[];
  snapshot: ControlCenterSnapshot;
}

export interface WalletAssetImpact { directEndpoints: string[]; canvases: string[]; }

export interface ProjectTree {
  projects: Record<string, {
    id: string;
    name: string;
    folders: Record<string, { id: string; name: string; canvasIds: string[] }>;
    canvases: Record<string, { id: string; name: string; folderId: string | null; graph?: WorkflowGraph; appliedGraph?: WorkflowGraph | null; draftRevision: number; appliedRevision: number; publisherId: string | null }>;
  }>;
}

export type WorkflowPortType = "candidate" | "service_plan" | "health_signal";
export interface WorkflowGraph {
  schema_version: number;
  id: string;
  nodes: Array<{
    id: string;
    name: string;
    kind: "provider" | "composer" | "middleware" | "probe" | "publisher" | "group";
    enabled: boolean;
    inputs: Array<{ id: string; data_type: WorkflowPortType }>;
    outputs: Array<{ id: string; data_type: WorkflowPortType }>;
    config: WorkflowNodeConfig;
  }>;
  edges: Array<{ id: string; from: { node: string; port: string }; to: { node: string; port: string }; enabled: boolean; label?: string | null }>;
}

export type SelectionStrategy = "priority_failover" | "weighted_round_robin" | "lowest_latency";
export type StandardRouteError = "AUTH_FAILED" | "RATE_LIMITED" | "BALANCE_EXHAUSTED" | "MODEL_UNAVAILABLE" | "PROVIDER_TIMEOUT" | "NETWORK_UNREACHABLE" | "TLS_FAILED" | "INVALID_REQUEST" | "INVALID_RESPONSE" | "CONTENT_REJECTED" | "INTERNAL_RUNTIME_ERROR";
export interface CandidateBinding { provider_node_id: string; upstream_model: string; priority: number; weight: number; }
export interface PublicModelRoute { public_model: string; candidates: CandidateBinding[]; }
export type MiddlewareKind =
  | { kind: "request_defaults"; temperature?: number | null; top_p?: number | null; max_output_tokens?: number | null }
  | { kind: "model_policy"; allowed_models: string[]; max_output_tokens?: number | null }
  | { kind: "rate_limit"; requests_per_minute: number; max_concurrent: number }
  | { kind: "budget_monitor"; warning_thresholds: number[] };
export type WorkflowNodeConfig =
  | { type: "provider"; asset_id: string; selected_models: string[] }
  | { type: "composer"; strategy: SelectionStrategy; timeout_ms: number; max_retries: number; failover_on: StandardRouteError[]; allow_capability_degradation: boolean; latency_hysteresis_ms: number; routes: PublicModelRoute[] }
  | { type: "middleware"; middleware: MiddlewareKind }
  | { type: "probe"; provider_node_id: string; interval_seconds: number; timeout_ms: number; failure_threshold: number; recovery_threshold: number; safe_only: boolean }
  | { type: "publisher"; publisher_id?: string | null }
  | { type: "group"; member_ids: string[]; collapsed: boolean };

export interface CompilationIssue {
  code: string;
  severity: "error" | "warning";
  message: string;
  nodeId?: string | null;
  edgeId?: string | null;
  field?: string | null;
  fixTarget?: string | null;
}

export interface CanvasCompilationReport {
  valid: boolean;
  publicModels: string[];
  candidates: Array<{ providerNodeId: string; assetId: string; publicModel: string; upstreamModel: string; priority: number; weight: number }>;
  candidateCount: number;
  selectedStrategy: SelectionStrategy;
  middlewareOrder: string[];
  unreachableNodes: string[];
  missingSecrets: string[];
  staleRuntime: boolean;
  requiresConfirmation: boolean;
  warnings: CompilationIssue[];
  errors: CompilationIssue[];
}
export interface CanvasRuntimeSnapshot {
  projectId: string;
  canvasId: string;
  draftRevision: number;
  appliedRevision: number;
  staleRuntime: boolean;
  publisherRunning: boolean;
  strategy: SelectionStrategy;
  providers: Array<{ providerNodeId: string; upstreamId: string; status: "healthy" | "degraded" | "unhealthy" | "unknown" | "paused"; latencyEwmaMs?: number | null; consecutiveFailures: number }>;
  preferredCandidates: Record<string, string>;
  middleware: string[];
}
export type GraphTemplateKind = "single_provider" | "priority_failover" | "multi_model" | "weighted_round_robin" | "lowest_latency" | "failover_with_probe" | "guarded_failover";
export interface GraphNodeDefinition { kind: WorkflowGraph["nodes"][number]["kind"]; label: string; description: string; inputs: WorkflowGraph["nodes"][number]["inputs"]; outputs: WorkflowGraph["nodes"][number]["outputs"]; }

export type CanvasTab = "overview" | "workflow" | "routes" | "publisher" | "docs" | "runs";

export interface WorkspaceUiState {
  schemaVersion: 6;
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
  };
  publisher: null | { id: string; status: PublisherLifecycle; message: string | null; baseUrl: string; publicModels: string[] };
  assetIds: string[];
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

export interface AuditQuery {
  limit?: number;
  offset?: number;
  result?: "all" | "success" | "failure" | "client_disconnected";
  publisherId?: string;
  model?: string;
  fromMs?: number;
  toMs?: number;
}

export interface StoredNotification {
  id: string;
  level: "silent" | "notification_center" | "system" | "action_required" | string;
  object_id: string;
  summary: string;
  occurrence_count: number;
  first_seen_at_ms: number;
  last_seen_at_ms: number;
  read: boolean;
}

export const getProviderCatalog = () => invoke<ProviderCatalogItem[]>("provider_catalog");
export const getWalletGallery = () => invoke<WalletCard[]>("wallet_gallery");
export const createWalletAsset = (input: { providerId: string; name: string; endpointOverride?: string; monthlyBudgetMicros?: number; currency?: string; apiKey?: string }) => invoke<WalletCard>("create_wallet_asset", { input });
export const updateWalletAsset = (input: { assetId: string; name: string; endpointOverride?: string; enabled: boolean; apiKey?: string; monthlyBudgetMicros?: number; currency?: string }) => invoke<WalletCard[]>("update_wallet_asset", { input });
export const deleteWalletAsset = (assetId: string) => invoke<WalletCard[]>("delete_wallet_asset", { assetId });
export const deleteWalletAssetSecret = (assetId: string) => invoke<WalletCard[]>("delete_wallet_asset_secret", { assetId });
export interface SecretRevealResult { value: string; expiresInMs: number; protection: "windows_hello" }
export const revealWalletSecret = (assetId: string) => invoke<SecretRevealResult>("reveal_wallet_secret", { assetId });
export const getWalletAssetImpact = (assetId: string) => invoke<WalletAssetImpact>("wallet_asset_impact", { assetId });
export const probeWalletAsset = (assetId: string) => invoke<InspectionReport>("probe_wallet_asset", { assetId });
export const getDirectEndpoints = () => invoke<DirectEndpointItem[]>("direct_endpoints");
export const createDirectEndpoint = (input: { endpointId?: string; assetId: string; name: string; alias: string; token: string; publicModel: string; upstreamModel: string; timeoutMs?: number; maxRetries?: number; monthlyBudgetMicros?: number; currency?: string }) => invoke<DirectEndpointItem[]>("create_direct_endpoint", { input });
export const updateDirectEndpoint = (input: { endpointId: string; assetId: string; name: string; alias: string; token: string; publicModel: string; upstreamModel: string; timeoutMs?: number; maxRetries?: number; monthlyBudgetMicros?: number; currency?: string }) => invoke<DirectEndpointItem[]>("update_direct_endpoint", { input });
export const deleteDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("delete_direct_endpoint_safe", { input: { endpointId } });
export const startDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("start_direct_endpoint", { input: { endpointId } });
export const pauseDirectEndpoint = (endpointId: string) => invoke<DirectEndpointItem[]>("pause_direct_endpoint", { input: { endpointId } });
export const getControlCenterSnapshot = () => invoke<ControlCenterSnapshot>("control_center_snapshot");
export const startManagedInstance = (instanceId: string, allowWarnings = false) => invoke<InstanceBatchResult>("start_managed_instance", { input: { instanceId, allowWarnings } });
export const stopManagedInstance = (instanceId: string) => invoke<InstanceBatchResult>("stop_managed_instance", { input: { instanceId } });
export const testManagedInstance = (instanceId: string) => invoke<PublisherConnectionTest>("test_managed_instance", { input: { instanceId } });
export const startAllInstances = () => invoke<InstanceBatchResult>("start_all_instances");
export const stopAllInstances = () => invoke<InstanceBatchResult>("stop_all_instances");
export const startInstancesByKind = (kind: ManagedInstanceKind, allowWarnings = false) => invoke<InstanceBatchResult>("start_instances_by_kind", { input: { kind, allowWarnings } });
export const stopInstancesByKind = (kind: ManagedInstanceKind) => invoke<InstanceBatchResult>("stop_instances_by_kind", { input: { kind } });
export const testDirectEndpoint = (endpointId: string) => invoke<PublisherConnectionTest>("test_direct_endpoint", { endpointId });
export const getDirectEndpointTemplates = (endpointId: string) => invoke<CodeTemplate[]>("direct_endpoint_templates", { endpointId });
export const getDirectEndpointLiveDocument = (endpointId: string, language: TemplateLanguage) => invoke<LiveDocument>("direct_endpoint_live_document", { endpointId, language });
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
export const saveCanvasGraph = (projectId: string, canvasId: string, expectedDraftRevision: number, graph: WorkflowGraph) => invoke<CanvasSnapshot>("save_canvas_graph", { input: { projectId, canvasId, expectedDraftRevision, graph } });
export const getGraphNodeCatalog = () => invoke<GraphNodeDefinition[]>("graph_node_catalog");
export const applyCanvasTemplate = (projectId: string, canvasId: string, expectedDraftRevision: number, template: GraphTemplateKind, assetIds: string[]) => invoke<CanvasSnapshot>("apply_canvas_template", { input: { projectId, canvasId, expectedDraftRevision, template, assetIds } });
export const getCanvasRuntimeSnapshot = (projectId: string, canvasId: string) => invoke<CanvasRuntimeSnapshot>("canvas_runtime_snapshot", { input: { projectId, canvasId } });
export const runCanvasProbe = (projectId: string, canvasId: string, probeNodeId: string) => invoke<InspectionReport>("run_canvas_probe", { input: { projectId, canvasId, probeNodeId } });
export const setCanvasProbeSchedule = (projectId: string, canvasId: string, probeNodeId: string, enabled: boolean) => invoke<void>("set_canvas_probe_schedule", { input: { projectId, canvasId, probeNodeId, enabled } });
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
export const compileCanvasGraph = (projectId: string, canvasId: string) => invoke<CanvasCompilationReport>("compile_canvas_graph", { input: { projectId, canvasId } });
export const validateCanvasRuntime = (projectId: string, canvasId: string) => invoke<CanvasCompilationReport>("validate_canvas_runtime", { input: { projectId, canvasId } });
export const removePublisher = (publisherId: string) => invoke<DesktopSnapshot>("delete_publisher", { publisherId });
export const getPublisherPreview = (publisherId: string) => invoke<{ id: string; baseUrl: string; loopbackOnly: boolean; authenticationEnabled: boolean }>("publisher_preview", { publisherId });
export const getPublisherTemplates = (publisherId: string) => invoke<CodeTemplate[]>("publisher_templates", { publisherId });
export const getCanvasLiveDocument = (publisherId: string, language: TemplateLanguage) => invoke<LiveDocument>("canvas_live_document", { publisherId, language });
export const saveLiveDocumentMarkdown = (subjectType: "direct" | "canvas", subjectId: string, language: TemplateLanguage, filename: string) => invoke<string>("save_live_document_markdown", { input: { subjectType, subjectId, language, filename } });
export const saveMarkdownDocument = (filename: string, markdown: string) => invoke<string>("save_markdown_document", { input: { filename, markdown } });

export interface WorkspaceStorageStatus { healthy: boolean; integrity_message: string; database_bytes: number; revision: number; last_backup_at_ms: number | null }
export interface WorkspaceBackup { path: string; created_at_ms: number; database_bytes: number }
export interface WorkspaceLocation { id: string; name: string; root: string; last_opened_at_ms: number }
export const getWorkspaceStorageStatus = () => invoke<WorkspaceStorageStatus>("workspace_storage_status");
export const getGatewayStatus = () => invoke<GatewayStatus>("gateway_status");
export const updateGatewaySettings = (listenAddress: string, port: number) => invoke<DesktopSnapshot>("update_gateway_settings", { input: { listenAddress, port } });
export const verifyWorkspace = () => invoke<WorkspaceStorageStatus>("verify_workspace");
export const backupWorkspace = () => invoke<WorkspaceBackup>("backup_workspace");
export const compactWorkspace = () => invoke<WorkspaceStorageStatus>("compact_workspace");
export const getWorkspaceLocations = () => invoke<WorkspaceLocation[]>("workspace_locations");
export const createWorkspaceAt = (root: string, name: string) => invoke<DesktopSnapshot>("create_workspace_at", { input: { root, name } });
export const createDefaultWorkspace = (name: string) => invoke<DesktopSnapshot>("create_default_workspace", { name });
export const openWorkspaceAt = (root: string) => invoke<DesktopSnapshot>("open_workspace_at", { input: { root } });
export const relocateWorkspace = (root: string) => invoke<DesktopSnapshot>("relocate_workspace", { input: { root } });
export const testPublisherConnection = (publisherId: string) => invoke<PublisherConnectionTest>("test_publisher_connection", { publisherId });
export const setWindowMaterialTheme = (dark: boolean | null) => invoke<void>("set_window_material_theme", { dark });
export const getWorkspaceUiState = () => invoke<WorkspaceUiState>("workspace_ui_state");
export const saveWorkspaceUiState = (uiState: WorkspaceUiState) =>
  invoke<WorkspaceUiState>("save_workspace_ui_state", { uiState });
export const validateWorkflowGraph = (graph: WorkflowGraph) =>
  invoke<WorkflowValidationResult>("validate_workflow_graph", { graph });
export const getAuditRecords = (query: number | AuditQuery = 100) => invoke<ExecutionTrace[]>("audit_records", { query: typeof query === "number" ? { limit: query } : query });
export const getNotifications = (query: { unreadOnly?: boolean; limit?: number } = {}) => invoke<StoredNotification[]>("notifications", { query });
export const markNotificationsRead = (ids: string[]) => invoke<void>("mark_notifications_read", { input: { ids } });
export const clearReadNotifications = () => invoke<number>("clear_read_notifications");
export interface DesktopNotificationPreference { systemNotifications: boolean }
export const getDesktopNotificationPreference = () => invoke<DesktopNotificationPreference>("desktop_notification_preference");
export const saveDesktopNotificationPreference = (systemNotifications: boolean) => invoke<DesktopNotificationPreference>("save_desktop_notification_preference", { input: { systemNotifications } });
export const exportWorkspace = () => invoke<string>("export_workspace");
export const importWorkspace = (json: string) => invoke<DesktopSnapshot>("import_workspace", { input: { json } });
