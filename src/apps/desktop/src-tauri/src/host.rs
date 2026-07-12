fn workspace_repository(app: &AppHandle) -> Result<WorkspaceRepository, String> {
    let data_dir: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    Ok(WorkspaceRepository::new(
        data_dir.join("workspaces").join(DEFAULT_WORKSPACE_ID),
    ))
}

#[allow(dead_code)]
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

fn install_application_tray(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "gateway_status", "网关状态：随应用运行", false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示 API ARRAY", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏 API ARRAY", true, None::<&str>)?;
    let wallet = MenuItem::with_id(app, "navigate_wallet", "API 钱包", true, None::<&str>)?;
    let direct = MenuItem::with_id(app, "navigate_direct", "审计直出", true, None::<&str>)?;
    let compositions = MenuItem::with_id(app, "navigate_compositions", "编组模式", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "开机启动", true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 API ARRAY", true, None::<&str>)?;
    let separator_a = PredefinedMenuItem::separator(app)?;
    let separator_b = PredefinedMenuItem::separator(app)?;
    let separator_c = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&status, &separator_a, &show, &hide, &separator_b, &wallet, &direct, &compositions, &separator_c, &autostart, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("API ARRAY · 本地 API 钱包与审计网关")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "hide" => { if let Some(window) = app.get_webview_window("main") { let _ = window.hide(); } }
            "navigate_wallet" => navigate_from_tray(app, "overview"),
            "navigate_direct" => navigate_from_tray(app, "direct"),
            "navigate_compositions" => navigate_from_tray(app, "workflows"),
            "autostart" => { let manager = app.autolaunch(); if manager.is_enabled().unwrap_or(false) { let _ = manager.disable(); } else { let _ = manager.enable(); } }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() { builder = builder.icon(icon.clone()); }
    builder.build(app)?;
    Ok(())
}

fn navigate_from_tray(app: &AppHandle, page: &str) {
    show_main_window(app);
    let _ = app.emit("desktop:navigate", page);
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
            reset_legacy_poc_workspace(&repository, &secret_store);
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
                gateway: Mutex::new(None),
                gateway_error: Mutex::new(None),
                startup_error: Mutex::new(startup_error),
            });
            install_application_tray(&app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                apply_native_material(&window, None);
            }
            if has_workspace {
                let state = app.state::<DesktopState>();
                let _ = tauri::async_runtime::block_on(restart_gateway(&state));
            }
            eprintln!("API ARRAY desktop host ready; embedded Rust Runtime is in-process; gateway={}", if has_workspace { "configured" } else { "waiting for workspace" });
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
            wallet_gallery,
            wallet_assets,
            create_wallet_asset,
            update_wallet_asset,
            delete_wallet_asset,
            wallet_asset_impact,
            probe_wallet_asset,
            direct_endpoints,
            create_direct_endpoint,
            update_direct_endpoint,
            delete_direct_endpoint,
            start_direct_endpoint,
            pause_direct_endpoint,
            test_direct_endpoint,
            direct_endpoint_templates,
            project_tree,
            create_project,
            rename_project,
            delete_project,
            create_folder,
            rename_folder,
            delete_folder,
            create_canvas,
            rename_canvas,
            move_canvas,
            duplicate_canvas,
            canvas_graph,
            canvas_snapshot,
            save_canvas_graph,
            compile_canvas_graph,
            canvas_node_impact,
            commit_wallet_placement,
            run_canvas,
            pause_canvas,
            stop_canvas,
            refresh_canvas,
            delete_canvas,
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
            create_canvas_publisher,
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

fn reset_legacy_poc_workspace(repository: &WorkspaceRepository, store: &WindowsCredentialStore) {
    let path = repository.root().join("workspace.json");
    let Ok(text) = fs::read_to_string(&path) else { return; };
    let Ok(raw) = serde_json::from_str::<Value>(&text) else { return; };
    if !raw.get("schema_version").and_then(Value::as_u64).is_some_and(|version| version < u64::from(WORKSPACE_SCHEMA_VERSION)) { return; }
    let mut references = Vec::new();
    collect_legacy_secret_refs(&raw, &mut references);
    for reference in references {
        if let Ok(reference) = SecretRef::parse(reference) { let _ = store.delete(&reference); }
    }
    let _ = fs::remove_dir_all(repository.root());
}

fn collect_legacy_secret_refs(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) if text.starts_with("secret://") => output.push(text.clone()),
        Value::Array(values) => values.iter().for_each(|value| collect_legacy_secret_refs(value, output)),
        Value::Object(values) => values.values().for_each(|value| collect_legacy_secret_refs(value, output)),
        _ => {}
    }
}
