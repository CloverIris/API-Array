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
    if state.schema_version == 1 || state.schema_version == 2 {
        state.schema_version = UI_STATE_SCHEMA_VERSION;
        state.shell.left_width = 248;
        state.shell.right_width = 320;
        state.shell.right_inspector_open = false;
        state.shell.right_inspector_pinned = false;
    } else if state.schema_version != UI_STATE_SCHEMA_VERSION {
        return Err("不支持的桌面 UI 状态版本。".to_owned());
    }
    state.shell.left_width = state.shell.left_width.clamp(220, 320);
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
async fn run_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .and_then(|project| project.canvases.get_mut(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    canvas.graph.validate().map_err(|error| error.message)?;
    let publisher_id = canvas.publisher_id.clone().ok_or_else(|| {
        "Configure a local Publisher for this canvas before running it".to_owned()
    })?;
    canvas.applied_graph = Some(canvas.graph.clone());
    canvas.applied_revision = canvas.draft_revision;
    workspace.validate().map_err(|error| error.message)?;
    state.repository.save(&workspace).map_err(safe_error)?;
    with_plane(&state, |plane| async move {
        plane.start_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
}

#[tauri::command]
async fn stop_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher_id = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?
        .publisher_id
        .clone()
        .ok_or_else(|| "This canvas has not been published".to_owned())?;
    with_plane(&state, |plane| async move {
        plane.stop_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
}

#[tauri::command]
async fn pause_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
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
    with_plane(&state, |plane| async move {
        plane.pause_publisher(&publisher_id).await.map(|_| ())
    })
    .await?;
    snapshot(&state).await
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
                .get("provider_instance_id")
                .and_then(Value::as_str)
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
