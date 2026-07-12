struct InstanceControlService;

impl InstanceControlService {
    async fn snapshot(state: &DesktopState) -> Result<ControlCenterSnapshot, String> {
        build_control_center_snapshot(state).await
    }

    async fn control(state: &DesktopState, app: &AppHandle, ids: Vec<String>, start: bool, allow_warnings: bool) -> Result<InstanceBatchResult, String> {
        control_instances(state, app, ids, start, allow_warnings).await
    }
}

fn managed_instance_id(kind: ManagedInstanceKind, first: &str, second: Option<&str>) -> String {
    match kind {
        ManagedInstanceKind::DirectEndpoint => format!("direct:{first}"),
        ManagedInstanceKind::Canvas => format!("canvas:{first}:{}", second.unwrap_or_default()),
    }
}

fn parse_managed_instance_id(id: &str) -> Result<(ManagedInstanceKind, String, Option<String>), String> {
    let parts = id.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["direct", endpoint_id] if !endpoint_id.is_empty() => Ok((ManagedInstanceKind::DirectEndpoint, (*endpoint_id).to_owned(), None)),
        ["canvas", project_id, canvas_id] if !project_id.is_empty() && !canvas_id.is_empty() => Ok((ManagedInstanceKind::Canvas, (*project_id).to_owned(), Some((*canvas_id).to_owned()))),
        _ => Err("实例 ID 无效。".to_owned()),
    }
}

fn trace_summary<'a>(audit: &'a [ExecutionTrace], publisher_id: &str) -> (u64, Option<&'a ExecutionTrace>) {
    let matching = audit.iter().filter(|trace| trace.publisher_id == publisher_id).collect::<Vec<_>>();
    (matching.len() as u64, matching.into_iter().max_by_key(|trace| trace.started_at_unix_ms))
}

fn provider_secret_reasons(workspace: &WorkspacePackage, asset_id: &str, store: &WindowsCredentialStore) -> Vec<String> {
    let mut reasons = Vec::new();
    let Some(asset) = workspace.wallet.assets.get(asset_id) else {
        reasons.push("引用的钱包资产不存在。".to_owned());
        return reasons;
    };
    if !asset.enabled { reasons.push(format!("钱包资产“{}”已停用。", asset.name)); }
    let Some(provider) = workspace.runtime.providers.get(&asset.provider_instance_id) else {
        reasons.push(format!("钱包资产“{}”缺少 Provider 实例。", asset.name));
        return reasons;
    };
    if !provider.enabled { reasons.push(format!("Provider“{}”已停用。", provider.id)); }
    if provider.secret_refs.is_empty() || provider.secret_refs.values().any(|reference| !store.contains(reference)) {
        reasons.push(format!("钱包资产“{}”缺少上游 Secret。", asset.name));
    }
    reasons
}

async fn build_control_center_snapshot(state: &DesktopState) -> Result<ControlCenterSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
    let audit = state.repository.read_audit(1_000).map_err(safe_error)?;
    let gateway = state.gateway.lock().await;
    let gateway_running = gateway.is_some();
    let mounted = gateway.as_ref().map(|item| item.entry_prefixes().iter().cloned().collect::<BTreeSet<_>>()).unwrap_or_default();
    let gateway_error = state.gateway_error.lock().await.clone();
    let base = format!("http://{}:{}", workspace.gateway.listen_address, workspace.gateway.port);
    let mut instances = Vec::new();

    for endpoint in workspace.direct_endpoints.values() {
        let id = managed_instance_id(ManagedInstanceKind::DirectEndpoint, &endpoint.id, None);
        let prefix = format!("/direct/{}", endpoint.alias);
        let token_ready = state.secret_store.contains(&endpoint.token_ref);
        let mut blocking_reasons = provider_secret_reasons(&workspace, &endpoint.asset_id, &state.secret_store);
        if !token_ready { blocking_reasons.push("本地访问 Token 尚未配置。".to_owned()); }
        if endpoint.models.is_empty() { blocking_reasons.push("尚未配置公开模型映射。".to_owned()); }
        let actual_running = gateway_running && mounted.contains(&prefix);
        let status = if endpoint.enabled && actual_running { ManagedInstanceStatus::Running }
            else if endpoint.enabled && !blocking_reasons.is_empty() { ManagedInstanceStatus::Blocked }
            else if endpoint.enabled { ManagedInstanceStatus::Failed }
            else if !blocking_reasons.is_empty() { ManagedInstanceStatus::Blocked }
            else { ManagedInstanceStatus::Stopped };
        let publisher_id = format!("direct:{}", endpoint.id);
        let (request_count, last) = trace_summary(&audit, &publisher_id);
        instances.push(ManagedInstance {
            id, kind: ManagedInstanceKind::DirectEndpoint, name: endpoint.name.clone(), ownership: "审计直出".to_owned(),
            project_id: None, project_name: None, canvas_id: None, direct_endpoint_id: Some(endpoint.id.clone()), audit_publisher_id: Some(publisher_id.clone()),
            asset_names: workspace.wallet.assets.get(&endpoint.asset_id).map(|asset| vec![asset.name.clone()]).unwrap_or_default(),
            base_url: Some(format!("{base}{prefix}/v1")), public_models: endpoint.models.iter().map(|item| item.public_model.clone()).collect(),
            token_ready, secrets_ready: !blocking_reasons.iter().any(|item| item.contains("Secret")), desired_running: endpoint.enabled,
            status, blocking_reasons, repair_target: Some(if !token_ready { "direct" } else { "wallet" }.to_owned()),
            draft_revision: None, applied_revision: None, has_unapplied_changes: false, request_count,
            last_call_at_ms: last.map(|trace| trace.started_at_unix_ms), last_latency_ms: last.map(|trace| trace.total_latency_ms),
            retry_count: last.map_or(0, |trace| trace.retry_count), failover_count: last.map_or(0, |trace| trace.failover_count),
            last_error: last.and_then(|trace| trace.final_error.as_ref()).map(|error| format!("{error:?}")),
        });
    }

    for project in workspace.projects.projects.values() {
        for canvas in project.canvases.values() {
            let id = managed_instance_id(ManagedInstanceKind::Canvas, &project.id, Some(&canvas.id));
            let publisher = canvas.publisher_id.as_ref().and_then(|publisher_id| workspace.runtime.publishers.get(publisher_id));
            let desired_running = canvas.publisher_id.as_ref().is_some_and(|publisher_id| workspace.runtime_state.enabled_publishers.contains(publisher_id));
            let prefix = format!("/canvas/{}", canvas.id);
            let actual_running = gateway_running && mounted.contains(&prefix);
            let mut blocking_reasons = Vec::new();
            let asset_ids = canvas.graph.nodes.iter().filter_map(|node| node.config.get("asset_id").and_then(Value::as_str)).collect::<BTreeSet<_>>();
            for asset_id in &asset_ids { blocking_reasons.extend(provider_secret_reasons(&workspace, asset_id, &state.secret_store)); }
            let compilation = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime);
            if let Err(error) = &compilation { blocking_reasons.push(error.message.clone()); }
            let token_ready = publisher.and_then(|item| item.config.token_ref.as_ref()).is_some_and(|reference| state.secret_store.contains(reference));
            if publisher.is_some() && !token_ready { blocking_reasons.push("Canvas 本地访问 Token 尚未配置。".to_owned()); }
            let status = if publisher.is_none() { ManagedInstanceStatus::Unpublished }
                else if desired_running && actual_running { ManagedInstanceStatus::Running }
                else if desired_running && !blocking_reasons.is_empty() { ManagedInstanceStatus::Blocked }
                else if desired_running { ManagedInstanceStatus::Failed }
                else if !blocking_reasons.is_empty() { ManagedInstanceStatus::Blocked }
                else { ManagedInstanceStatus::Stopped };
            let publisher_id = canvas.publisher_id.as_deref().unwrap_or_default();
            let (request_count, last) = trace_summary(&audit, publisher_id);
            instances.push(ManagedInstance {
                id, kind: ManagedInstanceKind::Canvas, name: canvas.name.clone(), ownership: format!("{} / {}", project.name, canvas.name),
                project_id: Some(project.id.clone()), project_name: Some(project.name.clone()), canvas_id: Some(canvas.id.clone()), direct_endpoint_id: None, audit_publisher_id: canvas.publisher_id.clone(),
                asset_names: asset_ids.iter().filter_map(|asset_id| workspace.wallet.assets.get(*asset_id).map(|asset| asset.name.clone())).collect(),
                base_url: publisher.map(|_| format!("{base}{prefix}/v1")),
                public_models: publisher.map(|item| item.routes.iter().map(|route| route.public_model.clone()).collect()).unwrap_or_default(),
                token_ready, secrets_ready: !blocking_reasons.iter().any(|item| item.contains("Secret")), desired_running, status,
                blocking_reasons, repair_target: Some(if publisher.is_none() || !token_ready { "canvas:publisher" } else { "canvas:workflow" }.to_owned()),
                draft_revision: Some(canvas.draft_revision), applied_revision: Some(canvas.applied_revision), has_unapplied_changes: canvas.draft_revision != canvas.applied_revision,
                request_count, last_call_at_ms: last.map(|trace| trace.started_at_unix_ms), last_latency_ms: last.map(|trace| trace.total_latency_ms),
                retry_count: last.map_or(0, |trace| trace.retry_count), failover_count: last.map_or(0, |trace| trace.failover_count),
                last_error: last.and_then(|trace| trace.final_error.as_ref()).map(|error| format!("{error:?}")),
            });
        }
    }
    instances.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()).then_with(|| left.id.cmp(&right.id)));
    let running_count = instances.iter().filter(|item| item.status == ManagedInstanceStatus::Running).count();
    let stopped_count = instances.iter().filter(|item| item.status == ManagedInstanceStatus::Stopped).count();
    let blocked_count = instances.iter().filter(|item| matches!(item.status, ManagedInstanceStatus::Blocked | ManagedInstanceStatus::Unpublished)).count();
    let failed_count = instances.iter().filter(|item| item.status == ManagedInstanceStatus::Failed).count();
    Ok(ControlCenterSnapshot {
        gateway: DesktopGatewaySnapshot { running: gateway_running, base_url: base, entry_count: mounted.len(), error: gateway_error },
        instances, running_count, stopped_count, blocked_count, failed_count,
    })
}

#[tauri::command]
async fn control_center_snapshot(state: State<'_, DesktopState>) -> Result<ControlCenterSnapshot, String> {
    InstanceControlService::snapshot(&state).await
}

fn apply_instance_start(workspace: &mut WorkspacePackage, store: &WindowsCredentialStore, instance_id: &str, allow_warnings: bool) -> Result<bool, String> {
    let (kind, first, second) = parse_managed_instance_id(instance_id)?;
    match kind {
        ManagedInstanceKind::DirectEndpoint => {
            let endpoint = workspace.direct_endpoints.get(&first).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
            let reasons = provider_secret_reasons(workspace, &endpoint.asset_id, store);
            if !store.contains(&endpoint.token_ref) { return Err("本地访问 Token 尚未配置。".to_owned()); }
            if !reasons.is_empty() { return Err(reasons.join(" ")); }
            let endpoint = workspace.direct_endpoints.get_mut(&first).expect("endpoint checked");
            let changed = !endpoint.enabled;
            endpoint.enabled = true;
            return Ok(changed);
        }
        ManagedInstanceKind::Canvas => {
            let canvas_id = second.ok_or_else(|| "Canvas ID 缺失。".to_owned())?;
            let canvas = workspace.projects.projects.get(&first).and_then(|project| project.canvases.get(&canvas_id)).cloned().ok_or_else(|| "Canvas 不存在或归属不正确。".to_owned())?;
            let publisher_id = canvas.publisher_id.clone().ok_or_else(|| "Canvas 尚未配置出口。".to_owned())?;
            let publisher = workspace.runtime.publishers.get(&publisher_id).ok_or_else(|| "Canvas Publisher 不存在。".to_owned())?;
            let token_ref = publisher.config.token_ref.as_ref().ok_or_else(|| "Canvas 尚未配置本地 Token。".to_owned())?;
            if !store.contains(token_ref) { return Err("Canvas 本地 Token 不存在。".to_owned()); }
            for asset_id in canvas.graph.nodes.iter().filter_map(|node| node.config.get("asset_id").and_then(Value::as_str)) {
                let reasons = provider_secret_reasons(workspace, asset_id, store);
                if !reasons.is_empty() { return Err(reasons.join(" ")); }
            }
            let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime).map_err(|error| error.message)?;
            if !allow_warnings && !compiled.report.warnings.is_empty() { return Err(format!("需要确认能力警告：{}", compiled.report.warnings.join("；"))); }
            let was_running = workspace.runtime_state.enabled_publishers.contains(&publisher_id);
            let has_unapplied_changes = canvas.draft_revision != canvas.applied_revision;
            workspace.runtime.publishers.get_mut(&publisher_id).expect("publisher checked").routes = compiled.routes;
            let target = workspace.projects.projects.get_mut(&first).and_then(|project| project.canvases.get_mut(&canvas_id)).expect("canvas checked");
            target.applied_graph = Some(target.graph.clone());
            target.applied_revision = target.draft_revision;
            workspace.runtime_state.enabled_publishers.insert(publisher_id);
            return Ok(!was_running || has_unapplied_changes);
        }
    }
}

fn apply_instance_stop(workspace: &mut WorkspacePackage, instance_id: &str) -> Result<bool, String> {
    let (kind, first, second) = parse_managed_instance_id(instance_id)?;
    match kind {
        ManagedInstanceKind::DirectEndpoint => {
            let endpoint = workspace.direct_endpoints.get_mut(&first).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
            let changed = endpoint.enabled;
            endpoint.enabled = false;
            Ok(changed)
        }
        ManagedInstanceKind::Canvas => {
            let canvas_id = second.ok_or_else(|| "Canvas ID 缺失。".to_owned())?;
            let canvas = workspace.projects.projects.get(&first).and_then(|project| project.canvases.get(&canvas_id)).ok_or_else(|| "Canvas 不存在或归属不正确。".to_owned())?;
            Ok(canvas.publisher_id.as_ref().is_some_and(|publisher_id| workspace.runtime_state.enabled_publishers.remove(publisher_id)))
        }
    }
}

async fn control_instances(state: &DesktopState, app: &AppHandle, ids: Vec<String>, start: bool, allow_warnings: bool) -> Result<InstanceBatchResult, String> {
    let before = build_control_center_snapshot(state).await?;
    let previous = before.instances.iter().map(|item| (item.id.clone(), item.status)).collect::<BTreeMap<_, _>>();
    let original = load_workspace(&state.repository)?;
    let mut next = original.clone();
    let mut results = Vec::new();
    let mut changed = false;
    for id in ids {
        let old_status = previous.get(&id).copied().unwrap_or(ManagedInstanceStatus::Failed);
        let operation = if start { apply_instance_start(&mut next, &state.secret_store, &id, allow_warnings) } else { apply_instance_stop(&mut next, &id) };
        match operation {
            Ok(item_changed) => {
                changed |= item_changed;
                results.push(ManagedInstanceActionResult { instance_id: id, previous_status: old_status, next_status: if start { ManagedInstanceStatus::Running } else { ManagedInstanceStatus::Stopped }, success: true, skipped: !item_changed, message: if item_changed { if start { "已加入运行计划。" } else { "已停止接收新请求。" } } else { "状态无需更改。" }.to_owned(), repair_target: None });
            }
            Err(message) => results.push(ManagedInstanceActionResult { instance_id: id, previous_status: old_status, next_status: old_status, success: false, skipped: false, message, repair_target: Some("details".to_owned()) }),
        }
    }
    let mut gateway_refreshed = false;
    if changed {
        next.validate().map_err(|error| error.message)?;
        state.repository.save(&next).map_err(safe_error)?;
        if let Err(error) = reload_control_plane(state).await.and_then(|_| Ok(())) {
            let _ = state.repository.save(&original);
            let _ = reload_control_plane(state).await;
            return Err(error);
        }
        if let Err(error) = restart_gateway(state).await {
            let _ = state.repository.save(&original);
            let _ = reload_control_plane(state).await;
            let _ = restart_gateway(state).await;
            return Err(format!("网关刷新失败，运行状态已回滚：{error}"));
        }
        gateway_refreshed = true;
    }
    let snapshot = build_control_center_snapshot(state).await?;
    for result in &mut results {
        if let Some(instance) = snapshot.instances.iter().find(|item| item.id == result.instance_id) { result.next_status = instance.status; }
    }
    let _ = app.emit("desktop:instances-changed", ());
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("API ARRAY · {} 个实例运行中", snapshot.running_count)));
    }
    let total = results.len();
    let succeeded = results.iter().filter(|item| item.success && !item.skipped).count();
    let skipped = results.iter().filter(|item| item.skipped).count();
    let failed = results.iter().filter(|item| !item.success).count();
    Ok(InstanceBatchResult { total, succeeded, failed, skipped, gateway_refreshed, results, snapshot })
}

#[tauri::command]
async fn start_managed_instance(input: ManagedInstanceActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    InstanceControlService::control(&state, &app, vec![input.instance_id], true, input.allow_warnings).await
}

#[tauri::command]
async fn stop_managed_instance(input: ManagedInstanceActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    InstanceControlService::control(&state, &app, vec![input.instance_id], false, false).await
}

#[tauri::command]
async fn start_all_instances(app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    let ids = build_control_center_snapshot(&state).await?.instances.into_iter().map(|item| item.id).collect();
    InstanceControlService::control(&state, &app, ids, true, false).await
}

#[tauri::command]
async fn stop_all_instances(app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    let ids = build_control_center_snapshot(&state).await?.instances.into_iter().map(|item| item.id).collect();
    InstanceControlService::control(&state, &app, ids, false, false).await
}

#[tauri::command]
async fn start_instances_by_kind(input: ManagedInstanceKindInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    let ids = build_control_center_snapshot(&state).await?.instances.into_iter().filter(|item| item.kind == input.kind).map(|item| item.id).collect();
    InstanceControlService::control(&state, &app, ids, true, input.allow_warnings).await
}

#[tauri::command]
async fn stop_instances_by_kind(input: ManagedInstanceKindInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<InstanceBatchResult, String> {
    let ids = build_control_center_snapshot(&state).await?.instances.into_iter().filter(|item| item.kind == input.kind).map(|item| item.id).collect();
    InstanceControlService::control(&state, &app, ids, false, false).await
}

#[tauri::command]
async fn test_managed_instance(input: ManagedInstanceActionInput, state: State<'_, DesktopState>) -> Result<PublisherConnectionTest, String> {
    let snapshot = build_control_center_snapshot(&state).await?;
    let instance = snapshot.instances.iter().find(|item| item.id == input.instance_id).ok_or_else(|| "实例不存在。".to_owned())?;
    if instance.status != ManagedInstanceStatus::Running { return Err("实例尚未运行。".to_owned()); }
    let workspace = load_workspace(&state.repository)?;
    let (kind, first, second) = parse_managed_instance_id(&input.instance_id)?;
    let token_ref = match kind {
        ManagedInstanceKind::DirectEndpoint => workspace.direct_endpoints.get(&first).map(|item| &item.token_ref).ok_or_else(|| "审计直出端点不存在。".to_owned())?,
        ManagedInstanceKind::Canvas => {
            let canvas_id = second.ok_or_else(|| "Canvas ID 缺失。".to_owned())?;
            let publisher_id = workspace.projects.projects.get(&first).and_then(|project| project.canvases.get(&canvas_id)).and_then(|canvas| canvas.publisher_id.as_ref()).ok_or_else(|| "Canvas 尚未发布。".to_owned())?;
            workspace.runtime.publishers.get(publisher_id).and_then(|publisher| publisher.config.token_ref.as_ref()).ok_or_else(|| "Canvas Token 未配置。".to_owned())?
        }
    };
    let token = state.secret_store.get(token_ref).map_err(safe_error)?;
    let base_url = instance.base_url.as_ref().ok_or_else(|| "实例没有本地地址。".to_owned())?;
    let started = Instant::now();
    let response = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|_| "无法创建本地自检客户端。".to_owned())?.get(format!("{base_url}/models")).bearer_auth(token.expose()).send().await.map_err(|_| "无法连接本地实例。".to_owned())?;
    if !response.status().is_success() { return Err(format!("本地实例返回 HTTP {}。", response.status().as_u16())); }
    let payload: Value = response.json().await.map_err(|_| "本地实例返回了无效 JSON。".to_owned())?;
    Ok(PublisherConnectionTest { reachable: true, latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX), model_count: model_count_from_payload(&payload), safe_summary: "本地鉴权与模型列表验证通过。".to_owned() })
}
