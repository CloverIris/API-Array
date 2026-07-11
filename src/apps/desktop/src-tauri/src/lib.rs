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
    workspace::{
        ApiAsset, ApiWallet, BillingPolicy, Canvas, Project, ProjectFolder,
        WORKSPACE_SCHEMA_VERSION, WorkspaceLoad, WorkspacePackage, WorkspaceProjects,
        WorkspaceRuntimeState, load_workspace_json,
    },
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

include!("state.rs");

include!("contracts.rs");

include!("commands/wallet_projects.rs");

include!("commands/providers.rs");

include!("commands/publishers.rs");

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

include!("commands/workspace.rs");

include!("support.rs");

include!("host.rs");

#[cfg(test)]
mod tests {
    use super::{
        ShellUiState, WorkspaceUiState, empty_workspace, model_count_from_payload,
        new_canvas_graph, read_ui_state, sanitize_ui_state, workspace_name,
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
    fn new_canvas_starts_with_one_valid_total_output() {
        let graph = new_canvas_graph("canvas-a");
        let summary = graph.validate().expect("new canvas graph is valid");
        assert_eq!(summary.publisher_count, 1);
        assert_eq!(summary.node_count, 1);
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

        assert_eq!(state.shell.left_width, 220);
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

        assert_eq!(state.schema_version, 3);
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
