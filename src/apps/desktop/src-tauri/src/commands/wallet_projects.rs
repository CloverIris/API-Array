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
fn wallet_assets(state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> { wallet_cards(&state) }

#[tauri::command]
fn wallet_gallery(state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> { wallet_cards(&state) }

fn wallet_cards(state: &DesktopState) -> Result<Vec<WalletCard>, String> {
    let workspace = load_workspace(&state.repository)?;
    let audit = state.repository.read_audit(2_000).unwrap_or_default();
    let mut cards = builtin_provider_manifests().map_err(|error| error.message)?.into_iter().map(|manifest| WalletCard {
        id: format!("catalog:{}", manifest.provider.id), provider_id: manifest.provider.id, provider_instance_id: None,
        name: manifest.provider.name, endpoint_override: Some(manifest.endpoint.default_base_url), configured: false, enabled: false, source: "catalog".to_owned(), budget_micros: None,
        currency: None, request_count: 0, input_tokens: 0, output_tokens: 0, estimated_cost_micros: None, reference_count: 0,
    }).collect::<Vec<_>>();
    for asset in workspace.wallet.assets.values() {
        let provider = workspace.runtime.providers.get(&asset.provider_instance_id);
        let usage = wallet_usage(&audit, &asset.provider_instance_id);
        let direct = workspace.direct_endpoints.values().filter(|endpoint| endpoint.asset_id == asset.id).count();
        let canvases = workspace.projects.projects.values().flat_map(|project| project.canvases.values()).filter(|canvas| canvas.graph.nodes.iter().any(|node| node.config.get("asset_id").and_then(Value::as_str) == Some(asset.id.as_str()))).count();
        cards.push(WalletCard { id: asset.id.clone(), provider_id: asset.provider_id.clone(), provider_instance_id: Some(asset.provider_instance_id.clone()), name: asset.name.clone(), endpoint_override: provider.and_then(|item| item.endpoint_override.clone()), configured: provider.is_some_and(|item| !item.secret_refs.is_empty()), enabled: asset.enabled && provider.is_some_and(|item| item.enabled), source: "asset".to_owned(), budget_micros: asset.billing.monthly_budget_micros, currency: asset.billing.currency.clone(), request_count: usage.0, input_tokens: usage.1, output_tokens: usage.2, estimated_cost_micros: None, reference_count: direct + canvases });
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
            enabled: input.api_key.as_deref().is_some_and(|value| !value.trim().is_empty()),
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
        enabled: input.api_key.as_deref().is_some_and(|value| !value.trim().is_empty()),
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
    if let Some(api_key) = input.api_key.filter(|value| !value.trim().is_empty()) {
        let field = manifest.authentication.fields.iter().find(|field| field.secret && field.required).map(|field| field.id.clone()).unwrap_or_else(|| "api_key".to_owned());
        let reference = SecretRef::parse(format!("secret://wallet/{asset_id}/{field}")).map_err(|error| error.message)?;
        state.secret_store.put(&reference, SecretValue::new(api_key)).map_err(safe_error)?;
        if let Some(provider) = workspace.runtime.providers.get_mut(&asset.provider_instance_id) {
            provider.secret_refs.insert(field, reference);
            provider.enabled = true;
        }
    }
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    Ok(WalletCard {
        id: asset.id,
        provider_id: asset.provider_id,
        provider_instance_id: Some(asset.provider_instance_id.clone()),
        name: asset.name,
        endpoint_override: workspace.runtime.providers.get(&asset.provider_instance_id).and_then(|provider| provider.endpoint_override.clone()),
        configured: workspace.runtime.providers.get(&asset.provider_instance_id).is_some_and(|provider| !provider.secret_refs.is_empty()),
        enabled: asset.enabled,
        source: "asset".to_owned(),
        budget_micros: asset.billing.monthly_budget_micros,
        currency: asset.billing.currency,
        request_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        estimated_cost_micros: None,
        reference_count: 0,
    })
}

#[tauri::command]
async fn update_wallet_asset(input: UpdateWalletAssetInput, state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let asset = workspace.wallet.assets.get_mut(&input.asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    asset.name = limited_name(&input.name, "API asset"); asset.enabled = input.enabled;
    asset.billing.monthly_budget_micros = input.monthly_budget_micros; asset.billing.currency = input.currency;
    let provider = workspace.runtime.providers.get_mut(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    provider.endpoint_override = input.endpoint_override; provider.enabled = input.enabled;
    if let Some(value) = input.api_key.filter(|value| !value.trim().is_empty()) {
        let field = provider.manifest.authentication.fields.iter().find(|field| field.secret && field.required).map(|field| field.id.clone()).unwrap_or_else(|| "api_key".to_owned());
        let reference = provider.secret_refs.get(&field).cloned().unwrap_or(SecretRef::parse(format!("secret://wallet/{}/{field}", asset.id)).map_err(|error| error.message)?);
        state.secret_store.put(&reference, SecretValue::new(value)).map_err(safe_error)?; provider.secret_refs.insert(field, reference);
    }
    workspace.validate().map_err(|error| error.message)?; state.repository.save(&workspace).map_err(safe_error)?; reload_control_plane(&state).await?; restart_gateway(&state).await?; wallet_cards(&state)
}

#[tauri::command]
fn wallet_asset_impact(asset_id: String, state: State<'_, DesktopState>) -> Result<WalletAssetImpact, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    Ok(WalletAssetImpact { direct_endpoints: workspace.direct_endpoints.values().filter(|endpoint| endpoint.asset_id == asset_id).map(|endpoint| endpoint.name.clone()).collect(), canvases: workspace.projects.projects.values().flat_map(|project| project.canvases.values()).filter(|canvas| canvas.graph.nodes.iter().any(|node| node.config.get("asset_id").and_then(Value::as_str) == Some(asset_id.as_str()))).map(|canvas| canvas.name.clone()).collect() })
}

#[tauri::command]
async fn delete_wallet_asset(asset_id: String, state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let impact = wallet_asset_impact(asset_id.clone(), state.clone())?;
    if !impact.direct_endpoints.is_empty() || !impact.canvases.is_empty() { return Err(format!("该资产仍被 {} 个直出端点和 {} 个 Canvas 引用。", impact.direct_endpoints.len(), impact.canvases.len())); }
    let mut workspace = load_workspace(&state.repository)?; let asset = workspace.wallet.assets.remove(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    if let Some(provider) = workspace.runtime.providers.remove(&asset.provider_instance_id) { for reference in provider.secret_refs.values() { let _ = state.secret_store.delete(reference); } }
    workspace.validate().map_err(|error| error.message)?; state.repository.save(&workspace).map_err(safe_error)?; reload_control_plane(&state).await?; wallet_cards(&state)
}

#[tauri::command]
async fn delete_wallet_asset_secret(asset_id: String, state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let asset = workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    let provider = workspace.runtime.providers.get_mut(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    for reference in provider.secret_refs.values() { let _ = state.secret_store.delete(reference); }
    provider.secret_refs.clear();
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    restart_gateway(&state).await?;
    wallet_cards(&state)
}

#[tauri::command]
async fn reveal_wallet_secret(asset_id: String, app: AppHandle, state: State<'_, DesktopState>) -> Result<SecretRevealResult, String> {
    let window = app.get_webview_window("main").ok_or_else(|| "无法定位主窗口。".to_owned())?;
    #[cfg(windows)]
    let hwnd = window.hwnd().map_err(|_| "无法取得 Windows 主窗口句柄。".to_owned())?.0 as isize;
    #[cfg(not(windows))]
    let hwnd = 0_isize;
    let consent = tauri::async_runtime::spawn_blocking(move || apiarray_windows_security::verify_for_window(hwnd, "查看 API ARRAY 中保存的 API Key")).await.map_err(|_| "Windows Hello 验证任务无法启动。".to_owned())?.map_err(|_| "当前系统不支持 HWND 绑定的 Windows Hello 验证；Windows 10 仅允许替换或删除 Key。".to_owned())?;
    use apiarray_windows_security::ConsentResult;
    match consent {
        ConsentResult::Verified => {}
        ConsentResult::Canceled => return Err("已取消 Windows Hello 验证。".to_owned()),
        ConsentResult::NotConfigured => return Err("当前用户尚未配置 Windows Hello PIN 或生物识别。".to_owned()),
        ConsentResult::DisabledByPolicy => return Err("Windows Hello 已被系统策略禁用。".to_owned()),
        ConsentResult::RetriesExhausted => return Err("Windows Hello 验证重试次数已耗尽。".to_owned()),
        ConsentResult::DeviceUnavailable => return Err("Windows Hello 验证设备当前不可用。".to_owned()),
    }
    let workspace = load_workspace(&state.repository)?;
    let asset = workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    let provider = workspace.runtime.providers.get(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    let reference = provider.secret_refs.values().next().ok_or_else(|| "该资产尚未绑定 Key。".to_owned())?;
    let value = state.secret_store.get(reference).map_err(safe_error)?;
    Ok(SecretRevealResult { value: value.expose().to_owned(), expires_in_ms: 30_000, protection: "windows_hello".to_owned() })
}

#[tauri::command]
async fn probe_wallet_asset(asset_id: String, state: State<'_, DesktopState>) -> Result<apiarray_core::inspection::InspectionReport, String> {
    let workspace = load_workspace(&state.repository)?; let asset = workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?; let instance = workspace.runtime.providers.get(&asset.provider_instance_id).cloned().ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    let resolver = StoreSecretResolver::new(state.secret_store.clone()); let report = state.probe_runner.run(&instance, &resolver, false).await.map_err(safe_error)?; state.inspection_reports.save(&report).map_err(safe_error)?; Ok(report)
}

#[tauri::command]
fn direct_endpoints(state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { direct_endpoint_items(&state) }

fn direct_endpoint_items(state: &DesktopState) -> Result<Vec<DirectEndpointItem>, String> {
    let workspace = load_workspace(&state.repository)?; let audit = state.repository.read_audit(2_000).unwrap_or_default();
    Ok(workspace.direct_endpoints.values().map(|endpoint| DirectEndpointItem { id: endpoint.id.clone(), name: endpoint.name.clone(), alias: endpoint.alias.clone(), asset_id: endpoint.asset_id.clone(), asset_name: workspace.wallet.assets.get(&endpoint.asset_id).map_or_else(|| "Unknown asset".to_owned(), |asset| asset.name.clone()), enabled: endpoint.enabled, token_configured: state.secret_store.contains(&endpoint.token_ref), base_url: direct_endpoint_url(&workspace, &endpoint.alias), public_models: endpoint.models.iter().map(|mapping| mapping.public_model.clone()).collect(), request_count: audit.iter().filter(|trace| trace.publisher_id == format!("direct:{}", endpoint.id)).count() as u64 }).collect())
}

#[tauri::command]
async fn create_direct_endpoint(input: DirectEndpointInput, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> {
    if input.token.trim().is_empty() { return Err("本地直出 Token 不能为空。".to_owned()); }
    let mut workspace = load_workspace(&state.repository)?; if !workspace.wallet.assets.contains_key(&input.asset_id) { return Err("钱包资产不存在。".to_owned()); }
    let base = normalize_id(&input.name, "direct"); let id = unique_map_id(&workspace.direct_endpoints, &base); let token_ref = SecretRef::parse(format!("secret://direct/{id}/token")).map_err(|error| error.message)?;
    let endpoint = DirectEndpoint { id: id.clone(), name: limited_name(&input.name, "Direct endpoint"), alias: normalize_id(&input.alias, "direct"), asset_id: input.asset_id, token_ref: token_ref.clone(), enabled: false, models: vec![DirectModelMapping { public_model: limited_name(&input.public_model, "default"), upstream_model: limited_name(&input.upstream_model, "default") }], timeout_ms: input.timeout_ms.unwrap_or(30_000), max_retries: input.max_retries.unwrap_or(2), audit_tags: BTreeMap::new(), billing_override: input.monthly_budget_micros.map(|budget| BillingPolicy { monthly_budget_micros: Some(budget), currency: input.currency, rules: Vec::new() }) };
    workspace.direct_endpoints.insert(id, endpoint); workspace.validate().map_err(|error| error.message)?; state.secret_store.put(&token_ref, SecretValue::new(input.token)).map_err(safe_error)?; state.repository.save(&workspace).map_err(safe_error)?; restart_gateway(&state).await?; direct_endpoint_items(&state)
}

#[tauri::command]
async fn update_direct_endpoint(input: DirectEndpointInput, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> {
    let id = input.endpoint_id.clone().ok_or_else(|| "缺少直出端点 ID。".to_owned())?; let mut workspace = load_workspace(&state.repository)?;
    let endpoint = workspace.direct_endpoints.get_mut(&id).ok_or_else(|| "直出端点不存在。".to_owned())?; endpoint.name = limited_name(&input.name, "Direct endpoint"); endpoint.alias = normalize_id(&input.alias, "direct"); endpoint.asset_id = input.asset_id; endpoint.models = vec![DirectModelMapping { public_model: limited_name(&input.public_model, "default"), upstream_model: limited_name(&input.upstream_model, "default") }]; endpoint.timeout_ms = input.timeout_ms.unwrap_or(30_000); endpoint.max_retries = input.max_retries.unwrap_or(2);
    if !input.token.trim().is_empty() { state.secret_store.put(&endpoint.token_ref, SecretValue::new(input.token)).map_err(safe_error)?; }
    workspace.validate().map_err(|error| error.message)?; state.repository.save(&workspace).map_err(safe_error)?; restart_gateway(&state).await?; direct_endpoint_items(&state)
}

async fn set_direct_endpoint_enabled(endpoint_id: &str, enabled: bool, state: &DesktopState) -> Result<Vec<DirectEndpointItem>, String> { let mut workspace = load_workspace(&state.repository)?; let endpoint = workspace.direct_endpoints.get_mut(endpoint_id).ok_or_else(|| "直出端点不存在。".to_owned())?; if enabled && !state.secret_store.contains(&endpoint.token_ref) { return Err("直出端点缺少本地 Token。".to_owned()); } endpoint.enabled = enabled; workspace.validate().map_err(|error| error.message)?; state.repository.save(&workspace).map_err(safe_error)?; restart_gateway(state).await?; direct_endpoint_items(state) }

#[tauri::command]
async fn start_direct_endpoint(input: DirectEndpointActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { let result = set_direct_endpoint_enabled(&input.endpoint_id, true, &state).await?; let _ = app.emit("desktop:instances-changed", ()); Ok(result) }
#[tauri::command]
async fn pause_direct_endpoint(input: DirectEndpointActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { let result = set_direct_endpoint_enabled(&input.endpoint_id, false, &state).await?; let _ = app.emit("desktop:instances-changed", ()); Ok(result) }

#[tauri::command]
async fn delete_direct_endpoint(input: DirectEndpointActionInput, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { let mut workspace = load_workspace(&state.repository)?; let endpoint = workspace.direct_endpoints.remove(&input.endpoint_id).ok_or_else(|| "直出端点不存在。".to_owned())?; let _ = state.secret_store.delete(&endpoint.token_ref); state.repository.save(&workspace).map_err(safe_error)?; restart_gateway(&state).await?; direct_endpoint_items(&state) }

#[tauri::command]
fn direct_endpoint_templates(endpoint_id: String, state: State<'_, DesktopState>) -> Result<Vec<CodeTemplate>, String> {
    let workspace = load_workspace(&state.repository)?;
    let endpoint = workspace.direct_endpoints.get(&endpoint_id).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
    generate_templates(&TemplateContext {
        base_url: direct_endpoint_url(&workspace, &endpoint.alias),
        model: endpoint.models.first().map_or_else(|| "default".to_owned(), |mapping| mapping.public_model.clone()),
        stream: false,
        token_placeholder: format!("${{APIARRAY_DIRECT_{}_TOKEN}}", endpoint.alias.to_ascii_uppercase().replace('-', "_")),
    }).map_err(|error| error.message)
}

#[tauri::command]
fn direct_endpoint_live_document(endpoint_id: String, language: TemplateLanguage, state: State<'_, DesktopState>) -> Result<LiveDocument, String> {
    let workspace = load_workspace(&state.repository)?;
    let endpoint = workspace.direct_endpoints.get(&endpoint_id).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
    generate_live_document(&TemplateContext {
        base_url: direct_endpoint_url(&workspace, &endpoint.alias),
        model: endpoint.models.first().map_or_else(|| "default".to_owned(), |mapping| mapping.public_model.clone()),
        stream: false,
        token_placeholder: format!("${{APIARRAY_DIRECT_{}_TOKEN}}", endpoint.alias.to_ascii_uppercase().replace('-', "_")),
    }, language, if endpoint.enabled { "运行中" } else { "已暂停" }).map_err(|error| error.message)
}

#[tauri::command]
async fn test_direct_endpoint(endpoint_id: String, state: State<'_, DesktopState>) -> Result<PublisherConnectionTest, String> {
    let workspace = load_workspace(&state.repository)?; let endpoint = workspace.direct_endpoints.get(&endpoint_id).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
    let token = state.secret_store.get(&endpoint.token_ref).map_err(safe_error)?; let started = Instant::now();
    let response = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|_| "无法创建本地自检客户端。".to_owned())?.get(format!("{}/models", direct_endpoint_url(&workspace, &endpoint.alias))).bearer_auth(token.expose()).send().await.map_err(|_| "无法连接审计直出端点。请确认它已启动。".to_owned())?;
    if !response.status().is_success() { return Err(format!("审计直出端点返回 HTTP {}。", response.status().as_u16())); }
    let payload: Value = response.json().await.map_err(|_| "本地模型列表响应无效。".to_owned())?;
    Ok(PublisherConnectionTest { reachable: true, latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX), model_count: model_count_from_payload(&payload), safe_summary: "本地鉴权与模型列表验证通过。".to_owned() })
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
    let asset_ids = canvas
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            node.config
                .get("asset_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let secret_status =
        workspace.secret_status(&available_secret_refs(&workspace, &state.secret_store));
    let missing_secret_count = asset_ids
        .iter()
        .filter_map(|id| workspace.wallet.assets.get(id))
        .filter_map(|asset| workspace.runtime.providers.get(&asset.provider_instance_id))
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
        asset_ids,
        missing_secret_count,
    })
}

#[tauri::command]
async fn save_canvas_graph(
    input: SaveCanvasGraphInput,
    state: State<'_, DesktopState>,
) -> Result<CanvasSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    validate_canvas_domain_graph(&workspace, &input.graph, false)?;
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
fn compile_canvas_graph(input: CanvasActionInput, state: State<'_, DesktopState>) -> Result<CanvasCompilationReport, String> {
    let workspace = load_workspace(&state.repository)?;
    let canvas = workspace.projects.projects.get(&input.project_id).and_then(|project| project.canvases.get(&input.canvas_id)).ok_or_else(|| "Canvas not found".to_owned())?;
    compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime).map(|compiled| compiled.report).map_err(|error| error.message)
}

fn validate_canvas_domain_graph(workspace: &WorkspacePackage, graph: &WorkflowGraph, require_secrets: bool) -> Result<(), String> {
    graph.validate().map_err(|error| error.message)?;
    let publishers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Publisher).collect::<Vec<_>>();
    let composers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Composer).collect::<Vec<_>>();
    if publishers.len() != 1 { return Err("每个 Canvas 必须且只能包含一个总输出器。".to_owned()); }
    if composers.len() != 1 { return Err("每个 Canvas 必须且只能包含一个 Composer。".to_owned()); }
    if !publishers[0].enabled { return Err("Canvas 总输出器不能停用。".to_owned()); }
    let mut referenced_assets = std::collections::BTreeSet::new();
    for node in graph.nodes.iter().filter(|node| node.kind == NodeKind::Provider) {
        let asset_id = node.config.get("asset_id").and_then(Value::as_str).ok_or_else(|| format!("Adapter {} 未绑定 API 钱包资产。", node.name))?;
        let asset = workspace.wallet.assets.get(asset_id).ok_or_else(|| format!("Adapter {} 引用了不存在的钱包资产。", node.name))?;
        if !referenced_assets.insert(asset_id) { return Err(format!("钱包资产 {} 在当前 Canvas 中被重复添加。", asset.name)); }
        let provider = workspace.runtime.providers.get(&asset.provider_instance_id).ok_or_else(|| format!("钱包资产 {} 缺少 Provider 实例。", asset.name))?;
        if require_secrets && (!asset.enabled || !provider.enabled || provider.secret_refs.is_empty()) { return Err(format!("钱包资产 {} 已停用或缺少 Secret，不能运行。", asset.name)); }
    }
    if referenced_assets.is_empty() { return Err("Composer 至少需要一个钱包资产候选。".to_owned()); }
    let composer_id = &composers[0].id;
    for provider in graph.nodes.iter().filter(|node| node.kind == NodeKind::Provider && node.enabled) {
        if !graph.edges.iter().any(|edge| edge.from.node == provider.id && edge.to.node == *composer_id) { return Err(format!("Provider {} 尚未连接 Composer。", provider.name)); }
    }
    let publisher_id = &publishers[0].id;
    let reachable = graph.edges.iter().any(|edge| edge.to.node == *publisher_id && (edge.from.node == *composer_id || graph.nodes.iter().any(|node| node.id == edge.from.node && node.kind == NodeKind::Middleware)));
    if !referenced_assets.is_empty() && !reachable { return Err("启用的编组路径尚未连接到 Canvas 总输出器。".to_owned()); }
    Ok(())
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
    canvas.graph.nodes.push(Node { id: node_id.clone(), name: asset.name, kind: NodeKind::Provider, enabled: true, inputs: vec![], outputs: vec![Port { id: "candidate_out".to_owned(), data_type: PortType::Candidate }], config: serde_json::json!({"asset_id": asset.id, "upstream_model": "default", "public_model": "default", "priority": 0}) });
    let composer = canvas
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Composer)
        .map(|node| node.id.clone())
        .ok_or_else(|| "Canvas Composer is missing".to_owned())?;
    canvas.graph.edges.push(Edge {
        id: unique_edge_id(&canvas.graph, &format!("{node_id}-to-output")),
        from: Endpoint {
            node: node_id,
            port: "candidate_out".to_owned(),
        },
        to: Endpoint {
            node: composer,
            port: "candidate_in".to_owned(),
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
