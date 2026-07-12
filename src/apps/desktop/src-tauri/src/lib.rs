use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use apiarray_core::{
    SCHEMA_VERSION,
    catalog::builtin_provider_manifests,
    graph::{
        compile_canvas_graph as compile_graph, CanvasCompilationReport, Edge, Endpoint, GraphSummary, Node, NodeImpact, NodeKind, Port, PortType, WorkflowGraph, GRAPH_SCHEMA_VERSION,
    },
    publisher::PublisherSummary,
    routing::{RoutePolicy, StandardError},
    runtime::{ModelRoute, ProviderInstance, RuntimeConfig, RuntimePublisher, UpstreamRoute},
    secret::SecretRef,
    templates::{CodeTemplate, LiveDocument, TemplateContext, TemplateLanguage, generate_live_document, generate_templates},
    workspace::{
        ApiAsset, ApiWallet, BillingPolicy, Canvas, DirectEndpoint, DirectModelMapping, Project, ProjectFolder,
        WORKSPACE_SCHEMA_VERSION, WorkspaceLoad, WorkspacePackage, WorkspaceProjects,
        WorkspaceRuntimeState, load_workspace_json,
    },
};
use apiarray_runtime::{
    control::{ControlPlane, ControlPlaneSnapshot},
    inspection::{InspectionRepository, ProviderProbeRunner},
    persistence::{WorkspaceBackup, WorkspaceHealth, WorkspaceLocation, WorkspaceRepository},
    resilience::{ExecutionTrace, SqliteAuditSink},
    secret::{SecretStore, SecretValue, StoreSecretResolver, WindowsCredentialStore},
    supervisor::PublisherLifecycle,
    transport::TransportConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tokio::sync::Mutex;

include!("state.rs");

include!("contracts.rs");

include!("commands/wallet_projects.rs");

include!("commands/providers.rs");

include!("commands/publishers.rs");

include!("commands/instances.rs");

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
        apply_instance_stop, managed_instance_id, parse_managed_instance_id, new_canvas_graph,
        read_ui_state, sanitize_ui_state, workspace_name, DirectEndpoint, ManagedInstanceKind,
        NodeKind, SecretRef, UI_STATE_KEY,
    };
    use apiarray_runtime::persistence::WorkspaceRepository;

    #[test]
    fn creates_an_empty_workspace_without_publishers_or_providers() {
        let workspace = empty_workspace("桌面工作区");

        assert_eq!(workspace.name, "桌面工作区");
        assert!(workspace.runtime.providers.is_empty());
        assert!(workspace.runtime.publishers.is_empty());
        assert!(workspace.projects.projects.contains_key("default"));
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
        assert_eq!(summary.node_count, 2);
        assert_eq!(summary.edge_count, 1);
        assert!(graph.nodes.iter().any(|node| node.kind == NodeKind::Composer));
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

        assert_eq!(state.schema_version, 5);
        assert_eq!(state.theme_preference, super::ThemePreference::System);
        assert!(!state.shell.right_inspector_open);
        assert!(!state.shell.right_inspector_pinned);
    }

    #[test]
    fn corrupted_ui_state_falls_back_without_blocking_workspace() {
        let root = std::env::temp_dir().join(format!("apiarray-ui-state-{}", std::process::id()));
        let repository = WorkspaceRepository::new(&root);
        repository.write_setting(UI_STATE_KEY, "{not-json").expect("corrupted fixture is written");

        assert_eq!(read_ui_state(&repository), WorkspaceUiState::default());

        std::fs::remove_dir_all(root).expect("temporary directory is removed");
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

    #[test]
    fn managed_instance_ids_are_stable_and_explicitly_scoped() {
        let direct = managed_instance_id(ManagedInstanceKind::DirectEndpoint, "primary", None);
        let canvas = managed_instance_id(ManagedInstanceKind::Canvas, "project-a", Some("canvas-b"));
        assert_eq!(direct, "direct:primary");
        assert_eq!(canvas, "canvas:project-a:canvas-b");
        assert_eq!(parse_managed_instance_id(&direct).expect("direct id").0, ManagedInstanceKind::DirectEndpoint);
        let parsed = parse_managed_instance_id(&canvas).expect("canvas id");
        assert_eq!(parsed.1, "project-a");
        assert_eq!(parsed.2.as_deref(), Some("canvas-b"));
        assert!(parse_managed_instance_id("canvas:missing").is_err());
    }

    #[test]
    fn stopping_one_instance_does_not_change_another_instance_intent() {
        let mut workspace = empty_workspace("rack-test");
        workspace.direct_endpoints.insert("direct-a".to_owned(), DirectEndpoint {
            id: "direct-a".to_owned(), name: "Direct A".to_owned(), alias: "direct-a".to_owned(),
            asset_id: "asset-a".to_owned(), token_ref: SecretRef::parse("secret://direct/direct-a/token").expect("secret ref"),
            enabled: true, models: Vec::new(), timeout_ms: 30_000, max_retries: 2,
            audit_tags: std::collections::BTreeMap::new(), billing_override: None,
        });
        workspace.projects.projects.get_mut("default").expect("project").canvases.get_mut("main").expect("canvas").publisher_id = Some("canvas-publisher".to_owned());
        workspace.runtime_state.enabled_publishers.insert("canvas-publisher".to_owned());
        assert!(apply_instance_stop(&mut workspace, "direct:direct-a").expect("stop direct"));
        assert!(!workspace.direct_endpoints["direct-a"].enabled);
        assert!(workspace.runtime_state.enabled_publishers.contains("canvas-publisher"));
        assert!(apply_instance_stop(&mut workspace, "canvas:default:main").expect("stop canvas"));
        assert!(!workspace.runtime_state.enabled_publishers.contains("canvas-publisher"));
    }
}
