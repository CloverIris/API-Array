fn workspace_repository(app: &AppHandle) -> Result<(WorkspaceRepository, apiarray_runtime::persistence::LauncherRepository), String> {
    let data_dir: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    let launcher = apiarray_runtime::persistence::LauncherRepository::new(data_dir.join("launcher.sqlite3"));
    let default_root = data_dir.join("workspaces").join(DEFAULT_WORKSPACE_ID);
    let root = launcher.active_root().map_err(safe_error)?.filter(|root| root.join("workspace.sqlite3").is_file()).unwrap_or(default_root);
    Ok((WorkspaceRepository::new(root), launcher))
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn normalize_startup_window(window: &WebviewWindow) {
    let _ = window.set_fullscreen(false);
    let _ = window.unmaximize();
    let _ = window.set_size(Size::Logical(LogicalSize::new(1440.0, 920.0)));
    let _ = window.center();
}

fn quit_application(app: &AppHandle) {
    let state = app.state::<DesktopState>();
    if state.is_quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(gateway) = app.state::<DesktopState>().gateway.lock().await.take() {
            gateway.stop().await;
        }
        if let Some(plane) = app.state::<DesktopState>().control_plane.lock().await.clone() {
            plane.shutdown().await;
        }
        *app.state::<DesktopState>().control_plane.lock().await = None;
        app.exit(0);
    });
}

fn install_ctrl_c_handler(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            quit_application(&app);
        }
    });
}

fn webview_assets_changed(previous: Option<&str>, current: &str) -> bool {
    previous.map(str::trim) != Some(current)
}

#[cfg(not(debug_assertions))]
fn refresh_webview_assets_if_needed(app: &AppHandle, window: &WebviewWindow) {
    let current = env!("APIARRAY_UI_BUILD_ID");
    let Ok(cache_dir) = app.path().app_cache_dir() else { return };
    let marker = cache_dir.join("webview-ui-build-id");
    let previous = std::fs::read_to_string(&marker).ok();
    if !webview_assets_changed(previous.as_deref(), current) { return; }

    if let Err(error) = window.clear_all_browsing_data() {
        eprintln!("API ARRAY webview cache refresh failed: {error}");
        return;
    }
    if std::fs::create_dir_all(&cache_dir).and_then(|_| std::fs::write(&marker, current)).is_err() {
        eprintln!("API ARRAY webview build marker could not be saved");
        return;
    }
    let _ = window.eval("window.setTimeout(() => window.location.reload(), 0)");
}

#[cfg(debug_assertions)]
fn refresh_webview_assets_if_needed(_app: &AppHandle, _window: &WebviewWindow) {
    debug_assert!(!webview_assets_changed(Some("development"), "development"));
}

fn install_application_tray(app: &AppHandle) -> tauri::Result<()> {
    let home = MenuItem::with_id(app, "navigate_home", "打开主控台", true, None::<&str>)?;
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
    let menu = Menu::with_items(app, &[&status, &separator_a, &show, &hide, &separator_b, &home, &wallet, &direct, &compositions, &separator_c, &autostart, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("API ARRAY · 本地 API 钱包与审计网关")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "hide" => { if let Some(window) = app.get_webview_window("main") { let _ = window.hide(); } }
            "navigate_home" => navigate_from_tray(app, "home"),
            "navigate_wallet" => navigate_from_tray(app, "wallet"),
            "navigate_direct" => navigate_from_tray(app, "direct"),
            "navigate_compositions" => navigate_from_tray(app, "compositions"),
            "autostart" => { let manager = app.autolaunch(); if manager.is_enabled().unwrap_or(false) { let _ = manager.disable(); } else { let _ = manager.enable(); } }
            "quit" => quit_application(app),
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .setup(|app| {
            let (repository, launcher) = workspace_repository(&app.handle())?;
            let secret_store = Arc::new(WindowsCredentialStore::new(CREDENTIAL_SERVICE));
            let has_workspace = repository.exists();
            if has_workspace && let Ok(descriptor) = repository.descriptor() { let _ = launcher.register_and_activate(&descriptor); }
            let (control_plane, startup_error) = if has_workspace {
                match open_control_plane(&repository, &secret_store) {
                    Ok(plane) => (Some(plane), None),
                    Err(error) => (None, Some(safe_error(error))),
                }
            } else {
                (None, None)
            };

            app.manage(DesktopState {
                app: app.handle().clone(),
                inspection_reports: ActiveInspectionRepository::new(repository.clone()),
                probe_runner: ProviderProbeRunner::new(TransportConfig::default()).map_err(
                    |error| {
                        tauri::Error::Setup((Box::new(error) as Box<dyn std::error::Error>).into())
                    },
                )?,
                paused_probes: Mutex::new(BTreeSet::new()),
                repository: ActiveWorkspace::new(repository, launcher),
                secret_store,
                control_plane: Mutex::new(control_plane),
                gateway: Mutex::new(None),
                gateway_error: Mutex::new(None),
                startup_error: Mutex::new(startup_error),
                is_quitting: AtomicBool::new(false),
            });
            install_application_tray(&app.handle())?;
            install_ctrl_c_handler(&app.handle());

            if let Some(window) = app.get_webview_window("main") {
                normalize_startup_window(&window);
                apply_native_material(&window, None);
                refresh_webview_assets_if_needed(&app.handle(), &window);
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
                if window.app_handle().state::<DesktopState>().is_quitting.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_snapshot,
            application_version,
            provider_catalog,
            wallet_gallery,
            wallet_assets,
            create_wallet_asset,
            update_wallet_asset,
            delete_wallet_asset,
            delete_wallet_asset_secret,
            reveal_wallet_secret,
            wallet_asset_impact,
            probe_wallet_asset,
            direct_endpoints,
            create_direct_endpoint,
            update_direct_endpoint,
            delete_direct_endpoint_safe,
            start_direct_endpoint,
            pause_direct_endpoint,
            test_direct_endpoint,
            direct_endpoint_templates,
            direct_endpoint_live_document,
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
            canvas_live_document,
            save_live_document_markdown,
            save_markdown_document,
            test_publisher_connection,
            set_window_material_theme,
            workspace_ui_state,
            save_workspace_ui_state,
            validate_workflow_graph,
            audit_records,
            notifications,
            mark_notifications_read,
            clear_read_notifications,
            desktop_notification_preference,
            save_desktop_notification_preference,
            export_workspace,
            import_workspace,
            workspace_storage_status,
            update_gateway_settings,
            workspace_locations,
            create_workspace_at,
            create_default_workspace,
            open_workspace_at,
            relocate_workspace,
            verify_workspace,
            backup_workspace,
            compact_workspace,
            initialize_workspace,
            start_publisher,
            pause_publisher,
            stop_publisher,
            control_center_snapshot,
            start_managed_instance,
            stop_managed_instance,
            test_managed_instance,
            start_all_instances,
            stop_all_instances,
            start_instances_by_kind,
            stop_instances_by_kind
        ])
        .run(tauri::generate_context!())
        .expect("API ARRAY 妗岄潰绋嬪簭鏃犳硶鍚姩");
}
