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
            wallet_gallery,
            create_wallet_asset,
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
