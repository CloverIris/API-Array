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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSnapshot {
    initialized: bool,
    startup_error: Option<String>,
    control: Option<DesktopControlSnapshot>,
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
    provider_instance: String,
    public_model: String,
    upstream_model: String,
    port: u16,
    base_path: Option<String>,
    token: String,
    timeout_ms: Option<u64>,
    max_retries: Option<u32>,
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
    configured: bool,
    enabled: bool,
    source: String,
    budget_micros: Option<u64>,
    currency: Option<String>,
    request_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    estimated_cost_micros: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletAssetInput {
    provider_id: String,
    name: String,
    endpoint_override: Option<String>,
    monthly_budget_micros: Option<u64>,
    currency: Option<String>,
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
    provider_instance_ids: Vec<String>,
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
