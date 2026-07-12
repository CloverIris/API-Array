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

trait RepositoryAccess { fn active_repository(&self) -> WorkspaceRepository; }
impl RepositoryAccess for WorkspaceRepository { fn active_repository(&self) -> WorkspaceRepository { self.clone() } }
impl RepositoryAccess for ActiveWorkspace { fn active_repository(&self) -> WorkspaceRepository { self.current() } }

fn load_workspace(repository: &impl RepositoryAccess) -> Result<WorkspacePackage, String> {
    let WorkspaceLoad::Ready { workspace } = repository.active_repository().load().map_err(safe_error)?.loaded;
    Ok(*workspace)
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
        Some(plane) => {
            let plane_snapshot = plane.snapshot().await;
            let system_notifications_enabled = state.repository.current().read_setting("desktop.system_notifications").ok().flatten().map_or(true, |value| value != "false");
            for notification in &plane_snapshot.notifications {
                let changed = state.repository.upsert_notification(notification).unwrap_or(false);
                if changed && system_notifications_enabled && matches!(notification.level, apiarray_core::events::NotificationLevel::System | apiarray_core::events::NotificationLevel::ActionRequired) {
                    let _ = state.app.notification().builder().title("API ARRAY").body(&notification.event.summary).show();
                }
            }
            let notifications = state.repository.read_notifications(apiarray_runtime::persistence::NotificationQuery { unread_only: false, limit: 200 }).unwrap_or_default();
            Some(to_desktop_control_snapshot(plane_snapshot, provider_count, notifications))
        }
        None => None,
    };

    let workspace = load_workspace(&state.repository).ok();
    let gateway = state.gateway.lock().await;
    Ok(DesktopSnapshot {
        initialized: control.is_some(),
        startup_error,
        control,
        gateway: DesktopGatewaySnapshot {
            running: gateway.is_some(),
            base_url: workspace.as_ref().map_or_else(|| "http://127.0.0.1:7480".to_owned(), |workspace| gateway_origin(&workspace.gateway.listen_address, workspace.gateway.port)),
            entry_count: gateway.as_ref().map_or(0, |item| item.entry_prefixes().len()),
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

fn gateway_origin(address: &str, port: u16) -> String {
    if address.contains(':') { format!("http://[{address}]:{port}") } else { format!("http://{address}:{port}") }
}

fn gateway_socket_address(address: &str, port: u16) -> Result<std::net::SocketAddr, String> {
    let ip = address.parse::<std::net::IpAddr>().map_err(|_| "监听地址必须是本机回环 IP。".to_owned())?;
    if !ip.is_loopback() { return Err("统一网关只能监听 127.0.0.1 或 ::1。".to_owned()); }
    Ok(std::net::SocketAddr::new(ip, port))
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
    format!("{}/direct/{alias}/v1", gateway_origin(&workspace.gateway.listen_address, workspace.gateway.port))
}

fn canvas_endpoint_url(workspace: &WorkspacePackage, project_id: &str, canvas_id: &str) -> String {
    format!("{}/canvas/{project_id}/{canvas_id}/v1", gateway_origin(&workspace.gateway.listen_address, workspace.gateway.port))
}

async fn restart_gateway(state: &DesktopState) -> Result<(), String> {
    if state.is_quitting.load(Ordering::SeqCst) {
        return Ok(());
    }
    let workspace = load_workspace(&state.repository)?;
    let entries = gateway_entries(&workspace, &state.repository, &state.secret_store)?;
    let address = gateway_socket_address(&workspace.gateway.listen_address, workspace.gateway.port)?;
    let entry_prefixes = entries.iter().map(|entry| entry.prefix.clone()).collect::<Vec<_>>();

    {
        let gateway = state.gateway.lock().await;
        if let Some(gateway) = gateway.as_ref()
            && gateway.address() == address
            && gateway.entry_prefixes() == entry_prefixes.as_slice()
        {
            *state.gateway_error.lock().await = None;
            return Ok(());
        }
    }

    // Build the route set before touching the current gateway. Binding the same
    // port still requires a short hand-over, so retain the old workspace and
    // restore it if the new listener cannot be created.
    let previous_workspace = workspace.clone();
    let previous = state.gateway.lock().await.take();
    if let Some(previous) = previous { previous.stop().await; }
    match apiarray_runtime::gateway::LocalGateway::start(address, entries).await {
        Ok(gateway) => {
            eprintln!("API ARRAY LocalGateway listening on {}", gateway.address());
            *state.gateway.lock().await = Some(gateway);
            *state.gateway_error.lock().await = None;
            Ok(())
        }
        Err(error) => {
            let message = safe_error(error);
            // Best-effort recovery keeps an already valid workspace reachable
            // when a new port or route configuration is rejected.
            if let Ok(old_entries) = gateway_entries(&previous_workspace, &state.repository, &state.secret_store) {
                if let Ok(old_gateway) = apiarray_runtime::gateway::LocalGateway::start(
                    gateway_socket_address(&previous_workspace.gateway.listen_address, previous_workspace.gateway.port)?,
                    old_entries,
                ).await {
                    *state.gateway.lock().await = Some(old_gateway);
                }
            }
            *state.gateway_error.lock().await = Some(message.clone());
            Err(message)
        }
    }
}

fn gateway_entries(workspace: &WorkspacePackage, repository: &impl RepositoryAccess, store: &Arc<WindowsCredentialStore>) -> Result<Vec<apiarray_runtime::gateway::GatewayEntry>, String> {
    let repository = repository.active_repository();
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
                let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime)
                    .map_err(|error| format!("Canvas {}/{} compilation blocked: {}", project.id, canvas.id, error.message))?;
                if let Some(publisher) = runtime.publishers.get_mut(publisher_id) {
                    publisher.routes = compiled.routes;
                } else {
                    return Err(format!("Canvas {}/{} references a missing Publisher", project.id, canvas.id));
                }
                // Canvas IDs are only unique within a Project. Keep the public
                // route explicitly scoped so two Projects can never collide.
                prefixes.push((format!("/canvas/{}/{}", project.id, canvas.id), publisher_id.clone()));
            }
        }
    }
    let compiled = Arc::new(runtime.compile().map_err(|error| error.message)?);
    let audit: Arc<dyn apiarray_runtime::resilience::AuditSink> = Arc::new(SqliteAuditSink::new(repository));
    let resolver: Arc<dyn apiarray_runtime::secret::SecretResolver> = Arc::new(StoreSecretResolver::new(store.clone()));
    let transport = apiarray_runtime::transport::HttpExecutor::new(TransportConfig::default()).map_err(safe_error)?;
    Ok(prefixes.into_iter().map(|(prefix, publisher_id)| apiarray_runtime::gateway::GatewayEntry { prefix, state: apiarray_runtime::publisher::PublisherState::with_audit(Arc::clone(&compiled), publisher_id, transport.clone(), Arc::clone(&resolver), Arc::clone(&audit)) }).collect())
}

fn new_canvas_graph(id: &str) -> WorkflowGraph {
    WorkflowGraph {
        schema_version: GRAPH_SCHEMA_VERSION,
        id: id.to_owned(),
        nodes: vec![Node {
            id: "composer".to_owned(),
            name: "Composer".to_owned(),
            kind: NodeKind::Composer,
            enabled: true,
            inputs: vec![Port { id: "candidate_in".to_owned(), data_type: PortType::Candidate }, Port { id: "health_in".to_owned(), data_type: PortType::HealthSignal }],
            outputs: vec![Port { id: "service_plan_out".to_owned(), data_type: PortType::ServicePlan }],
            config: serde_json::json!({"strategy":"priority_failover","timeout_ms":30000,"max_retries":2,"allow_capability_degradation":false}),
        }, Node {
            id: "total-output".to_owned(),
            name: "Local total output".to_owned(),
            kind: NodeKind::Publisher,
            enabled: true,
            inputs: vec![Port {
                id: "service_plan_in".to_owned(),
                data_type: PortType::ServicePlan,
            }],
            outputs: Vec::new(),
            config: serde_json::json!({"publisher_id": null, "status": "not_published"}),
        }],
        edges: vec![Edge { id: "composer-to-output".to_owned(), from: Endpoint { node: "composer".to_owned(), port: "service_plan_out".to_owned() }, to: Endpoint { node: "total-output".to_owned(), port: "service_plan_in".to_owned() } }],
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
    repository: &impl RepositoryAccess,
    secret_store: &Arc<WindowsCredentialStore>,
) -> Result<ControlPlane, apiarray_runtime::RuntimeError> {
    let repository = repository.active_repository();
    let audit = Some(Arc::new(SqliteAuditSink::new(repository.clone())) as Arc<dyn apiarray_runtime::resilience::AuditSink>);
    ControlPlane::open(repository, secret_store.clone(), audit)
}

fn to_desktop_control_snapshot(
    snapshot: ControlPlaneSnapshot,
    provider_count: usize,
    notifications: Vec<apiarray_runtime::persistence::StoredNotification>,
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
        notifications: notifications
            .into_iter()
            .map(|notification| DesktopNotification {
                id: notification.id,
                message: notification.summary,
                created_at: notification.last_seen_at_ms,
                level: notification.level,
                object_id: notification.object_id,
                occurrence_count: notification.occurrence_count,
                read: notification.read,
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
