#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowValidationResult {
    valid: bool,
    summary: Option<GraphSummary>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublisherConnectionTest {
    reachable: bool,
    latency_ms: u64,
    model_count: usize,
    safe_summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveDocumentExportInput {
    subject_type: String,
    subject_id: String,
    language: TemplateLanguage,
    filename: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkdownContentExportInput { filename: String, markdown: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePathInput { root: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorkspaceAtInput { root: String, name: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretRevealResult { value: String, expires_in_ms: u64, protection: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSnapshot {
    initialized: bool,
    startup_error: Option<String>,
    control: Option<DesktopControlSnapshot>,
    gateway: DesktopGatewaySnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopGatewaySnapshot {
    running: bool,
    base_url: String,
    entry_count: usize,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopControlSnapshot {
    workspace_id: String,
    workspace_name: String,
    recovered_from_backup: bool,
    required_secret_count: usize,
    missing_secret_count: usize,
    provider_count: usize,
    publisher_count: usize,
    supervisor: DesktopSupervisorSnapshot,
    notifications: Vec<DesktopNotification>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSupervisorSnapshot {
    publishers: Vec<DesktopPublisherSnapshot>,
    running_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopPublisherSnapshot {
    id: String,
    status: PublisherLifecycle,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopNotification {
    id: String,
    message: String,
    created_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCatalogItem {
    id: String,
    name: String,
    category: String,
    adapter: String,
    default_base_url: String,
    editable_endpoint: bool,
    auth_fields: Vec<ProviderAuthField>,
    probe_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderAuthField {
    id: String,
    label: String,
    required: bool,
    secret: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInstanceItem {
    id: String,
    provider_id: String,
    name: String,
    endpoint_override: Option<String>,
    enabled: bool,
    secret_fields: Vec<String>,
    inspection_available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInstanceInput {
    provider_id: String,
    instance_id: String,
    endpoint_override: Option<String>,
    enabled: Option<bool>,
    secret_fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretInput {
    reference: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderYamlInput {
    yaml: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomProviderInput {
    yaml: String,
    instance_id: String,
    endpoint_override: Option<String>,
    secret_fields: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderYamlValidation {
    valid: bool,
    provider_id: Option<String>,
    name: Option<String>,
    adapter: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeInput {
    provider_id: String,
    allow_billable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbePauseInput {
    provider_id: String,
    paused: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherInput {
    project_id: String,
    canvas_id: String,
    id: String,
    name: String,
    port: u16,
    base_path: Option<String>,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceImportInput {
    json: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletCard {
    id: String,
    provider_id: String,
    provider_instance_id: Option<String>,
    name: String,
    endpoint_override: Option<String>,
    configured: bool,
    enabled: bool,
    source: String,
    budget_micros: Option<u64>,
    currency: Option<String>,
    request_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    estimated_cost_micros: Option<u64>,
    reference_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletAssetInput {
    provider_id: String,
    name: String,
    endpoint_override: Option<String>,
    monthly_budget_micros: Option<u64>,
    currency: Option<String>,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateWalletAssetInput {
    asset_id: String,
    name: String,
    endpoint_override: Option<String>,
    enabled: bool,
    api_key: Option<String>,
    monthly_budget_micros: Option<u64>,
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectEndpointInput {
    endpoint_id: Option<String>,
    asset_id: String,
    name: String,
    alias: String,
    token: String,
    public_model: String,
    upstream_model: String,
    timeout_ms: Option<u64>,
    max_retries: Option<u32>,
    monthly_budget_micros: Option<u64>,
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectEndpointActionInput { endpoint_id: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectEndpointItem {
    id: String,
    name: String,
    alias: String,
    asset_id: String,
    asset_name: String,
    enabled: bool,
    token_configured: bool,
    base_url: String,
    public_models: Vec<String>,
    request_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ManagedInstanceKind {
    DirectEndpoint,
    Canvas,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ManagedInstanceStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Blocked,
    Failed,
    Unpublished,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstance {
    id: String,
    kind: ManagedInstanceKind,
    name: String,
    ownership: String,
    project_id: Option<String>,
    project_name: Option<String>,
    canvas_id: Option<String>,
    direct_endpoint_id: Option<String>,
    audit_publisher_id: Option<String>,
    asset_names: Vec<String>,
    base_url: Option<String>,
    public_models: Vec<String>,
    token_ready: bool,
    secrets_ready: bool,
    desired_running: bool,
    status: ManagedInstanceStatus,
    blocking_reasons: Vec<String>,
    repair_target: Option<String>,
    draft_revision: Option<u64>,
    applied_revision: Option<u64>,
    has_unapplied_changes: bool,
    request_count: u64,
    last_call_at_ms: Option<u64>,
    last_latency_ms: Option<u64>,
    retry_count: u32,
    failover_count: u32,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceRackSnapshot {
    gateway: DesktopGatewaySnapshot,
    instances: Vec<ManagedInstance>,
    running_count: usize,
    stopped_count: usize,
    blocked_count: usize,
    failed_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstanceActionInput {
    instance_id: String,
    #[serde(default)]
    allow_warnings: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstanceKindInput {
    kind: ManagedInstanceKind,
    #[serde(default)]
    allow_warnings: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstanceActionResult {
    instance_id: String,
    previous_status: ManagedInstanceStatus,
    next_status: ManagedInstanceStatus,
    success: bool,
    skipped: bool,
    message: String,
    repair_target: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceBatchResult {
    total: usize,
    succeeded: usize,
    failed: usize,
    skipped: usize,
    gateway_refreshed: bool,
    results: Vec<ManagedInstanceActionResult>,
    snapshot: InstanceRackSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletAssetImpact {
    direct_endpoints: Vec<String>,
    canvases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectInput {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectActionInput {
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameProjectInput {
    project_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderInput {
    project_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameFolderInput {
    project_id: String,
    folder_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderActionInput {
    project_id: String,
    folder_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanvasInput {
    project_id: String,
    folder_id: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanvasActionInput {
    project_id: String,
    canvas_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameCanvasInput {
    project_id: String,
    canvas_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveCanvasInput {
    project_id: String,
    canvas_id: String,
    folder_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveCanvasGraphInput {
    project_id: String,
    canvas_id: String,
    graph: WorkflowGraph,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlacementInput {
    project_id: String,
    canvas_id: String,
    asset_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTreeSnapshot {
    projects: WorkspaceProjects,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasSnapshot {
    project_id: String,
    project_name: String,
    canvas: Canvas,
    publisher: Option<CanvasPublisherSnapshot>,
    asset_ids: Vec<String>,
    missing_secret_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasPublisherSnapshot {
    id: String,
    status: PublisherLifecycle,
    message: Option<String>,
    base_url: String,
    public_models: Vec<String>,
}
