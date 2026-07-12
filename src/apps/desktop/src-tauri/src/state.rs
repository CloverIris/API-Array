const CREDENTIAL_SERVICE: &str = "API ARRAY";
const DEFAULT_WORKSPACE_ID: &str = "default";
const UI_STATE_KEY: &str = "desktop.ui_state";
const UI_STATE_SCHEMA_VERSION: u32 = 6;

struct DesktopState {
    app: AppHandle,
    repository: ActiveWorkspace,
    secret_store: Arc<WindowsCredentialStore>,
    probe_runner: ProviderProbeRunner,
    inspection_reports: ActiveInspectionRepository,
    paused_probes: Mutex<BTreeSet<String>>,
    control_plane: Mutex<Option<ControlPlane>>,
    gateway: Mutex<Option<apiarray_runtime::gateway::LocalGateway>>,
    gateway_error: Mutex<Option<String>>,
    startup_error: Mutex<Option<String>>,
}

struct ActiveInspectionRepository { current: std::sync::RwLock<InspectionRepository> }
impl ActiveInspectionRepository {
    fn new(repository: WorkspaceRepository) -> Self { Self { current: std::sync::RwLock::new(InspectionRepository::from_repository(repository)) } }
    fn switch_to(&self, repository: WorkspaceRepository) -> Result<(), String> { *self.current.write().map_err(|_| "无法切换体检报告存储。".to_owned())? = InspectionRepository::from_repository(repository); Ok(()) }
    fn save(&self, report: &apiarray_core::inspection::InspectionReport) -> Result<(), apiarray_runtime::RuntimeError> { self.current.read().map_err(|_| apiarray_runtime::RuntimeError::new(apiarray_runtime::RuntimeErrorCode::WorkspaceStorageUnavailable, "体检报告存储不可用"))?.save(report) }
    fn load(&self, provider_id: &str) -> Result<Option<apiarray_core::inspection::InspectionReport>, apiarray_runtime::RuntimeError> { self.current.read().map_err(|_| apiarray_runtime::RuntimeError::new(apiarray_runtime::RuntimeErrorCode::WorkspaceStorageUnavailable, "体检报告存储不可用"))?.load(provider_id) }
}

struct ActiveWorkspace {
    current: std::sync::RwLock<WorkspaceRepository>,
    launcher: apiarray_runtime::persistence::LauncherRepository,
}

impl ActiveWorkspace {
    fn new(repository: WorkspaceRepository, launcher: apiarray_runtime::persistence::LauncherRepository) -> Self { Self { current: std::sync::RwLock::new(repository), launcher } }
    fn current(&self) -> WorkspaceRepository { self.current.read().expect("active workspace lock poisoned").clone() }
    fn switch_to(&self, repository: WorkspaceRepository) -> Result<(), String> { let descriptor = repository.descriptor().map_err(safe_error)?; self.launcher.register_and_activate(&descriptor).map_err(safe_error)?; *self.current.write().map_err(|_| "无法切换活动工作区。".to_owned())? = repository; Ok(()) }
    fn locations(&self) -> Result<Vec<apiarray_runtime::persistence::WorkspaceLocation>, String> { self.launcher.locations().map_err(safe_error) }
    fn root(&self) -> PathBuf { self.current().root().to_path_buf() }
    fn save(&self, workspace: &WorkspacePackage) -> Result<(), apiarray_runtime::RuntimeError> { self.current().save(workspace) }
    fn health(&self) -> Result<WorkspaceHealth, apiarray_runtime::RuntimeError> { self.current().health() }
    fn backup(&self) -> Result<WorkspaceBackup, apiarray_runtime::RuntimeError> { self.current().backup() }
    fn compact(&self) -> Result<(), apiarray_runtime::RuntimeError> { self.current().compact() }
    fn write_setting(&self, key: &str, value: &str) -> Result<(), apiarray_runtime::RuntimeError> { self.current().write_setting(key, value) }
    fn read_setting(&self, key: &str) -> Result<Option<String>, apiarray_runtime::RuntimeError> { self.current().read_setting(key) }
    fn read_audit(&self, limit: usize) -> Result<Vec<ExecutionTrace>, apiarray_runtime::RuntimeError> { self.current().read_audit(limit) }
    fn query_audit(&self, query: apiarray_runtime::persistence::AuditQuery) -> Result<Vec<ExecutionTrace>, apiarray_runtime::RuntimeError> { self.current().query_audit(query) }
    fn read_notifications(&self, query: apiarray_runtime::persistence::NotificationQuery) -> Result<Vec<apiarray_runtime::persistence::StoredNotification>, apiarray_runtime::RuntimeError> { self.current().read_notifications(query) }
    fn mark_notifications_read(&self, ids: &[String]) -> Result<(), apiarray_runtime::RuntimeError> { self.current().mark_notifications_read(ids) }
    fn clear_read_notifications(&self) -> Result<usize, apiarray_runtime::RuntimeError> { self.current().clear_read_notifications() }
    fn upsert_notification(&self, notification: &apiarray_core::events::AggregatedNotification) -> Result<bool, apiarray_runtime::RuntimeError> { self.current().upsert_notification(notification) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct WorkspaceUiState {
    schema_version: u32,
    #[serde(default)]
    theme_preference: ThemePreference,
    #[serde(default = "default_page")]
    last_page: String,
    #[serde(default)]
    workspace_intent: WorkspaceIntent,
    shell: ShellUiState,
    #[serde(default)]
    workflows: BTreeMap<String, WorkflowUiState>,
    #[serde(default)]
    selected_project_id: Option<String>,
    #[serde(default)]
    selected_canvas_id: Option<String>,
    #[serde(default)]
    expanded_project_ids: Vec<String>,
    #[serde(default)]
    expanded_folder_ids: Vec<String>,
    #[serde(default)]
    canvas_tabs: BTreeMap<String, String>,
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
enum WorkspaceIntent {
    #[default]
    ManageApis,
    UnifiedEndpoint,
    Reliability,
    Import,
}

fn default_page() -> String {
    "home".to_owned()
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
            last_page: default_page(),
            workspace_intent: WorkspaceIntent::ManageApis,
            shell: ShellUiState {
                left_sidebar_collapsed: false,
                right_inspector_open: false,
                right_inspector_pinned: false,
                left_width: 248,
                right_width: 320,
            },
            workflows: BTreeMap::new(),
            selected_project_id: None,
            selected_canvas_id: None,
            expanded_project_ids: Vec::new(),
            expanded_folder_ids: Vec::new(),
            canvas_tabs: BTreeMap::new(),
        }
    }
}
