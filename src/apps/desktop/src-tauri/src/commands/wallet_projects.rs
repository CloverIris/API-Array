#[tauri::command]
async fn desktop_snapshot(state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    snapshot(&state).await
}

#[tauri::command]
fn provider_catalog() -> Result<Vec<ProviderCatalogItem>, String> {
    builtin_provider_manifests()
        .map(|manifests| manifests.into_iter().map(provider_catalog_item).collect())
        .map_err(|error| error.message)
}

#[tauri::command]
fn wallet_gallery(state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let workspace = load_workspace(&state.repository)?;
    let audit = read_jsonl_audit(audit_path(&state.repository), 2_000).unwrap_or_default();
    let mut cards = builtin_provider_manifests()
        .map_err(|error| error.message)?
        .into_iter()
        .map(|manifest| WalletCard {
            id: format!("catalog:{}", manifest.provider.id),
            provider_id: manifest.provider.id,
            provider_instance_id: None,
            name: manifest.provider.name,
            configured: false,
            enabled: false,
            source: "catalog".to_owned(),
            budget_micros: None,
            currency: None,
            request_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost_micros: None,
        })
        .collect::<Vec<_>>();
    for asset in workspace.wallet.assets.values() {
        let provider = workspace.runtime.providers.get(&asset.provider_instance_id);
        let usage = wallet_usage(&audit, &asset.provider_instance_id);
        cards.push(WalletCard {
            id: asset.id.clone(),
            provider_id: asset.provider_id.clone(),
            provider_instance_id: Some(asset.provider_instance_id.clone()),
            name: asset.name.clone(),
            configured: provider.is_some_and(|item| !item.secret_refs.is_empty()),
            enabled: provider.is_some_and(|item| item.enabled),
            source: "asset".to_owned(),
            budget_micros: asset.billing.monthly_budget_micros,
            currency: asset.billing.currency.clone(),
            request_count: usage.0,
            input_tokens: usage.1,
            output_tokens: usage.2,
            estimated_cost_micros: None,
        });
    }
    Ok(cards)
}

#[tauri::command]
async fn create_wallet_asset(
    input: WalletAssetInput,
    state: State<'_, DesktopState>,
) -> Result<WalletCard, String> {
    let manifest = builtin_provider_manifests()
        .map_err(|error| error.message)?
        .into_iter()
        .find(|manifest| manifest.provider.id == input.provider_id)
        .ok_or_else(|| "Provider catalog entry was not found".to_owned())?;
    let mut workspace = load_workspace(&state.repository)?;
    let base = normalize_id(&input.name, "api");
    let asset_id = unique_map_id(&workspace.wallet.assets, &base);
    let provider_instance_id = unique_map_id(&workspace.runtime.providers, &asset_id);
    workspace.runtime.providers.insert(
        provider_instance_id.clone(),
        ProviderInstance {
            id: provider_instance_id.clone(),
            manifest: manifest.clone(),
            endpoint_override: input.endpoint_override,
            secret_refs: BTreeMap::new(),
            enabled: false,
        },
    );
    let asset = ApiAsset {
        id: asset_id.clone(),
        provider_instance_id,
        provider_id: manifest.provider.id.clone(),
        name: unique_display_name(
            &workspace.wallet.assets,
            &input.name,
            &manifest.provider.name,
        ),
        billing: BillingPolicy {
            monthly_budget_micros: input.monthly_budget_micros,
            currency: input.currency,
            rules: Vec::new(),
        },
    };
    workspace
        .wallet
        .assets
        .insert(asset_id.clone(), asset.clone());
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    Ok(WalletCard {
        id: asset.id,
        provider_id: asset.provider_id,
        provider_instance_id: Some(asset.provider_instance_id),
        name: asset.name,
        configured: false,
        enabled: false,
        source: "asset".to_owned(),
        budget_micros: asset.billing.monthly_budget_micros,
        currency: asset.billing.currency,
        request_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        estimated_cost_micros: None,
    })
}

#[tauri::command]
fn project_tree(state: State<'_, DesktopState>) -> Result<ProjectTreeSnapshot, String> {
    Ok(ProjectTreeSnapshot {
        projects: load_workspace(&state.repository)?.projects,
    })
}

#[tauri::command]
async fn create_project(
    input: ProjectInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let id = unique_map_id(
        &workspace.projects.projects,
        &normalize_id(&input.name, "project"),
    );
    let folder = ProjectFolder {
        id: "canvases".to_owned(),
        name: "Canvases".to_owned(),
        canvas_ids: Vec::new(),
    };
    workspace.projects.projects.insert(
        id.clone(),
        Project {
            id: id.clone(),
            name: limited_name(&input.name, "New project"),
            folders: BTreeMap::from([(folder.id.clone(), folder)]),
            canvases: BTreeMap::new(),
        },
    );
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn rename_project(
    input: RenameProjectInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    project.name = limited_name(&input.name, "Project");
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn delete_project(
    input: ProjectActionInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    if !project.canvases.is_empty() {
        return Err("Project still contains canvases. Move or delete them first.".to_owned());
    }
    workspace.projects.projects.remove(&input.project_id);
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn create_folder(
    input: FolderInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let id = unique_map_id(&project.folders, &normalize_id(&input.name, "folder"));
    project.folders.insert(
        id.clone(),
        ProjectFolder {
            id,
            name: limited_name(&input.name, "New folder"),
            canvas_ids: Vec::new(),
        },
    );
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn rename_folder(
    input: RenameFolderInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let folder = project
        .folders
        .get_mut(&input.folder_id)
        .ok_or_else(|| "Folder not found".to_owned())?;
    folder.name = limited_name(&input.name, "Folder");
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn delete_folder(
    input: FolderActionInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let folder = project
        .folders
        .get(&input.folder_id)
        .ok_or_else(|| "Folder not found".to_owned())?;
    if !folder.canvas_ids.is_empty() {
        return Err("Folder still contains canvases. Move them first.".to_owned());
    }
    project.folders.remove(&input.folder_id);
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn create_canvas(
    input: CanvasInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let id = unique_map_id(&project.canvases, &normalize_id(&input.name, "canvas"));
    let folder_id = input
        .folder_id
        .or_else(|| project.folders.keys().next().cloned());
    if let Some(folder_id) = &folder_id {
        if !project.folders.contains_key(folder_id) {
            return Err("Folder not found".to_owned());
        }
    }
    let canvas = Canvas {
        id: id.clone(),
        name: limited_name(&input.name, "New canvas"),
        folder_id: folder_id.clone(),
        graph: new_canvas_graph(&id),
        applied_graph: None,
        draft_revision: 1,
        applied_revision: 0,
        publisher_id: None,
        legacy_multi_output: false,
    };
    project.canvases.insert(id.clone(), canvas);
    if let Some(folder_id) = folder_id {
        project
            .folders
            .get_mut(&folder_id)
            .expect("checked")
            .canvas_ids
            .push(id.clone());
    }
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn rename_canvas(
    input: RenameCanvasInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .and_then(|project| project.canvases.get_mut(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    canvas.name = limited_name(&input.name, "Canvas");
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn move_canvas(
    input: MoveCanvasInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    if let Some(folder_id) = &input.folder_id
        && !project.folders.contains_key(folder_id)
    {
        return Err("Folder not found".to_owned());
    }
    let previous = project
        .canvases
        .get(&input.canvas_id)
        .ok_or_else(|| "Canvas not found".to_owned())?
        .folder_id
        .clone();
    if let Some(folder_id) = previous
        && let Some(folder) = project.folders.get_mut(&folder_id)
    {
        folder.canvas_ids.retain(|id| id != &input.canvas_id);
    }
    if let Some(folder_id) = &input.folder_id {
        project
            .folders
            .get_mut(folder_id)
            .expect("checked")
            .canvas_ids
            .push(input.canvas_id.clone());
    }
    project
        .canvases
        .get_mut(&input.canvas_id)
        .expect("checked")
        .folder_id = input.folder_id;
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
async fn duplicate_canvas(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let source = project
        .canvases
        .get(&input.canvas_id)
        .cloned()
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let id = unique_map_id(&project.canvases, &format!("{}-copy", source.id));
    let mut graph = source.graph;
    graph.id.clone_from(&id);
    for node in &mut graph.nodes {
        if node.kind == NodeKind::Publisher {
            node.config = serde_json::json!({"publisher_id": null, "status": "not_published"});
        }
    }
    let copy = Canvas {
        id: id.clone(),
        name: format!("{} Copy", source.name),
        folder_id: source.folder_id.clone(),
        graph,
        applied_graph: None,
        draft_revision: 1,
        applied_revision: 0,
        publisher_id: None,
        legacy_multi_output: source.legacy_multi_output,
    };
    project.canvases.insert(id.clone(), copy);
    if let Some(folder_id) = source.folder_id
        && let Some(folder) = project.folders.get_mut(&folder_id)
    {
        folder.canvas_ids.push(id);
    }
    save_projects_workspace(&state, workspace).await
}

#[tauri::command]
fn canvas_graph(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<WorkflowGraph, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .map(|canvas| canvas.graph.clone())
        .ok_or_else(|| "Canvas not found".to_owned())
}

#[tauri::command]
async fn canvas_snapshot(
    input: CanvasActionInput,
    state: State<'_, DesktopState>,
) -> Result<CanvasSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
    let project = workspace
        .projects
        .projects
        .get(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let canvas = project
        .canvases
        .get(&input.canvas_id)
        .cloned()
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let provider_instance_ids = canvas
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            node.config
                .get("provider_instance_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let secret_status =
        workspace.secret_status(&available_secret_refs(&workspace, &state.secret_store));
    let missing_secret_count = provider_instance_ids
        .iter()
        .filter_map(|id| workspace.runtime.providers.get(id))
        .flat_map(|provider| provider.secret_refs.values())
        .filter(|reference| secret_status.missing.contains(reference.as_str()))
        .count();
    let plane_snapshot = state
        .control_plane
        .lock()
        .await
        .clone()
        .map(|plane| async move { plane.snapshot().await });
    let control = match plane_snapshot {
        Some(future) => Some(future.await),
        None => None,
    };
    let publisher = canvas.publisher_id.as_ref().and_then(|publisher_id| {
        let runtime = workspace.runtime.publishers.get(publisher_id)?;
        let summary = runtime.config.validate().ok()?;
        let state = control
            .as_ref()
            .and_then(|snapshot| snapshot.supervisor.publishers.get(publisher_id));
        Some(CanvasPublisherSnapshot {
            id: publisher_id.clone(),
            status: state.map_or(PublisherLifecycle::Stopped, |item| item.lifecycle),
            message: state.and_then(|item| item.last_error.clone()),
            base_url: summary.base_url,
            public_models: runtime
                .routes
                .iter()
                .map(|route| route.public_model.clone())
                .collect(),
        })
    });
    Ok(CanvasSnapshot {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        canvas,
        publisher,
        provider_instance_ids,
        missing_secret_count,
    })
}

#[tauri::command]
async fn save_canvas_graph(
    input: SaveCanvasGraphInput,
    state: State<'_, DesktopState>,
) -> Result<CanvasSnapshot, String> {
    input.graph.validate().map_err(|error| error.message)?;
    let mut workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .and_then(|project| project.canvases.get_mut(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    if input.graph.id != canvas.id {
        return Err("Canvas graph ID must match the target Canvas".to_owned());
    }
    canvas.graph = input.graph;
    canvas.draft_revision = canvas.draft_revision.saturating_add(1);
    workspace.validate().map_err(|error| error.message)?;
    state.repository.save(&workspace).map_err(safe_error)?;
    canvas_snapshot(
        CanvasActionInput {
            project_id: input.project_id,
            canvas_id: input.canvas_id,
        },
        state,
    )
    .await
}

#[tauri::command]
fn canvas_node_impact(
    input: CanvasActionInput,
    node_id: String,
    state: State<'_, DesktopState>,
) -> Result<NodeImpact, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?
        .graph
        .impact_of_node(&node_id)
        .map_err(|error| error.message)
}

#[tauri::command]
async fn commit_wallet_placement(
    input: PlacementInput,
    state: State<'_, DesktopState>,
) -> Result<ProjectTreeSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let asset = workspace
        .wallet
        .assets
        .get(&input.asset_id)
        .cloned()
        .ok_or_else(|| "Configure this catalog provider before placing it".to_owned())?;
    let project = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .ok_or_else(|| "Project not found".to_owned())?;
    let canvas = project
        .canvases
        .get_mut(&input.canvas_id)
        .ok_or_else(|| "Canvas not found".to_owned())?;
    let base = format!("{}-group", asset.id);
    let node_id = unique_node_id(&canvas.graph, &base);
    canvas.graph.nodes.push(Node { id: node_id.clone(), name: asset.name, kind: NodeKind::Group, enabled: true, inputs: vec![Port { id: "request".to_owned(), data_type: PortType::Request }], outputs: vec![Port { id: "request".to_owned(), data_type: PortType::Request }], config: serde_json::json!({"provider_instance_id": asset.provider_instance_id, "members": ["adapter", "safe-probe", "router"]}) });
    let publisher = canvas
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Publisher)
        .map(|node| node.id.clone())
        .ok_or_else(|| "Canvas total output is missing".to_owned())?;
    canvas.graph.edges.push(Edge {
        id: unique_edge_id(&canvas.graph, &format!("{node_id}-to-output")),
        from: Endpoint {
            node: node_id,
            port: "request".to_owned(),
        },
        to: Endpoint {
            node: publisher,
            port: "request".to_owned(),
        },
    });
    canvas.draft_revision = canvas.draft_revision.saturating_add(1);
    if let Some(publisher_id) = canvas.publisher_id.clone()
        && let Some(runtime_publisher) = workspace.runtime.publishers.get_mut(&publisher_id)
    {
        let enabled = workspace
            .runtime
            .providers
            .get(&asset.provider_instance_id)
            .is_some_and(|provider| provider.enabled);
        for route in &mut runtime_publisher.routes {
            if !route
                .upstreams
                .iter()
                .any(|upstream| upstream.provider_instance == asset.provider_instance_id)
            {
                let priority = u32::try_from(route.upstreams.len()).unwrap_or(u32::MAX);
                route.upstreams.push(UpstreamRoute {
                    id: format!("{publisher_id}-{}", asset.id),
                    provider_instance: asset.provider_instance_id.clone(),
                    upstream_model: route.public_model.clone(),
                    priority,
                    enabled,
                    conditions: Vec::new(),
                });
            }
        }
    }
    save_projects_workspace(&state, workspace).await
}
