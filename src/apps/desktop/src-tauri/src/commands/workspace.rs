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
    let value = serde_json::to_string(&ui_state)
        .map_err(|error| format!("无法序列化 UI 状态：{error}"))?;
    state.repository.write_setting(UI_STATE_KEY, &value).map_err(safe_error)?;
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

fn read_ui_state(repository: &impl RepositoryAccess) -> WorkspaceUiState {
    let Ok(Some(content)) = repository.active_repository().read_setting(UI_STATE_KEY) else {
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
    if matches!(state.schema_version, 1 | 2 | 3 | 4) {
        state.schema_version = UI_STATE_SCHEMA_VERSION;
        state.shell.left_width = 248;
        state.shell.right_width = 320;
        state.shell.right_inspector_open = false;
        state.shell.right_inspector_pinned = false;
        state.last_page = match state.last_page.as_str() {
            "overview" => "wallet".to_owned(),
            "workflows" => "compositions".to_owned(),
            _ => state.last_page.clone(),
        };
    } else if state.schema_version != UI_STATE_SCHEMA_VERSION {
        return Err("不支持的桌面 UI 状态版本。".to_owned());
    }
    state.shell.left_width = state.shell.left_width.clamp(220, 320);
    state.shell.right_width = state.shell.right_width.clamp(296, 420);
    if !matches!(
        state.last_page.as_str(),
        "instances"
            | "wallet"
            | "direct"
            | "compositions"
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
    state.expanded_project_ids.sort();
    state.expanded_project_ids.dedup();
    state.expanded_folder_ids.sort();
    state.expanded_folder_ids.dedup();
    state.canvas_tabs.retain(|_, tab| {
        matches!(
            tab.as_str(),
            "overview" | "workflow" | "routes" | "publisher" | "docs" | "runs"
        )
    });
    Ok(())
}

#[tauri::command]
fn audit_records(
    query: AuditQuery,
    state: State<'_, DesktopState>,
) -> Result<Vec<ExecutionTrace>, String> {
    state.repository.read_audit(query.limit.unwrap_or(100)).map_err(safe_error)
}

#[tauri::command]
fn export_workspace(state: State<'_, DesktopState>) -> Result<String, String> {
    load_workspace(&state.repository)?
        .export_json()
        .map_err(|error| error.message)
}

#[tauri::command]
fn workspace_storage_status(state: State<'_, DesktopState>) -> Result<WorkspaceHealth, String> {
    state.repository.health().map_err(safe_error)
}

#[tauri::command]
fn workspace_locations(state: State<'_, DesktopState>) -> Result<Vec<WorkspaceLocation>, String> { state.repository.locations() }

async fn activate_repository(repository: WorkspaceRepository, state: &DesktopState) -> Result<DesktopSnapshot, String> {
    ensure_no_running_publishers(state).await?;
    repository.load().map_err(safe_error)?;
    if !repository.health().map_err(safe_error)?.healthy { return Err("目标工作区数据库完整性检查失败。".to_owned()); }
    let previous = state.repository.current();
    if let Some(gateway) = state.gateway.lock().await.take() { gateway.stop().await; }
    state.repository.switch_to(repository.clone())?;
    if let Err(error) = state.inspection_reports.switch_to(repository) { let _ = state.repository.switch_to(previous); return Err(error); }
    if let Err(error) = reload_control_plane(state).await { let _ = state.repository.switch_to(previous.clone()); let _ = state.inspection_reports.switch_to(previous); let _ = reload_control_plane(state).await; let _ = restart_gateway(state).await; return Err(error); }
    if let Err(error) = restart_gateway(state).await { let _ = state.repository.switch_to(previous.clone()); let _ = state.inspection_reports.switch_to(previous); let _ = reload_control_plane(state).await; let _ = restart_gateway(state).await; return Err(error); }
    snapshot(state).await
}

#[tauri::command]
async fn create_workspace_at(input: CreateWorkspaceAtInput, state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    let root = PathBuf::from(input.root.trim());
    if root.as_os_str().is_empty() { return Err("请选择工作区目录。".to_owned()); }
    let repository = WorkspaceRepository::new(root);
    if repository.exists() { return Err("所选目录已经包含 API ARRAY 工作区。".to_owned()); }
    repository.save(&empty_workspace(&workspace_name(&input.name))).map_err(safe_error)?;
    activate_repository(repository, &state).await
}

#[tauri::command]
async fn open_workspace_at(input: WorkspacePathInput, state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    activate_repository(WorkspaceRepository::new(PathBuf::from(input.root.trim())), &state).await
}

#[tauri::command]
async fn relocate_workspace(input: WorkspacePathInput, state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    let destination = PathBuf::from(input.root.trim());
    if destination.as_os_str().is_empty() { return Err("请选择新的工作区目录。".to_owned()); }
    if destination.join("workspace.sqlite3").exists() { return Err("目标目录已经存在工作区数据库。".to_owned()); }
    ensure_no_running_publishers(&state).await?;
    let source = state.repository.current();
    let backup = source.backup().map_err(safe_error)?;
    std::fs::create_dir_all(&destination).map_err(|_| "无法创建目标工作区目录。".to_owned())?;
    std::fs::copy(&backup.path, destination.join("workspace.sqlite3")).map_err(|_| "无法复制工作区数据库。".to_owned())?;
    for directory in ["attachments", "exports"] { copy_directory_if_present(&source.root().join(directory), &destination.join(directory))?; }
    activate_repository(WorkspaceRepository::new(destination), &state).await
}

fn copy_directory_if_present(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if !source.is_dir() { return Ok(()); }
    std::fs::create_dir_all(destination).map_err(|_| "无法创建工作区附属目录。".to_owned())?;
    for entry in std::fs::read_dir(source).map_err(|_| "无法读取工作区附属目录。".to_owned())? { let entry = entry.map_err(|_| "无法读取工作区文件。".to_owned())?; let target = destination.join(entry.file_name()); if entry.path().is_dir() { copy_directory_if_present(&entry.path(), &target)?; } else { std::fs::copy(entry.path(), target).map_err(|_| "无法复制工作区附属文件。".to_owned())?; } }
    Ok(())
}

#[tauri::command]
fn verify_workspace(state: State<'_, DesktopState>) -> Result<WorkspaceHealth, String> {
    state.repository.health().map_err(safe_error)
}

#[tauri::command]
fn backup_workspace(state: State<'_, DesktopState>) -> Result<WorkspaceBackup, String> {
    state.repository.backup().map_err(safe_error)
}

#[tauri::command]
fn compact_workspace(state: State<'_, DesktopState>) -> Result<WorkspaceHealth, String> {
    state.repository.compact().map_err(safe_error)?;
    state.repository.health().map_err(safe_error)
}

#[tauri::command]
async fn import_workspace(
    input: WorkspaceImportInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let loaded = load_workspace_json(&input.json).map_err(|error| error.message)?;
    let WorkspaceLoad::Ready { workspace } = loaded;
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
    let _ = restart_gateway(&state).await;

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
async fn run_canvas(
    input: CanvasActionInput,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let graph = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?
        .graph
        .clone();
    validate_canvas_domain_graph(&workspace, &graph, true)?;
    let compiled = compile_graph(&graph, &workspace.wallet, &workspace.runtime).map_err(|error| error.message)?;
    let canvas = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .and_then(|project| project.canvases.get_mut(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let publisher_id = canvas.publisher_id.clone().ok_or_else(|| {
        "Configure a local Publisher for this canvas before running it".to_owned()
    })?;
    canvas.applied_graph = Some(canvas.graph.clone());
    canvas.applied_revision = canvas.draft_revision;
    if let Some(publisher) = workspace.runtime.publishers.get_mut(&publisher_id) { publisher.routes = compiled.routes; }
    workspace.validate().map_err(|error| error.message)?;
    workspace.runtime_state.enabled_publishers.insert(publisher_id);
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    restart_gateway(&state).await?;
    let result = snapshot(&state).await?;
    let _ = app.emit("desktop:instances-changed", ());
    Ok(result)
}

#[tauri::command]
async fn stop_canvas(
    input: CanvasActionInput,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let publisher_id = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?
        .publisher_id
        .clone()
        .ok_or_else(|| "This canvas has not been published".to_owned())?;
    workspace.runtime_state.enabled_publishers.remove(&publisher_id);
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    restart_gateway(&state).await?;
    let result = snapshot(&state).await?;
    let _ = app.emit("desktop:instances-changed", ());
    Ok(result)
}

#[tauri::command]
async fn pause_canvas(
    input: CanvasActionInput,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let publisher_id = canvas
        .publisher_id
        .clone()
        .ok_or_else(|| "This canvas has not been published".to_owned())?;
    workspace.runtime_state.enabled_publishers.remove(&publisher_id);
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    restart_gateway(&state).await?;
    let result = snapshot(&state).await?;
    let _ = app.emit("desktop:instances-changed", ());
    Ok(result)
}

#[tauri::command]
async fn refresh_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let provider_ids = canvas
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            node.config
                .get("asset_id")
                .and_then(Value::as_str)
                .and_then(|asset_id| workspace.wallet.assets.get(asset_id))
                .map(|asset| asset.provider_instance_id.as_str())
        })
        .collect::<BTreeSet<_>>();
    let resolver = StoreSecretResolver::new(state.secret_store.clone());
    for provider_id in provider_ids {
        let Some(instance) = workspace.runtime.providers.get(provider_id).cloned() else {
            continue;
        };
        if !instance.enabled {
            continue;
        }
        let report = state
            .probe_runner
            .run(&instance, &resolver, false)
            .await
            .map_err(safe_error)?;
        state.inspection_reports.save(&report).map_err(safe_error)?;
    }
    Ok(ProjectTreeSnapshot {
        projects: workspace.projects,
    })
}

#[tauri::command]
async fn delete_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    ensure_no_running_publishers(&state).await?;
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let canvas = project
        .canvases
        .remove(&input.canvas_id)
        .ok_or_else(|| "Canvas not found".to_owned())?;
    if let Some(folder_id) = canvas.folder_id.as_deref()
        && let Some(folder) = project.folders.get_mut(folder_id)
    {
        folder.canvas_ids.retain(|id| id != &canvas.id);
    }
    if let Some(publisher_id) = canvas.publisher_id {
        if let Some(publisher) = workspace.runtime.publishers.remove(&publisher_id)
            && let Some(token_ref) = publisher.config.token_ref
        {
            let _ = state.secret_store.delete(&token_ref);
        }
        workspace
            .runtime_state
            .enabled_publishers
            .remove(&publisher_id);
    }
    save_projects_workspace(&state, workspace).await
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
