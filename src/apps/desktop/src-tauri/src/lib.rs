use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use apiarray_core::{
    graph::WorkflowGraph,
    runtime::RuntimeConfig,
    workspace::{WorkspacePackage, WorkspaceRuntimeState},
    SCHEMA_VERSION,
};
use apiarray_runtime::{
    control::{ControlPlane, ControlPlaneSnapshot},
    persistence::WorkspaceRepository,
    secret::WindowsCredentialStore,
    supervisor::PublisherLifecycle,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State, WebviewWindow, WindowEvent,
};
use tokio::sync::Mutex;

const CREDENTIAL_SERVICE: &str = "API ARRAY";
const DEFAULT_WORKSPACE_ID: &str = "default";

struct DesktopState {
    repository: WorkspaceRepository,
    control_plane: Mutex<Option<ControlPlane>>,
    startup_error: Mutex<Option<String>>,
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

#[tauri::command]
async fn desktop_snapshot(state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    snapshot(&state).await
}

#[tauri::command]
async fn initialize_workspace(
    name: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let package = empty_workspace(&workspace_name(&name));
    state
        .repository
        .save(&package)
        .map_err(safe_error)?;

    let control_plane = open_control_plane(&state.repository).map_err(safe_error)?;
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

async fn snapshot(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let control_plane = state.control_plane.lock().await.clone();
    let startup_error = state.startup_error.lock().await.clone();
    let control = match control_plane {
        Some(plane) => Some(to_desktop_control_snapshot(plane.snapshot().await)),
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

fn open_control_plane(repository: &WorkspaceRepository) -> Result<ControlPlane, apiarray_runtime::RuntimeError> {
    ControlPlane::open(
        repository.clone(),
        Arc::new(WindowsCredentialStore::new(CREDENTIAL_SERVICE)),
        None,
    )
}

fn to_desktop_control_snapshot(snapshot: ControlPlaneSnapshot) -> DesktopControlSnapshot {
    DesktopControlSnapshot {
        workspace_id: snapshot.workspace_id,
        workspace_name: snapshot.workspace_name,
        recovered_from_backup: snapshot.recovered_from_backup,
        required_secret_count: snapshot.secrets.required.len(),
        missing_secret_count: snapshot.secrets.missing.len(),
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
fn apply_native_material(window: &WebviewWindow) {
    if window_vibrancy::apply_mica(window, Some(true)).is_err() {
        let _ = window_vibrancy::apply_acrylic(window, Some((18, 20, 27, 205)));
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_native_material(_window: &WebviewWindow) {}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .setup(|app| {
            let repository = workspace_repository(&app.handle())?;
            let has_workspace = repository.root().join("workspace.json").is_file();
            let (control_plane, startup_error) = if has_workspace {
                match open_control_plane(&repository) {
                    Ok(plane) => (Some(plane), None),
                    Err(error) => (None, Some(safe_error(error))),
                }
            } else {
                (None, None)
            };

            app.manage(DesktopState {
                repository,
                control_plane: Mutex::new(control_plane),
                startup_error: Mutex::new(startup_error),
            });
            install_tray(&app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                apply_native_material(&window);
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
    use super::{empty_workspace, workspace_name};

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
}
