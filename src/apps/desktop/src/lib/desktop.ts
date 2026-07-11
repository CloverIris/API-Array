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

export interface WorkflowGraph {
  schema_version: number;
  id: string;
  nodes: Array<{
    id: string;
    name: string;
    kind: "adapter" | "probe" | "transform" | "router" | "guard" | "publisher" | "group";
    enabled: boolean;
    inputs: Array<{ id: string; data_type: string }>;
    outputs: Array<{ id: string; data_type: string }>;
    config: Record<string, unknown>;
  }>;
  edges: Array<{ id: string; from: { node: string; port: string }; to: { node: string; port: string } }>;
}

export interface WorkspaceUiState {
  schemaVersion: 2;
  themePreference: ThemePreference;
  viewMode: ViewMode;
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
}

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type ViewMode = "simple" | "professional";
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
export const createPublisher = (input: {
  id: string;
  name: string;
  providerInstance: string;
  publicModel: string;
  upstreamModel: string;
  port: number;
  basePath?: string;
  token: string;
  timeoutMs?: number;
  maxRetries?: number;
}) => invoke<DesktopSnapshot>("create_publisher", { input });
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
