use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use apiarray_core::{
    SCHEMA_VERSION,
    catalog::builtin_provider_manifests,
    graph::{
        Edge, Endpoint, GraphSummary, Node, NodeImpact, NodeKind, Port, PortType, WorkflowGraph,
    },
    publisher::PublisherSummary,
    routing::{RoutePolicy, StandardError},
    runtime::{ModelRoute, ProviderInstance, RuntimeConfig, RuntimePublisher, UpstreamRoute},
    secret::SecretRef,
    templates::{CodeTemplate, TemplateContext, generate_templates},
    workspace::{WorkspaceLoad, WorkspacePackage, WorkspaceRuntimeState, load_workspace_json},
};
use apiarray_runtime::{
    control::{ControlPlane, ControlPlaneSnapshot},
    inspection::{InspectionRepository, ProviderProbeRunner},
    persistence::WorkspaceRepository,
    resilience::{ExecutionTrace, JsonlAuditSink, read_jsonl_audit},
    secret::{SecretStore, SecretValue, StoreSecretResolver, WindowsCredentialStore},
    supervisor::PublisherLifecycle,
    transport::TransportConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Manager, State, WebviewWindow, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tokio::sync::Mutex;

const CREDENTIAL_SERVICE: &str = "API ARRAY";
const DEFAULT_WORKSPACE_ID: &str = "default";
const UI_STATE_FILE: &str = "ui-state.json";
const UI_STATE_SCHEMA_VERSION: u32 = 2;

struct DesktopState {
    repository: WorkspaceRepository,
    secret_store: Arc<WindowsCredentialStore>,
    probe_runner: ProviderProbeRunner,
    inspection_reports: InspectionRepository,
    paused_probes: Mutex<BTreeSet<String>>,
    control_plane: Mutex<Option<ControlPlane>>,
    startup_error: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct WorkspaceUiState {
    schema_version: u32,
    #[serde(default)]
    theme_preference: ThemePreference,
    #[serde(default)]
    view_mode: ViewMode,
    #[serde(default = "default_page")]
    last_page: String,
    #[serde(default)]
    workspace_intent: WorkspaceIntent,
    shell: ShellUiState,
    #[serde(default)]
    workflows: BTreeMap<String, WorkflowUiState>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ViewMode {
    #[default]
    Simple,
    Professional,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkspaceIntent {
    #[default]
    ManageApis,
    UnifiedEndpoint,
    Reliability,
    Import,
}

fn default_page() -> String {
    "overview".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ShellUiState {
    left_sidebar_collapsed: bool,
    right_inspector_open: bool,
    right_inspector_pinned: bool,
    left_width: u16,
    right_width: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct WorkflowUiState {
    viewport: CanvasViewport,
    #[serde(default)]
    node_positions: BTreeMap<String, CanvasPosition>,
    #[serde(default)]
    collapsed_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CanvasViewport {
    x: f64,
    y: f64,
    zoom: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CanvasPosition {
    x: f64,
    y: f64,
}

impl Default for WorkspaceUiState {
    fn default() -> Self {
        Self {
            schema_version: UI_STATE_SCHEMA_VERSION,
            theme_preference: ThemePreference::System,
            view_mode: ViewMode::Simple,
            last_page: default_page(),
            workspace_intent: WorkspaceIntent::ManageApis,
            shell: ShellUiState {
                left_sidebar_collapsed: false,
                right_inspector_open: false,
                right_inspector_pinned: false,
                left_width: 188,
                right_width: 320,
            },
            workflows: BTreeMap::new(),
        }
    }
}

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

#[tauri::command]
async fn desktop_snapshot(state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    snapshot(&state).await
}

#[tauri::command]
fn provider_catalog() -> Result<Vec<ProviderCatalogItem>, String> {
    builtin_provider_manifests()
        .map(|manifests| manifests.into_iter().map(provider_catalog_item).collect())
        .map_err(|error| error.message)
}

#[tauri::command]
fn provider_instances(state: State<'_, DesktopState>) -> Result<Vec<ProviderInstanceItem>, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .runtime
        .providers
        .values()
        .map(|instance| {
            Ok(ProviderInstanceItem {
                id: instance.id.clone(),
                provider_id: instance.manifest.provider.id.clone(),
                name: instance.manifest.provider.name.clone(),
                endpoint_override: instance.endpoint_override.clone(),
                enabled: instance.enabled,
                secret_fields: instance.secret_refs.keys().cloned().collect(),
                inspection_available: state
                    .inspection_reports
                    .load(&instance.id)
                    .map_err(safe_error)?
                    .is_some(),
            })
        })
        .collect()
}

#[tauri::command]
fn validate_provider_yaml(input: ProviderYamlInput) -> ProviderYamlValidation {
    match apiarray_core::provider::ProviderManifest::from_yaml(&input.yaml) {
        Ok(manifest) => ProviderYamlValidation {
            valid: true,
            provider_id: Some(manifest.provider.id),
            name: Some(manifest.provider.name),
            adapter: Some(manifest.adapter.id),
            error: None,
        },
        Err(error) => ProviderYamlValidation {
            valid: false,
            provider_id: None,
            name: None,
            adapter: None,
            error: Some(error.message),
        },
    }
}

#[tauri::command]
async fn upsert_custom_provider_yaml(
    input: CustomProviderInput,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let manifest = apiarray_core::provider::ProviderManifest::from_yaml(&input.yaml)
        .map_err(|error| error.message)?;
    let instance_id = normalize_id(&input.instance_id, "custom-provider");
    let mut secret_refs = BTreeMap::new();
    for (field, reference) in input.secret_fields {
        secret_refs.insert(
            field,
            SecretRef::parse(reference).map_err(|error| error.message)?,
        );
    }
    let mut workspace = load_workspace(&state.repository)?;
    workspace.runtime.providers.insert(
        instance_id.clone(),
        ProviderInstance {
            id: instance_id,
            manifest,
            endpoint_override: input.endpoint_override,
            secret_refs,
            enabled: true,
        },
    );
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
async fn upsert_provider_instance(
    input: ProviderInstanceInput,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let manifest = builtin_provider_manifests()
        .map_err(|error| error.message)?
        .into_iter()
        .find(|manifest| manifest.provider.id == input.provider_id)
        .ok_or_else(|| "未找到内置 Provider；自定义 YAML 需要先通过校验。".to_owned())?;
    let instance_id = normalize_id(&input.instance_id, "provider");
    let mut secret_refs = BTreeMap::new();
    for (field, reference) in input.secret_fields {
        let reference = SecretRef::parse(reference).map_err(|error| error.message)?;
        secret_refs.insert(field, reference);
    }
    workspace.runtime.providers.insert(
        instance_id.clone(),
        ProviderInstance {
            id: instance_id,
            manifest,
            endpoint_override: input.endpoint_override,
            secret_refs,
            enabled: input.enabled.unwrap_or(true),
        },
    );
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
async fn delete_provider_instance(
    provider_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let removed = workspace
        .runtime
        .providers
        .remove(&provider_id)
        .ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    for reference in removed.secret_refs.values() {
        let _ = state.secret_store.delete(reference);
    }
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
fn store_secret(input: SecretInput, state: State<'_, DesktopState>) -> Result<(), String> {
    let reference = SecretRef::parse(input.reference).map_err(|error| error.message)?;
    state
        .secret_store
        .put(&reference, SecretValue::new(input.value))
        .map_err(safe_error)
}

#[tauri::command]
fn delete_secret(reference: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let reference = SecretRef::parse(reference).map_err(|error| error.message)?;
    state.secret_store.delete(&reference).map_err(safe_error)
}

#[tauri::command]
async fn run_provider_probe(
    input: ProbeInput,
    state: State<'_, DesktopState>,
) -> Result<apiarray_core::inspection::InspectionReport, String> {
    if state
        .paused_probes
        .lock()
        .await
        .contains(&input.provider_id)
    {
        return Err("该 Provider 的体检已暂停。".to_owned());
    }
    let workspace = load_workspace(&state.repository)?;
    let instance = workspace
        .runtime
        .providers
        .get(&input.provider_id)
        .ok_or_else(|| "Provider 实例不存在。".to_owned())?
        .clone();
    let resolver = StoreSecretResolver::new(state.secret_store.clone());
    let report = state
        .probe_runner
        .run(&instance, &resolver, input.allow_billable)
        .await
        .map_err(safe_error)?;
    state.inspection_reports.save(&report).map_err(safe_error)?;
    Ok(report)
}

#[tauri::command]
fn inspection_report(
    provider_id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<apiarray_core::inspection::InspectionReport>, String> {
    state
        .inspection_reports
        .load(&provider_id)
        .map_err(safe_error)
}

#[tauri::command]
async fn pause_provider_probe(
    input: ProbePauseInput,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let mut paused = state.paused_probes.lock().await;
    if input.paused {
        paused.insert(input.provider_id);
    } else {
        paused.remove(&input.provider_id);
    }
    Ok(())
}

#[tauri::command]
async fn create_publisher(
    input: PublisherInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let provider = workspace
        .runtime
        .providers
        .get(&input.provider_instance)
        .ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    if input.token.trim().is_empty() {
        return Err("Publisher Token 不能为空。".to_owned());
    }
    let publisher_id = normalize_id(&input.id, "publisher");
    let token_ref = SecretRef::parse(format!("secret://publisher/{publisher_id}"))
        .map_err(|error| error.message)?;
    state
        .secret_store
        .put(&token_ref, SecretValue::new(input.token))
        .map_err(safe_error)?;
    let base_path = input.base_path.unwrap_or_else(|| "/v1".to_owned());
    let route = ModelRoute {
        public_model: input.public_model.clone(),
        policy: RoutePolicy {
            schema_version: SCHEMA_VERSION,
            id: format!("{publisher_id}-default-policy"),
            timeout_ms: input.timeout_ms.unwrap_or(30_000),
            max_retries: input.max_retries.unwrap_or(2),
            failover_on: HashSet::from([
                StandardError::ProviderTimeout,
                StandardError::NetworkUnreachable,
                StandardError::RateLimited,
            ]),
        },
        upstreams: vec![UpstreamRoute {
            id: format!("{publisher_id}-primary"),
            provider_instance: provider.id.clone(),
            upstream_model: input.upstream_model,
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        }],
    };
    workspace.runtime.publishers.insert(
        publisher_id.clone(),
        RuntimePublisher {
            config: apiarray_core::publisher::PublisherConfig {
                schema_version: SCHEMA_VERSION,
                id: publisher_id.clone(),
                name: input.name,
                listen_address: "127.0.0.1".parse().map_err(|_| "回环地址无效。")?,
                port: input.port,
                base_path,
                require_token: true,
                token_ref: Some(token_ref),
            },
            routes: vec![route],
        },
    );
    install_publisher_workflow(&mut workspace.graph, &publisher_id);
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    snapshot(&state).await
}

#[tauri::command]
async fn delete_publisher(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let removed = workspace
        .runtime
        .publishers
        .remove(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    if let Some(reference) = removed.config.token_ref.as_ref() {
        let _ = state.secret_store.delete(reference);
    }
    workspace
        .runtime_state
        .enabled_publishers
        .remove(&publisher_id);
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    snapshot(&state).await
}

#[tauri::command]
fn publisher_preview(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<PublisherSummary, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?
        .config
        .validate()
        .map_err(|error| error.message)
}

#[tauri::command]
fn publisher_templates(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<CodeTemplate>, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher = workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    let summary = publisher.config.validate().map_err(|error| error.message)?;
    generate_templates(&TemplateContext {
        base_url: summary.base_url,
        model: publisher
            .routes
            .first()
            .map(|route| route.public_model.clone())
            .unwrap_or_else(|| "default".to_owned()),
        stream: false,
        token_placeholder: "${APIARRAY_PUBLISHER_TOKEN}".to_owned(),
    })
    .map_err(|error| error.message)
}

#[tauri::command]
async fn test_publisher_connection(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<PublisherConnectionTest, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher = workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    let summary = publisher.config.validate().map_err(|error| error.message)?;
    let token = match publisher.config.token_ref.as_ref() {
        Some(reference) => Some(state.secret_store.get(reference).map_err(safe_error)?),
        None => None,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "无法创建本地 Publisher 自检客户端。".to_owned())?;
    let started = Instant::now();
    let mut request = client.get(format!("{}/models", summary.base_url.trim_end_matches('/')));
    if let Some(token) = token.as_ref() {
        request = request.bearer_auth(token.expose());
    }
    let response = request
        .send()
        .await
        .map_err(|_| "无法连接本地 Publisher。请确认它已启动且端口未被其他程序占用。".to_owned())?;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !response.status().is_success() {
        return Err(format!(
            "本地 Publisher 返回 HTTP {}。",
            response.status().as_u16()
        ));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|_| "本地 Publisher 返回了无法识别的模型列表。".to_owned())?;
    let model_count = model_count_from_payload(&payload);
    Ok(PublisherConnectionTest {
        reachable: true,
        latency_ms,
        model_count,
        safe_summary: "本地端点可访问，鉴权与 /v1/models 已通过。".to_owned(),
    })
}

fn model_count_from_payload(payload: &Value) -> usize {
    payload
        .get("data")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

#[tauri::command]
fn set_window_material_theme(dark: bool, window: WebviewWindow) -> Result<(), String> {
    window
        .set_theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .map_err(|error| format!("无法同步窗口主题：{error}"))?;
    apply_native_material(&window, Some(dark));
    Ok(())
}

#[tauri::command]
fn workflow_graph(state: State<'_, DesktopState>) -> Result<WorkflowGraph, String> {
    Ok(load_workspace(&state.repository)?.graph)
}

#[tauri::command]
fn workspace_ui_state(state: State<'_, DesktopState>) -> WorkspaceUiState {
    read_ui_state(&state.repository)
}

#[tauri::command]
fn save_workspace_ui_state(
    mut ui_state: WorkspaceUiState,
    state: State<'_, DesktopState>,
) -> Result<WorkspaceUiState, String> {
    sanitize_ui_state(&mut ui_state)?;
    let path = state.repository.root().join(UI_STATE_FILE);
    fs::create_dir_all(state.repository.root())
        .map_err(|error| format!("无法创建 UI 状态目录：{error}"))?;
    let bytes = serde_json::to_vec_pretty(&ui_state)
        .map_err(|error| format!("无法序列化 UI 状态：{error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("无法写入 UI 状态：{error}"))?;
    Ok(ui_state)
}

#[tauri::command]
fn validate_workflow_graph(graph: WorkflowGraph) -> WorkflowValidationResult {
    match graph.validate() {
        Ok(summary) => WorkflowValidationResult {
            valid: true,
            summary: Some(summary),
            errors: Vec::new(),
        },
        Err(error) => WorkflowValidationResult {
            valid: false,
            summary: None,
            errors: vec![error.message],
        },
    }
}

#[tauri::command]
fn workflow_node_impact(
    node_id: String,
    state: State<'_, DesktopState>,
) -> Result<NodeImpact, String> {
    load_workspace(&state.repository)?
        .graph
        .impact_of_node(&node_id)
        .map_err(|error| error.message)
}

#[tauri::command]
async fn save_workflow_graph(
    graph: WorkflowGraph,
    state: State<'_, DesktopState>,
) -> Result<WorkflowGraph, String> {
    graph.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    let mut workspace = load_workspace(&state.repository)?;
    workspace.graph = graph;
    workspace.validate().map_err(|error| error.message)?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    Ok(workspace.graph)
}

fn read_ui_state(repository: &WorkspaceRepository) -> WorkspaceUiState {
    let path = repository.root().join(UI_STATE_FILE);
    let Ok(content) = fs::read_to_string(path) else {
        return WorkspaceUiState::default();
    };
    let Ok(mut state) = serde_json::from_str::<WorkspaceUiState>(&content) else {
        return WorkspaceUiState::default();
    };
    if sanitize_ui_state(&mut state).is_err() {
        return WorkspaceUiState::default();
    }
    state
}

fn sanitize_ui_state(state: &mut WorkspaceUiState) -> Result<(), String> {
    if state.schema_version == 1 {
        state.schema_version = UI_STATE_SCHEMA_VERSION;
        state.shell.left_width = 188;
        state.shell.right_width = 320;
        state.shell.right_inspector_open = false;
        state.shell.right_inspector_pinned = false;
    } else if state.schema_version != UI_STATE_SCHEMA_VERSION {
        return Err("不支持的桌面 UI 状态版本。".to_owned());
    }
    state.shell.left_width = state.shell.left_width.clamp(160, 280);
    state.shell.right_width = state.shell.right_width.clamp(296, 420);
    if !matches!(
        state.last_page.as_str(),
        "overview"
            | "assets"
            | "workflows"
            | "publishers"
            | "runs"
            | "notifications"
            | "templates"
            | "settings"
    ) {
        state.last_page = default_page();
    }
    for workflow in state.workflows.values_mut() {
        if !workflow.viewport.x.is_finite()
            || !workflow.viewport.y.is_finite()
            || !workflow.viewport.zoom.is_finite()
        {
            return Err("画布视口包含无效数值。".to_owned());
        }
        workflow.viewport.zoom = workflow.viewport.zoom.clamp(0.2, 2.5);
        if workflow
            .node_positions
            .values()
            .any(|position| !position.x.is_finite() || !position.y.is_finite())
        {
            return Err("节点坐标包含无效数值。".to_owned());
        }
        workflow.collapsed_groups.sort();
        workflow.collapsed_groups.dedup();
    }
    Ok(())
}

#[tauri::command]
fn audit_records(
    query: AuditQuery,
    state: State<'_, DesktopState>,
) -> Result<Vec<ExecutionTrace>, String> {
    read_jsonl_audit(audit_path(&state.repository), query.limit.unwrap_or(100)).map_err(safe_error)
}

#[tauri::command]
fn export_workspace(state: State<'_, DesktopState>) -> Result<String, String> {
    load_workspace(&state.repository)?
        .export_json()
        .map_err(|error| error.message)
}

#[tauri::command]
async fn import_workspace(
    input: WorkspaceImportInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let loaded = load_workspace_json(&input.json).map_err(|error| error.message)?;
    let WorkspaceLoad::Ready { workspace } = loaded else {
        return Err("不支持导入比当前版本更新的工作区。".to_owned());
    };
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    snapshot(&state).await
}

#[tauri::command]
async fn initialize_workspace(
    name: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let package = empty_workspace(&workspace_name(&name));
    state.repository.save(&package).map_err(safe_error)?;

    let control_plane =
        open_control_plane(&state.repository, &state.secret_store).map_err(safe_error)?;
    *state.control_plane.lock().await = Some(control_plane);
    *state.startup_error.lock().await = None;

    snapshot(&state).await
}

#[tauri::command]
async fn start_publisher(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    with_plane(&state, |plane| async move {
        plane.start_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
}

#[tauri::command]
async fn pause_publisher(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    with_plane(&state, |plane| async move {
        plane.pause_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
}

#[tauri::command]
async fn stop_publisher(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    with_plane(&state, |plane| async move {
        plane.stop_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
}

async fn with_plane<F, Fut>(state: &DesktopState, action: F) -> Result<(), String>
where
    F: FnOnce(ControlPlane) -> Fut,
    Fut: std::future::Future<Output = Result<(), apiarray_runtime::RuntimeError>>,
{
    let plane = state
        .control_plane
        .lock()
        .await
        .clone()
        .ok_or_else(|| "请先创建工作区。".to_owned())?;
    action(plane).await.map_err(safe_error)
}

fn load_workspace(repository: &WorkspaceRepository) -> Result<WorkspacePackage, String> {
    match repository.load().map_err(safe_error)?.loaded {
        WorkspaceLoad::Ready { workspace } => Ok(*workspace),
        WorkspaceLoad::ReadOnly { reason, .. } => {
            Err(format!("工作区只能以只读模式打开：{reason}"))
        }
    }
}

async fn reload_control_plane(state: &DesktopState) -> Result<(), String> {
    let control_plane =
        open_control_plane(&state.repository, &state.secret_store).map_err(safe_error)?;
    *state.control_plane.lock().await = Some(control_plane);
    *state.startup_error.lock().await = None;
    Ok(())
}

async fn ensure_no_running_publishers(state: &DesktopState) -> Result<(), String> {
    let plane = state.control_plane.lock().await.clone();
    if let Some(plane) = plane
        && plane.snapshot().await.supervisor.running_count > 0
    {
        return Err("请先暂停或停止正在运行的 Publisher，再修改工作区配置。".to_owned());
    }
    Ok(())
}

async fn snapshot(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let control_plane = state.control_plane.lock().await.clone();
    let startup_error = state.startup_error.lock().await.clone();
    let provider_count = load_workspace(&state.repository)
        .map(|workspace| workspace.runtime.providers.len())
        .unwrap_or(0);
    let control = match control_plane {
        Some(plane) => Some(to_desktop_control_snapshot(
            plane.snapshot().await,
            provider_count,
        )),
        None => None,
    };

    Ok(DesktopSnapshot {
        initialized: control.is_some(),
        startup_error,
        control,
    })
}

fn workspace_name(input: &str) -> String {
    let name = input.trim();
    if name.is_empty() {
        return "我的 API ARRAY 工作区".to_owned();
    }
    name.chars().take(80).collect()
}

fn empty_workspace(name: &str) -> WorkspacePackage {
    WorkspacePackage {
        schema_version: SCHEMA_VERSION,
        id: DEFAULT_WORKSPACE_ID.to_owned(),
        name: name.to_owned(),
        runtime: RuntimeConfig {
            schema_version: SCHEMA_VERSION,
            id: DEFAULT_WORKSPACE_ID.to_owned(),
            providers: BTreeMap::new(),
            publishers: BTreeMap::new(),
        },
        runtime_state: WorkspaceRuntimeState::default(),
        graph: WorkflowGraph {
            schema_version: SCHEMA_VERSION,
            id: "main".to_owned(),
            nodes: Vec::new(),
            edges: Vec::new(),
        },
        ui: Value::Null,
        templates: BTreeMap::new(),
    }
}

fn install_publisher_workflow(graph: &mut WorkflowGraph, publisher_id: &str) {
    let prefix = format!("{publisher_id}-");
    graph.nodes.retain(|node| !node.id.starts_with(&prefix));
    graph.edges.retain(|edge| !edge.id.starts_with(&prefix));
    let request_input = || Port {
        id: "request".to_owned(),
        data_type: PortType::Request,
    };
    let request_output = || Port {
        id: "request".to_owned(),
        data_type: PortType::Request,
    };
    let nodes = [
        ("adapter", "Provider Adapter", NodeKind::Adapter),
        ("probe", "Provider Probe", NodeKind::Probe),
        ("router", "Failover Router", NodeKind::Router),
        ("publisher", "Local Publisher", NodeKind::Publisher),
    ]
    .into_iter()
    .map(|(suffix, name, kind)| Node {
        id: format!("{prefix}{suffix}"),
        name: name.to_owned(),
        kind,
        enabled: true,
        inputs: vec![request_input()],
        outputs: vec![request_output()],
        config: serde_json::json!({"publisher_id": publisher_id}),
    });
    graph.nodes.extend(nodes);
    for (index, (from, to)) in [
        ("adapter", "probe"),
        ("probe", "router"),
        ("router", "publisher"),
    ]
    .into_iter()
    .enumerate()
    {
        graph.edges.push(Edge {
            id: format!("{prefix}edge-{index}"),
            from: Endpoint {
                node: format!("{prefix}{from}"),
                port: "request".to_owned(),
            },
            to: Endpoint {
                node: format!("{prefix}{to}"),
                port: "request".to_owned(),
            },
        });
    }
}

fn audit_path(repository: &WorkspaceRepository) -> PathBuf {
    repository.root().join("audit.jsonl")
}

fn open_control_plane(
    repository: &WorkspaceRepository,
    secret_store: &Arc<WindowsCredentialStore>,
) -> Result<ControlPlane, apiarray_runtime::RuntimeError> {
    let audit = JsonlAuditSink::open(audit_path(repository))
        .ok()
        .map(|sink| Arc::new(sink) as Arc<dyn apiarray_runtime::resilience::AuditSink>);
    ControlPlane::open(repository.clone(), secret_store.clone(), audit)
}

fn to_desktop_control_snapshot(
    snapshot: ControlPlaneSnapshot,
    provider_count: usize,
) -> DesktopControlSnapshot {
    DesktopControlSnapshot {
        workspace_id: snapshot.workspace_id,
        workspace_name: snapshot.workspace_name,
        recovered_from_backup: snapshot.recovered_from_backup,
        required_secret_count: snapshot.secrets.required.len(),
        missing_secret_count: snapshot.secrets.missing.len(),
        provider_count,
        publisher_count: snapshot.supervisor.publishers.len(),
        supervisor: DesktopSupervisorSnapshot {
            publishers: snapshot
                .supervisor
                .publishers
                .into_values()
                .map(|publisher| DesktopPublisherSnapshot {
                    id: publisher.publisher_id,
                    status: publisher.lifecycle,
                    message: publisher.last_error,
                })
                .collect(),
            running_count: snapshot.supervisor.running_count,
        },
        notifications: snapshot
            .notifications
            .into_iter()
            .map(|notification| DesktopNotification {
                id: notification.key,
                message: notification.event.summary,
                created_at: notification.last_seen_unix_ms,
            })
            .collect(),
    }
}

fn provider_catalog_item(
    manifest: apiarray_core::provider::ProviderManifest,
) -> ProviderCatalogItem {
    ProviderCatalogItem {
        id: manifest.provider.id,
        name: manifest.provider.name,
        category: manifest.provider.category,
        adapter: manifest.adapter.id,
        default_base_url: manifest.endpoint.default_base_url,
        editable_endpoint: manifest.endpoint.editable,
        auth_fields: manifest
            .authentication
            .fields
            .into_iter()
            .map(|field| ProviderAuthField {
                id: field.id,
                label: field.label,
                required: field.required,
                secret: field.secret,
            })
            .collect(),
        probe_count: manifest.probes.len(),
    }
}

fn normalize_id(input: &str, kind: &str) -> String {
    let normalized: String = input
        .trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .take(64)
        .collect();
    if normalized.is_empty() {
        format!("{kind}-default")
    } else {
        normalized
    }
}

fn safe_error(error: apiarray_runtime::RuntimeError) -> String {
    error.safe_message
}

fn workspace_repository(app: &AppHandle) -> Result<WorkspaceRepository, String> {
    let data_dir: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    Ok(WorkspaceRepository::new(
        data_dir.join("workspaces").join(DEFAULT_WORKSPACE_ID),
    ))
}

fn install_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 API ARRAY", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::with_id("main")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "windows")]
fn apply_native_material(window: &WebviewWindow, dark: Option<bool>) {
    let _ = window_vibrancy::clear_mica(window);
    let _ = window_vibrancy::clear_acrylic(window);
    if window_vibrancy::apply_mica(window, dark).is_err() {
        let color = if dark.unwrap_or(true) {
            (18, 20, 27, 205)
        } else {
            (242, 244, 248, 215)
        };
        let _ = window_vibrancy::apply_acrylic(window, Some(color));
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_native_material(_window: &WebviewWindow, _dark: Option<bool>) {}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .setup(|app| {
            let repository = workspace_repository(&app.handle())?;
            let secret_store = Arc::new(WindowsCredentialStore::new(CREDENTIAL_SERVICE));
            let has_workspace = repository.root().join("workspace.json").is_file();
            let (control_plane, startup_error) = if has_workspace {
                match open_control_plane(&repository, &secret_store) {
                    Ok(plane) => (Some(plane), None),
                    Err(error) => (None, Some(safe_error(error))),
                }
            } else {
                (None, None)
            };

            app.manage(DesktopState {
                inspection_reports: InspectionRepository::new(repository.root()),
                probe_runner: ProviderProbeRunner::new(TransportConfig::default()).map_err(
                    |error| {
                        tauri::Error::Setup((Box::new(error) as Box<dyn std::error::Error>).into())
                    },
                )?,
                paused_probes: Mutex::new(BTreeSet::new()),
                repository,
                secret_store,
                control_plane: Mutex::new(control_plane),
                startup_error: Mutex::new(startup_error),
            });
            install_tray(&app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                apply_native_material(&window, None);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_snapshot,
            provider_catalog,
            provider_instances,
            validate_provider_yaml,
            upsert_provider_instance,
            upsert_custom_provider_yaml,
            delete_provider_instance,
            store_secret,
            delete_secret,
            run_provider_probe,
            inspection_report,
            pause_provider_probe,
            create_publisher,
            delete_publisher,
            publisher_preview,
            publisher_templates,
            test_publisher_connection,
            set_window_material_theme,
            workflow_graph,
            workspace_ui_state,
            save_workspace_ui_state,
            validate_workflow_graph,
            workflow_node_impact,
            save_workflow_graph,
            audit_records,
            export_workspace,
            import_workspace,
            initialize_workspace,
            start_publisher,
            pause_publisher,
            stop_publisher
        ])
        .run(tauri::generate_context!())
        .expect("API ARRAY 桌面程序无法启动");
}

#[cfg(test)]
mod tests {
    use super::{
        ShellUiState, WorkspaceUiState, empty_workspace, install_publisher_workflow,
        model_count_from_payload, read_ui_state, sanitize_ui_state, workspace_name,
    };
    use apiarray_runtime::persistence::WorkspaceRepository;
    use std::fs;

    #[test]
    fn creates_an_empty_workspace_without_publishers_or_providers() {
        let workspace = empty_workspace("桌面工作区");

        assert_eq!(workspace.name, "桌面工作区");
        assert!(workspace.runtime.providers.is_empty());
        assert!(workspace.runtime.publishers.is_empty());
        assert!(workspace.graph.nodes.is_empty());
    }

    #[test]
    fn normalizes_a_missing_or_very_long_workspace_name() {
        assert_eq!(workspace_name("  "), "我的 API ARRAY 工作区");
        assert_eq!(workspace_name(&"a".repeat(100)).chars().count(), 80);
    }

    #[test]
    fn publisher_creation_installs_a_valid_minimal_workflow() {
        let mut workspace = empty_workspace("桌面工作区");
        install_publisher_workflow(&mut workspace.graph, "local-api");
        let summary = workspace
            .graph
            .validate()
            .expect("generated graph is valid");
        assert_eq!(summary.node_count, 4);
        assert_eq!(summary.edge_count, 3);
        assert_eq!(summary.publisher_count, 1);
    }

    #[test]
    fn ui_state_defaults_are_safe_and_shell_widths_are_clamped() {
        let mut state = WorkspaceUiState::default();
        state.shell = ShellUiState {
            left_sidebar_collapsed: true,
            right_inspector_open: true,
            right_inspector_pinned: false,
            left_width: 10,
            right_width: 900,
        };

        sanitize_ui_state(&mut state).expect("valid schema is accepted");

        assert_eq!(state.shell.left_width, 160);
        assert_eq!(state.shell.right_width, 420);
    }

    #[test]
    fn unsupported_ui_state_version_is_rejected() {
        let mut state = WorkspaceUiState::default();
        state.schema_version = 99;
        assert!(sanitize_ui_state(&mut state).is_err());
    }

    #[test]
    fn version_one_ui_state_is_migrated_to_current_defaults() {
        let mut state: WorkspaceUiState = serde_json::from_str(
            r#"{"schemaVersion":1,"shell":{"leftSidebarCollapsed":false,"rightInspectorOpen":true,"rightInspectorPinned":true,"leftWidth":230,"rightWidth":310},"workflows":{}}"#,
        )
        .expect("version one state remains readable");

        sanitize_ui_state(&mut state).expect("version one state migrates");

        assert_eq!(state.schema_version, 2);
        assert_eq!(state.theme_preference, super::ThemePreference::System);
        assert_eq!(state.view_mode, super::ViewMode::Simple);
        assert!(!state.shell.right_inspector_open);
        assert!(!state.shell.right_inspector_pinned);
    }

    #[test]
    fn corrupted_ui_state_falls_back_without_blocking_workspace() {
        let root = std::env::temp_dir().join(format!("apiarray-ui-state-{}", std::process::id()));
        fs::create_dir_all(&root).expect("temporary directory is writable");
        fs::write(root.join("ui-state.json"), b"{not-json").expect("corrupted fixture is written");
        let repository = WorkspaceRepository::new(&root);

        assert_eq!(read_ui_state(&repository), WorkspaceUiState::default());

        fs::remove_dir_all(root).expect("temporary directory is removed");
    }

    #[test]
    fn local_publisher_test_counts_only_openai_model_data() {
        assert_eq!(
            model_count_from_payload(&serde_json::json!({"data": [{"id": "a"}, {"id": "b"}]})),
            2
        );
        assert_eq!(
            model_count_from_payload(&serde_json::json!({"models": []})),
            0
        );
    }
}
