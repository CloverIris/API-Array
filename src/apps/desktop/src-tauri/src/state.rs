const CREDENTIAL_SERVICE: &str = "API ARRAY";
const DEFAULT_WORKSPACE_ID: &str = "default";
const UI_STATE_FILE: &str = "ui-state.json";
const UI_STATE_SCHEMA_VERSION: u32 = 4;

struct DesktopState {
    repository: WorkspaceRepository,
    secret_store: Arc<WindowsCredentialStore>,
    probe_runner: ProviderProbeRunner,
    inspection_reports: InspectionRepository,
    paused_probes: Mutex<BTreeSet<String>>,
    control_plane: Mutex<Option<ControlPlane>>,
    gateway: Mutex<Option<apiarray_runtime::gateway::LocalGateway>>,
    gateway_error: Mutex<Option<String>>,
    startup_error: Mutex<Option<String>>,
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
