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

fn load_workspace(repository: &WorkspaceRepository) -> Result<WorkspacePackage, String> {
    match repository.load().map_err(safe_error)?.loaded {
        WorkspaceLoad::Ready { workspace } => Ok(*workspace),
        WorkspaceLoad::ReadOnly { reason, .. } => {
            Err(format!("工作区只能以只读模式打开：{reason}"))
        }
    }
}

async fn reload_control_plane(state: &DesktopState) -> Result<(), String> {
    let control_plane =
        open_control_plane(&state.repository, &state.secret_store).map_err(safe_error)?;
    *state.control_plane.lock().await = Some(control_plane);
    *state.startup_error.lock().await = None;
    Ok(())
}

async fn ensure_no_running_publishers(state: &DesktopState) -> Result<(), String> {
    let plane = state.control_plane.lock().await.clone();
    if let Some(plane) = plane
        && plane.snapshot().await.supervisor.running_count > 0
    {
        return Err("请先暂停或停止正在运行的 Publisher，再修改工作区配置。".to_owned());
    }
    Ok(())
}

async fn snapshot(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let control_plane = state.control_plane.lock().await.clone();
    let startup_error = state.startup_error.lock().await.clone();
    let provider_count = load_workspace(&state.repository)
        .map(|workspace| workspace.runtime.providers.len())
        .unwrap_or(0);
    let control = match control_plane {
        Some(plane) => Some(to_desktop_control_snapshot(
            plane.snapshot().await,
            provider_count,
        )),
        None => None,
    };

    let workspace = load_workspace(&state.repository).ok();
    Ok(DesktopSnapshot {
        initialized: control.is_some(),
        startup_error,
        control,
        gateway: DesktopGatewaySnapshot {
            running: state.gateway.lock().await.is_some(),
            base_url: workspace.as_ref().map_or_else(|| "http://127.0.0.1:7480".to_owned(), |workspace| format!("http://{}:{}", workspace.gateway.listen_address, workspace.gateway.port)),
            entry_count: workspace.as_ref().map_or(0, |workspace| workspace.direct_endpoints.values().filter(|endpoint| endpoint.enabled).count() + workspace.runtime_state.enabled_publishers.len()),
            error: state.gateway_error.lock().await.clone(),
        },
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
        schema_version: WORKSPACE_SCHEMA_VERSION,
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
        wallet: ApiWallet::default(),
        direct_endpoints: BTreeMap::new(),
        gateway: apiarray_core::workspace::DirectGateway::default(),
        projects: default_projects(),
        ui: Value::Null,
        templates: BTreeMap::new(),
    }
}

fn default_projects() -> WorkspaceProjects {
    let folder = ProjectFolder {
        id: "canvases".to_owned(),
        name: "Canvases".to_owned(),
        canvas_ids: vec!["main".to_owned()],
    };
    let canvas = Canvas {
        id: "main".to_owned(),
        name: "Main canvas".to_owned(),
        folder_id: Some(folder.id.clone()),
        graph: new_canvas_graph("main"),
        applied_graph: None,
        draft_revision: 1,
        applied_revision: 0,
        publisher_id: None,
        legacy_multi_output: false,
    };
    let project = Project {
        id: "default".to_owned(),
        name: "My project".to_owned(),
        folders: BTreeMap::from([(folder.id.clone(), folder)]),
        canvases: BTreeMap::from([(canvas.id.clone(), canvas)]),
    };
    WorkspaceProjects {
        projects: BTreeMap::from([(project.id.clone(), project)]),
    }
}

fn direct_endpoint_url(workspace: &WorkspacePackage, alias: &str) -> String {
    format!("http://{}:{}/direct/{alias}/v1", workspace.gateway.listen_address, workspace.gateway.port)
}

async fn restart_gateway(state: &DesktopState) -> Result<(), String> {
    if let Some(previous) = state.gateway.lock().await.take() {
        previous.stop().await;
    }
    let workspace = load_workspace(&state.repository)?;
    let entries = gateway_entries(&workspace, &state.repository, &state.secret_store)?;
    let address: std::net::SocketAddr = format!("{}:{}", workspace.gateway.listen_address, workspace.gateway.port)
        .parse().map_err(|_| "统一审计网关地址无效。".to_owned())?;
    match apiarray_runtime::gateway::LocalGateway::start(address, entries).await {
        Ok(gateway) => {
            eprintln!("API ARRAY LocalGateway listening on {}", gateway.address());
            *state.gateway.lock().await = Some(gateway);
            *state.gateway_error.lock().await = None;
            Ok(())
        }
        Err(error) => {
            let message = safe_error(error);
            *state.gateway_error.lock().await = Some(message.clone());
            Err(message)
        }
    }
}

fn gateway_entries(workspace: &WorkspacePackage, repository: &WorkspaceRepository, store: &Arc<WindowsCredentialStore>) -> Result<Vec<apiarray_runtime::gateway::GatewayEntry>, String> {
    let mut runtime = workspace.runtime.clone();
    let mut prefixes = Vec::new();
    for endpoint in workspace.direct_endpoints.values().filter(|endpoint| endpoint.enabled) {
        let Some(asset) = workspace.wallet.assets.get(&endpoint.asset_id) else { continue; };
        let Some(provider) = runtime.providers.get(&asset.provider_instance_id) else { continue; };
        if !asset.enabled || !provider.enabled || !store.contains(&endpoint.token_ref) { continue; }
        let id = format!("direct:{}", endpoint.id);
        runtime.publishers.insert(id.clone(), RuntimePublisher {
            config: apiarray_core::publisher::PublisherConfig { schema_version: SCHEMA_VERSION, id: id.clone(), name: endpoint.name.clone(), listen_address: "127.0.0.1".parse().map_err(|_| "回环地址无效。")?, port: workspace.gateway.port, base_path: "/v1".to_owned(), require_token: true, token_ref: Some(endpoint.token_ref.clone()) },
            routes: endpoint.models.iter().map(|mapping| ModelRoute { public_model: mapping.public_model.clone(), policy: RoutePolicy { schema_version: SCHEMA_VERSION, id: format!("{id}-{}-route", mapping.public_model), timeout_ms: endpoint.timeout_ms, max_retries: endpoint.max_retries, failover_on: HashSet::from([StandardError::ProviderTimeout, StandardError::NetworkUnreachable, StandardError::RateLimited]) }, upstreams: vec![UpstreamRoute { id: format!("{id}-{}-upstream", mapping.public_model), provider_instance: asset.provider_instance_id.clone(), upstream_model: mapping.upstream_model.clone(), priority: 0, enabled: true, conditions: Vec::new() }] }).collect(),
        });
        prefixes.push((format!("/direct/{}", endpoint.alias), id));
    }
    for project in workspace.projects.projects.values() {
        for canvas in project.canvases.values() {
            if let Some(publisher_id) = &canvas.publisher_id && workspace.runtime_state.enabled_publishers.contains(publisher_id) {
                prefixes.push((format!("/canvas/{}", canvas.id), publisher_id.clone()));
            }
        }
    }
    let compiled = Arc::new(runtime.compile().map_err(|error| error.message)?);
    let audit: Arc<dyn apiarray_runtime::resilience::AuditSink> = Arc::new(JsonlAuditSink::open(audit_path(repository)).map_err(safe_error)?);
    let resolver: Arc<dyn apiarray_runtime::secret::SecretResolver> = Arc::new(StoreSecretResolver::new(store.clone()));
    let transport = apiarray_runtime::transport::HttpExecutor::new(TransportConfig::default()).map_err(safe_error)?;
    Ok(prefixes.into_iter().map(|(prefix, publisher_id)| apiarray_runtime::gateway::GatewayEntry { prefix, state: apiarray_runtime::publisher::PublisherState::with_audit(Arc::clone(&compiled), publisher_id, transport.clone(), Arc::clone(&resolver), Arc::clone(&audit)) }).collect())
}

fn new_canvas_graph(id: &str) -> WorkflowGraph {
    WorkflowGraph {
        schema_version: SCHEMA_VERSION,
        id: id.to_owned(),
        nodes: vec![Node {
            id: "total-output".to_owned(),
            name: "Local total output".to_owned(),
            kind: NodeKind::Publisher,
            enabled: true,
            inputs: vec![Port {
                id: "request".to_owned(),
                data_type: PortType::Request,
            }],
            outputs: Vec::new(),
            config: serde_json::json!({"publisher_id": null, "status": "not_published"}),
        }],
        edges: Vec::new(),
    }
}

fn unique_map_id<T>(items: &BTreeMap<String, T>, base: &str) -> String {
    if !items.contains_key(base) {
        return base.to_owned();
    }
    for index in 2..10_000 {
        let candidate = format!("{base}-{index}");
        if !items.contains_key(&candidate) {
            return candidate;
        }
    }
    format!("{base}-overflow")
}

fn ensure_wallet_asset(workspace: &mut WorkspacePackage, provider_instance_id: &str) {
    if workspace
        .wallet
        .assets
        .values()
        .any(|asset| asset.provider_instance_id == provider_instance_id)
    {
        return;
    }
    let Some(provider) = workspace.runtime.providers.get(provider_instance_id) else {
        return;
    };
    let id = unique_map_id(&workspace.wallet.assets, provider_instance_id);
    let name = unique_display_name(
        &workspace.wallet.assets,
        &provider.manifest.provider.name,
        &provider.manifest.provider.name,
    );
    workspace.wallet.assets.insert(
        id.clone(),
        ApiAsset {
            id,
            provider_instance_id: provider_instance_id.to_owned(),
            provider_id: provider.manifest.provider.id.clone(),
            name,
            enabled: false,
            billing: BillingPolicy::default(),
        },
    );
}

fn unique_display_name(
    assets: &BTreeMap<String, ApiAsset>,
    requested: &str,
    fallback: &str,
) -> String {
    let base = limited_name(requested, fallback);
    let existing = assets
        .values()
        .map(|asset| asset.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    if !existing.contains(&base.to_ascii_lowercase()) {
        return base;
    }
    for index in 2..10_000 {
        let candidate = format!("{base} {index}");
        if !existing.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
    }
    format!("{base} 10000")
}

fn limited_name(input: &str, fallback: &str) -> String {
    let trimmed = input.trim();
    let value = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    value.chars().take(80).collect()
}

fn unique_node_id(graph: &WorkflowGraph, base: &str) -> String {
    let ids = graph
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    if !ids.contains(base) {
        return base.to_owned();
    }
    for index in 2..10_000 {
        let candidate = format!("{base}-{index}");
        if !ids.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base}-overflow")
}

fn unique_edge_id(graph: &WorkflowGraph, base: &str) -> String {
    let ids = graph
        .edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<HashSet<_>>();
    if !ids.contains(base) {
        return base.to_owned();
    }
    for index in 2..10_000 {
        let candidate = format!("{base}-{index}");
        if !ids.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base}-overflow")
}

async fn save_projects_workspace(
    state: &DesktopState,
    workspace: WorkspacePackage,
) -> Result<ProjectTreeSnapshot, String> {
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(state).await?;
    Ok(ProjectTreeSnapshot {
        projects: workspace.projects,
    })
}

fn audit_path(repository: &WorkspaceRepository) -> PathBuf {
    repository.root().join("audit.jsonl")
}

fn wallet_usage(records: &[ExecutionTrace], provider_instance_id: &str) -> (u64, u64, u64) {
    records
        .iter()
        .filter(|trace| {
            trace
                .attempts
                .iter()
                .any(|attempt| attempt.provider_instance == provider_instance_id)
        })
        .fold((0, 0, 0), |(requests, input, output), trace| {
            (
                requests.saturating_add(1),
                input.saturating_add(trace.input_tokens.unwrap_or(0)),
                output.saturating_add(trace.output_tokens.unwrap_or(0)),
            )
        })
}

fn available_secret_refs(
    workspace: &WorkspacePackage,
    store: &WindowsCredentialStore,
) -> BTreeSet<String> {
    workspace
        .required_secret_refs()
        .into_iter()
        .filter(|reference| {
            SecretRef::parse(reference).is_ok_and(|reference| store.contains(&reference))
        })
        .collect()
}

fn open_control_plane(
    repository: &WorkspaceRepository,
    secret_store: &Arc<WindowsCredentialStore>,
) -> Result<ControlPlane, apiarray_runtime::RuntimeError> {
    let audit = JsonlAuditSink::open(audit_path(repository))
        .ok()
        .map(|sink| Arc::new(sink) as Arc<dyn apiarray_runtime::resilience::AuditSink>);
    ControlPlane::open(repository.clone(), secret_store.clone(), audit)
}

fn to_desktop_control_snapshot(
    snapshot: ControlPlaneSnapshot,
    provider_count: usize,
) -> DesktopControlSnapshot {
    DesktopControlSnapshot {
        workspace_id: snapshot.workspace_id,
        workspace_name: snapshot.workspace_name,
        recovered_from_backup: snapshot.recovered_from_backup,
        required_secret_count: snapshot.secrets.required.len(),
        missing_secret_count: snapshot.secrets.missing.len(),
        provider_count,
        publisher_count: snapshot.supervisor.publishers.len(),
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

fn provider_catalog_item(
    manifest: apiarray_core::provider::ProviderManifest,
) -> ProviderCatalogItem {
    ProviderCatalogItem {
        id: manifest.provider.id,
        name: manifest.provider.name,
        category: manifest.provider.category,
        adapter: manifest.adapter.id,
        default_base_url: manifest.endpoint.default_base_url,
        editable_endpoint: manifest.endpoint.editable,
        auth_fields: manifest
            .authentication
            .fields
            .into_iter()
            .map(|field| ProviderAuthField {
                id: field.id,
                label: field.label,
                required: field.required,
                secret: field.secret,
            })
            .collect(),
        probe_count: manifest.probes.len(),
    }
}

fn normalize_id(input: &str, kind: &str) -> String {
    let normalized: String = input
        .trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .take(64)
        .collect();
    if normalized.is_empty() {
        format!("{kind}-default")
    } else {
        normalized
    }
}

fn safe_error(error: apiarray_runtime::RuntimeError) -> String {
    error.safe_message
}
