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
        let canvases = workspace.projects.projects.values().flat_map(|project| project.canvases.values()).filter(|canvas| canvas.graph.nodes.iter().any(|node| node.provider_config().is_some_and(|config| config.asset_id == asset.id))).count();
        cards.push(WalletCard { id: asset.id.clone(), provider_id: asset.provider_id.clone(), provider_instance_id: Some(asset.provider_instance_id.clone()), name: asset.name.clone(), endpoint_override: provider.and_then(|item| item.endpoint_override.clone()), configured: provider.is_some_and(|item| !item.secret_refs.is_empty()), enabled: asset.enabled && provider.is_some_and(|item| item.enabled), source: "asset".to_owned(), budget_micros: asset.billing.monthly_budget_micros, currency: asset.billing.currency.clone(), request_count: usage.0, input_tokens: usage.1, output_tokens: usage.2, estimated_cost_micros: None, reference_count: direct + canvases });
    }
    Ok(cards)
}

#[tauri::command]
async fn create_wallet_asset(
    input: WalletAssetInput,
    state: State<'_, DesktopState>,
) -> Result<WalletCard, String> {
    let _mutation = state.workspace_mutation.lock().await;
    let manifest = builtin_provider_manifests()
        .map_err(|error| error.message)?
        .into_iter()
        .find(|manifest| manifest.provider.id == input.provider_id)
        .ok_or_else(|| "Provider catalog entry was not found".to_owned())?;
    let workspace = load_workspace(&state.repository)?;
    let base = normalize_id(&input.name, "api");
    let asset_id = unique_map_id(&workspace.wallet.assets, &base);
    let provider_instance_id = unique_map_id(&workspace.runtime.providers, &asset_id);
    let has_key = input.api_key.as_deref().is_some_and(|value| !value.trim().is_empty());
    let mut provider = ProviderInstance {
        id: provider_instance_id.clone(),
        manifest: manifest.clone(),
        endpoint_override: input.endpoint_override,
        secret_refs: BTreeMap::new(),
        enabled: has_key,
    };
    let asset = ApiAsset {
        id: asset_id.clone(),
        provider_instance_id: provider_instance_id.clone(),
        provider_id: manifest.provider.id.clone(),
        name: unique_display_name(
            &workspace.wallet.assets,
            &input.name,
            &manifest.provider.name,
        ),
        enabled: has_key,
        billing: BillingPolicy {
            monthly_budget_micros: input.monthly_budget_micros,
            currency: input.currency,
            rules: Vec::new(),
        },
    };
    let mut created_reference = None;
    if let Some(api_key) = input.api_key.filter(|value| !value.trim().is_empty()) {
        let field = manifest.authentication.fields.iter().find(|field| field.secret && field.required).map(|field| field.id.clone()).unwrap_or_else(|| "api_key".to_owned());
        let reference = SecretRef::parse(format!("secret://wallet/{asset_id}/{field}")).map_err(|error| error.message)?;
        state.secret_store.put(&reference, SecretValue::new(api_key)).map_err(safe_error)?;
        created_reference = Some(reference.clone());
        provider.secret_refs.insert(field, reference);
        provider.enabled = true;
    }
    let asset_for_change = asset.clone();
    let provider_for_change = provider.clone();
    let result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        if workspace.wallet.assets.contains_key(&asset_id) || workspace.runtime.providers.contains_key(&provider_instance_id) {
            return Err("钱包资产 ID 已被占用，请重试。".to_owned());
        }
        workspace.runtime.providers.insert(provider_instance_id, provider_for_change);
        workspace.wallet.assets.insert(asset_id, asset_for_change);
        Ok(())
    }).await;
    if let Err(error) = result {
        if let Some(reference) = created_reference.as_ref() { let _ = state.secret_store.delete(reference); }
        return Err(error);
    }
    Ok(WalletCard {
        id: asset.id,
        provider_id: asset.provider_id,
        provider_instance_id: Some(asset.provider_instance_id.clone()),
        name: asset.name,
        endpoint_override: provider.endpoint_override,
        configured: !provider.secret_refs.is_empty(),
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
    let _mutation = state.workspace_mutation.lock().await;
    let asset_id = input.asset_id.clone();
    let replacement = input.api_key.filter(|value| !value.trim().is_empty());
    let workspace = load_workspace(&state.repository)?;
    let asset = workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    let provider = workspace.runtime.providers.get(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    let field = provider.manifest.authentication.fields.iter().find(|field| field.secret && field.required).map(|field| field.id.clone()).unwrap_or_else(|| "api_key".to_owned());
    let previous_reference = provider.secret_refs.get(&field).cloned();
    let staged_reference = replacement.as_ref().map(|_| staged_secret_ref("wallet", &asset_id, &field)).transpose()?;
    let rotated_secret = staged_reference.is_some();
    if let (Some(value), Some(reference)) = (replacement, staged_reference.as_ref()) {
        state.secret_store.put(reference, SecretValue::new(value)).map_err(safe_error)?;
    }
    let reference_for_change = staged_reference.clone().or(previous_reference.clone());
    let result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        let asset = workspace.wallet.assets.get_mut(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
        asset.name = limited_name(&input.name, "API asset");
        asset.enabled = input.enabled;
        asset.billing.monthly_budget_micros = input.monthly_budget_micros;
        asset.billing.currency = input.currency;
        let provider = workspace.runtime.providers.get_mut(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
        provider.endpoint_override = input.endpoint_override;
        provider.enabled = input.enabled;
        if let Some(reference) = reference_for_change {
            provider.secret_refs.insert(field, reference);
        }
        Ok(())
    }).await;
    if let Err(error) = result {
        if let Some(reference) = staged_reference { let _ = state.secret_store.delete(&reference); }
        return Err(error);
    }
    if rotated_secret && let Some(reference) = previous_reference { let _ = state.secret_store.delete(&reference); }
    wallet_cards(&state)
}

#[tauri::command]
fn wallet_asset_impact(asset_id: String, state: State<'_, DesktopState>) -> Result<WalletAssetImpact, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace.wallet.assets.get(&asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
    Ok(WalletAssetImpact { direct_endpoints: workspace.direct_endpoints.values().filter(|endpoint| endpoint.asset_id == asset_id).map(|endpoint| endpoint.name.clone()).collect(), canvases: workspace.projects.projects.values().flat_map(|project| project.canvases.values()).filter(|canvas| canvas.graph.nodes.iter().any(|node| node.provider_config().is_some_and(|config| config.asset_id == asset_id))).map(|canvas| canvas.name.clone()).collect() })
}

#[tauri::command]
async fn delete_wallet_asset(asset_id: String, state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let _mutation = state.workspace_mutation.lock().await;
    let impact = wallet_asset_impact(asset_id.clone(), state.clone())?;
    if !impact.direct_endpoints.is_empty() || !impact.canvases.is_empty() { return Err(format!("该资产仍被 {} 个直出端点和 {} 个 Canvas 引用。", impact.direct_endpoints.len(), impact.canvases.len())); }
    let target_asset_id = asset_id.clone();
    let ((asset, provider), _) = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        let asset = workspace.wallet.assets.remove(&target_asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
        let provider = workspace.runtime.providers.remove(&asset.provider_instance_id);
        Ok((asset, provider))
    }).await?;
    let secret_refs = provider.as_ref().map(|provider| provider.secret_refs.values().cloned().collect::<Vec<_>>()).unwrap_or_default();
    let secret_backups = secret_refs.iter().filter_map(|reference| state.secret_store.get(reference).ok().map(|value| (reference.clone(), value))).collect::<Vec<_>>();
    for reference in &secret_refs {
        if let Err(error) = state.secret_store.delete(reference) {
            for (reference, value) in secret_backups { let _ = state.secret_store.put(&reference, value); }
            let restore_asset = asset.clone();
            let restore_provider = provider.clone();
            let restore_result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
                workspace.wallet.assets.insert(restore_asset.id.clone(), restore_asset);
                if let Some(provider) = restore_provider { workspace.runtime.providers.insert(provider.id.clone(), provider); }
                Ok(())
            }).await;
            return Err(if restore_result.is_ok() {
                format!("凭据删除失败，钱包资产已恢复：{}", safe_error(error))
            } else {
                "凭据删除失败且钱包资产恢复失败，请检查工作区与凭据库。".to_owned()
            });
        }
    }
    wallet_cards(&state)
}

#[tauri::command]
async fn delete_wallet_asset_secret(asset_id: String, state: State<'_, DesktopState>) -> Result<Vec<WalletCard>, String> {
    let _mutation = state.workspace_mutation.lock().await;
    let target_asset_id = asset_id.clone();
    let ((secret_refs, provider_was_enabled, asset_was_enabled), _) = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        let asset = workspace.wallet.assets.get(&target_asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
        let asset_was_enabled = asset.enabled;
        let provider = workspace.runtime.providers.get_mut(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
        let provider_was_enabled = provider.enabled;
        let refs = provider.secret_refs.clone();
        provider.secret_refs.clear();
        provider.enabled = false;
        if let Some(asset) = workspace.wallet.assets.get_mut(&target_asset_id) { asset.enabled = false; }
        Ok((refs, provider_was_enabled, asset_was_enabled))
    }).await?;
    let secret_backups = secret_refs.values().filter_map(|reference| {
        state.secret_store.get(reference).ok().map(|value| (reference.clone(), value))
    }).collect::<Vec<_>>();
    for reference in secret_refs.values() {
        match state.secret_store.delete(reference) {
            Ok(()) => {}
            Err(error) => {
                for (reference, value) in secret_backups {
                    let _ = state.secret_store.put(&reference, value);
                }
                let restore_refs = secret_refs.clone();
                let restore_asset_id = asset_id.clone();
                let restore_result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
                    let asset = workspace.wallet.assets.get_mut(&restore_asset_id).ok_or_else(|| "API 钱包资产不存在。".to_owned())?;
                    asset.enabled = asset_was_enabled;
                    let provider = workspace.runtime.providers.get_mut(&asset.provider_instance_id).ok_or_else(|| "Provider 实例不存在。".to_owned())?;
                    provider.secret_refs = restore_refs;
                    provider.enabled = provider_was_enabled;
                    Ok(())
                }).await;
                return Err(if restore_result.is_ok() {
                    format!("Key 删除未完成，钱包引用已恢复：{}", safe_error(error))
                } else {
                    "Key 删除未完成且钱包引用恢复失败，请检查工作区与凭据库。".to_owned()
                });
            }
        }
    }
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
    let _mutation = state.workspace_mutation.lock().await;
    if input.token.trim().is_empty() { return Err("本地直出 Token 不能为空。".to_owned()); }
    let workspace = load_workspace(&state.repository)?; if !workspace.wallet.assets.contains_key(&input.asset_id) { return Err("钱包资产不存在。".to_owned()); }
    let base = normalize_id(&input.name, "direct"); let id = unique_map_id(&workspace.direct_endpoints, &base); let token_ref = SecretRef::parse(format!("secret://direct/{id}/token")).map_err(|error| error.message)?;
    let endpoint = DirectEndpoint { id: id.clone(), name: limited_name(&input.name, "Direct endpoint"), alias: normalize_id(&input.alias, "direct"), asset_id: input.asset_id, token_ref: token_ref.clone(), enabled: false, models: vec![DirectModelMapping { public_model: limited_name(&input.public_model, "default"), upstream_model: limited_name(&input.upstream_model, "default") }], timeout_ms: input.timeout_ms.unwrap_or(30_000), max_retries: input.max_retries.unwrap_or(2), audit_tags: BTreeMap::new(), billing_override: input.monthly_budget_micros.map(|budget| BillingPolicy { monthly_budget_micros: Some(budget), currency: input.currency, rules: Vec::new() }) };
    state.secret_store.put(&token_ref, SecretValue::new(input.token)).map_err(safe_error)?;
    let result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        if workspace.direct_endpoints.contains_key(&id) { return Err("直出端点 ID 已存在，请重试。".to_owned()); }
        workspace.direct_endpoints.insert(id, endpoint);
        Ok(())
    }).await;
    if let Err(error) = result {
        let _ = state.secret_store.delete(&token_ref);
        return Err(error);
    }
    direct_endpoint_items(&state)
}

#[tauri::command]
async fn update_direct_endpoint(input: DirectEndpointInput, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> {
    let _mutation = state.workspace_mutation.lock().await;
    let id = input.endpoint_id.clone().ok_or_else(|| "缺少直出端点 ID。".to_owned())?;
    let workspace = load_workspace(&state.repository)?;
    let endpoint = workspace.direct_endpoints.get(&id).ok_or_else(|| "直出端点不存在。".to_owned())?;
    let previous_token_ref = endpoint.token_ref.clone();
    let replacement = (!input.token.trim().is_empty()).then_some(input.token.clone());
    let staged_token_ref = replacement.as_ref().map(|_| staged_secret_ref("direct", &id, "token")).transpose()?;
    let rotated_token = staged_token_ref.is_some();
    if let (Some(value), Some(reference)) = (replacement, staged_token_ref.as_ref()) {
        state.secret_store.put(reference, SecretValue::new(value)).map_err(safe_error)?;
    }
    let token_ref_for_change = staged_token_ref.clone();
    let result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        if !workspace.wallet.assets.contains_key(&input.asset_id) { return Err("钱包资产不存在。".to_owned()); }
        let endpoint = workspace.direct_endpoints.get_mut(&id).ok_or_else(|| "直出端点不存在。".to_owned())?;
        endpoint.name = limited_name(&input.name, "Direct endpoint");
        endpoint.alias = normalize_id(&input.alias, "direct");
        endpoint.asset_id = input.asset_id;
        endpoint.models = vec![DirectModelMapping { public_model: limited_name(&input.public_model, "default"), upstream_model: limited_name(&input.upstream_model, "default") }];
        endpoint.timeout_ms = input.timeout_ms.unwrap_or(30_000);
        endpoint.max_retries = input.max_retries.unwrap_or(2);
        if let Some(reference) = token_ref_for_change { endpoint.token_ref = reference; }
        Ok(())
    }).await;
    if let Err(error) = result {
        if let Some(reference) = staged_token_ref { let _ = state.secret_store.delete(&reference); }
        return Err(error);
    }
    if rotated_token { let _ = state.secret_store.delete(&previous_token_ref); }
    direct_endpoint_items(&state)
}

async fn set_direct_endpoint_enabled(endpoint_id: &str, enabled: bool, state: &DesktopState) -> Result<Vec<DirectEndpointItem>, String> {
    let endpoint_id = endpoint_id.to_owned();
    commit_workspace_change(state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        let endpoint = workspace.direct_endpoints.get_mut(&endpoint_id).ok_or_else(|| "直出端点不存在。".to_owned())?;
        if enabled && !state.secret_store.contains(&endpoint.token_ref) { return Err("直出端点缺少本地 Token。".to_owned()); }
        endpoint.enabled = enabled;
        Ok(())
    }).await?;
    direct_endpoint_items(state)
}

#[tauri::command]
async fn start_direct_endpoint(input: DirectEndpointActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { let result = set_direct_endpoint_enabled(&input.endpoint_id, true, &state).await?; let _ = app.emit("desktop:instances-changed", ()); Ok(result) }
#[tauri::command]
async fn pause_direct_endpoint(input: DirectEndpointActionInput, app: AppHandle, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> { let result = set_direct_endpoint_enabled(&input.endpoint_id, false, &state).await?; let _ = app.emit("desktop:instances-changed", ()); Ok(result) }

#[tauri::command]
async fn delete_direct_endpoint_safe(input: DirectEndpointActionInput, state: State<'_, DesktopState>) -> Result<Vec<DirectEndpointItem>, String> {
    let _mutation = state.workspace_mutation.lock().await;
    let endpoint_id = input.endpoint_id;
    let (endpoint, _) = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
        workspace.direct_endpoints.remove(&endpoint_id).ok_or_else(|| "审计直出端点不存在。".to_owned())
    }).await?;
    if let Err(delete_error) = state.secret_store.delete(&endpoint.token_ref) {
        let restored = endpoint.clone();
        let restore_result = commit_workspace_change_locked(&state, WorkspaceChangeOptions::CONFIGURATION, move |workspace| {
            workspace.direct_endpoints.insert(restored.id.clone(), restored);
            Ok(())
        }).await;
        return Err(if restore_result.is_ok() {
            format!("凭据删除失败，直出端点已恢复：{}", safe_error(delete_error))
        } else {
            "凭据删除失败且端点恢复失败，请检查工作区与 Windows Credential Manager。".to_owned()
        });
    }
    direct_endpoint_items(&state)
}

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
            node.config = NodeConfig::Publisher(PublisherNodeConfig::default());
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
            node.provider_config().map(|config| config.asset_id.clone())
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
        runtime.config.validate().ok()?;
        let state = control
            .as_ref()
            .and_then(|snapshot| snapshot.supervisor.publishers.get(publisher_id));
        Some(CanvasPublisherSnapshot {
            id: publisher_id.clone(),
            status: state.map_or(PublisherLifecycle::Stopped, |item| item.lifecycle),
            message: state.and_then(|item| item.last_error.clone()),
            base_url: canvas_endpoint_url(&workspace, &project.id, &canvas.id),
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
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<CanvasSnapshot, String> {
    let project_id = input.project_id.clone();
    let canvas_id = input.canvas_id.clone();
    commit_workspace_change(&state, WorkspaceChangeOptions::STORAGE_ONLY, move |workspace| {
        validate_canvas_domain_graph(workspace, &input.graph, false)?;
        let canvas = workspace
            .projects
            .projects
            .get_mut(&input.project_id)
            .and_then(|project| project.canvases.get_mut(&input.canvas_id))
            .ok_or_else(|| "编组方案不存在。".to_owned())?;
        if canvas.draft_revision != input.expected_draft_revision {
            return Err("编组方案草稿已在其他位置更新，请刷新后再保存。".to_owned());
        }
        if input.graph.id != canvas.id {
            return Err("编组图 ID 与目标编组方案不一致。".to_owned());
        }
        canvas.graph = input.graph;
        canvas.draft_revision = canvas.draft_revision.saturating_add(1);
        Ok(())
    }).await?;
    let result = canvas_snapshot(
        CanvasActionInput {
            project_id,
            canvas_id: canvas_id.clone(),
        },
        state,
    )
    .await?;
    let _ = app.emit("desktop:canvas-compilation-changed", canvas_id);
    Ok(result)
}

#[tauri::command]
fn graph_node_catalog() -> Vec<apiarray_core::graph::NodeDefinition> {
    node_catalog()
}

#[tauri::command]
async fn apply_canvas_template(
    input: ApplyCanvasTemplateInput,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<CanvasSnapshot, String> {
    let workspace = load_workspace(&state.repository)?;
    let assets = input.asset_ids.iter().map(|asset_id| {
        workspace.wallet.assets.get(asset_id)
            .map(|asset| (asset.id.clone(), asset.name.clone()))
            .ok_or_else(|| format!("钱包资产 {asset_id} 不存在"))
    }).collect::<Result<Vec<_>, _>>()?;
    let graph = generate_graph_template(&input.canvas_id, input.template, &assets)
        .map_err(|error| error.message)?;
    let project_id = input.project_id.clone();
    let canvas_id = input.canvas_id.clone();
    commit_workspace_change(&state, WorkspaceChangeOptions::STORAGE_ONLY, move |workspace| {
        validate_canvas_domain_graph(workspace, &graph, false)?;
        let canvas = workspace.projects.projects.get_mut(&input.project_id)
            .and_then(|project| project.canvases.get_mut(&input.canvas_id))
            .ok_or_else(|| "编组方案不存在。".to_owned())?;
        if canvas.draft_revision != input.expected_draft_revision {
            return Err("编组方案草稿已更新，请刷新后重新应用模板。".to_owned());
        }
        canvas.graph = graph;
        canvas.draft_revision = canvas.draft_revision.saturating_add(1);
        Ok(())
    }).await?;
    let result = canvas_snapshot(CanvasActionInput { project_id, canvas_id: canvas_id.clone() }, state).await?;
    let _ = app.emit("desktop:canvas-compilation-changed", canvas_id);
    Ok(result)
}

#[tauri::command]
fn compile_canvas_graph(input: CanvasActionInput, state: State<'_, DesktopState>) -> Result<CanvasCompilationReport, String> {
    canvas_compilation_report(&state, &input)
}

#[tauri::command]
fn validate_canvas_runtime(input: CanvasActionInput, state: State<'_, DesktopState>) -> Result<CanvasCompilationReport, String> {
    canvas_compilation_report(&state, &input)
}

fn canvas_compilation_report(state: &DesktopState, input: &CanvasActionInput) -> Result<CanvasCompilationReport, String> {
    let workspace = load_workspace(&state.repository)?;
    let mut report = validate_runtime_semantics(&workspace, &input.project_id, &input.canvas_id).map_err(|error| error.message)?;
    let canvas = workspace.projects.projects.get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "编组方案不存在。".to_owned())?;
    for node in canvas.graph.nodes.iter().filter(|node| node.enabled && node.kind == NodeKind::Provider) {
        let Some(config) = node.provider_config() else { continue };
        let Some(asset) = workspace.wallet.assets.get(&config.asset_id) else { continue };
        let Some(provider) = workspace.runtime.providers.get(&asset.provider_instance_id) else { continue };
        if provider.secret_refs.values().any(|reference| !state.secret_store.contains(reference)) {
            if !report.missing_secrets.contains(&config.asset_id) { report.missing_secrets.push(config.asset_id.clone()); }
            report.errors.push(CompilationIssue {
                code: "SECRET_VALUE_UNAVAILABLE".to_owned(),
                severity: IssueSeverity::Error,
                message: format!("钱包资产 {} 的 Secret 引用存在，但 Windows Credential Manager 中没有可用值。", asset.name),
                node_id: Some(node.id.clone()),
                edge_id: None,
                field: Some("asset_id".to_owned()),
                fix_target: Some(format!("wallet:{}", asset.id)),
            });
        }
    }
    report.missing_secrets.sort();
    report.missing_secrets.dedup();
    report.valid = report.errors.is_empty();
    Ok(report)
}

#[tauri::command]
async fn simulate_canvas_route(
    input: SimulateCanvasRouteInput,
    state: State<'_, DesktopState>,
) -> Result<RouteSimulationResult, String> {
    let workspace = load_workspace(&state.repository)?;
    let canvas = workspace.projects.projects.get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "编组方案不存在。".to_owned())?;
    let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime)
        .map_err(|error| error.message)?;
    let health = state.probe_health.lock().await;
    let unavailable_provider_node_ids = health.iter()
        .filter(|(_, endpoint)| matches!(endpoint.status, HealthStatus::Unhealthy | HealthStatus::Paused))
        .filter_map(|(upstream_id, _)| upstream_id.split(':').next().map(ToOwned::to_owned))
        .collect();
    let latency_ms_by_provider_node = health.iter()
        .filter_map(|(upstream_id, endpoint)| {
            Some((upstream_id.split(':').next()?.to_owned(), endpoint.ewma_latency_ms?))
        })
        .collect();
    simulate_route(&compiled, &RouteSimulationInput {
        public_model: input.public_model,
        unavailable_provider_node_ids,
        latency_ms_by_provider_node,
    }).map_err(|error| error.message)
}

async fn execute_canvas_probe(state: &DesktopState, input: &CanvasProbeInput) -> Result<InspectionReport, String> {
    let workspace = load_workspace(&state.repository)?;
    let canvas = workspace.projects.projects.get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "编组方案不存在。".to_owned())?;
    let probe = canvas.graph.nodes.iter().find(|node| node.id == input.probe_node_id && node.enabled)
        .ok_or_else(|| "健康探测节点不存在或已停用。".to_owned())?;
    let NodeConfig::Probe(probe_config) = &probe.config else { return Err("目标节点不是健康探测。".to_owned()); };
    if !probe_config.safe_only { return Err("Graph V3 周期任务只允许无副作用探测。".to_owned()); }
    let provider = canvas.graph.nodes.iter().find(|node| node.id == probe_config.provider_node_id && node.enabled)
        .ok_or_else(|| "健康探测绑定的 Provider 不存在或已停用。".to_owned())?;
    let provider_config = provider.provider_config().ok_or_else(|| "健康探测绑定无效。".to_owned())?;
    let asset = workspace.wallet.assets.get(&provider_config.asset_id).ok_or_else(|| "健康探测绑定的钱包资产不存在。".to_owned())?;
    let instance = workspace.runtime.providers.get(&asset.provider_instance_id).ok_or_else(|| "钱包资产缺少 Provider 实例。".to_owned())?.clone();
    let resolver = StoreSecretResolver::new(state.secret_store.clone());
    let report = state.probe_runner.run(&instance, &resolver, false).await.map_err(safe_error)?;
    state.inspection_reports.save(&report).map_err(safe_error)?;
    let latency_ms = report.findings.values().filter_map(|finding| finding.latency_ms).min();
    let success = matches!(report.overall, InspectionOverall::Healthy | InspectionOverall::Degraded);
    let observation = HealthObservation { success, latency_ms, error: (!success).then_some(StandardError::NetworkUnreachable) };
    let policy = HealthPolicy { schema_version: SCHEMA_VERSION, failure_threshold: probe_config.failure_threshold, recovery_threshold: probe_config.recovery_threshold, degraded_latency_ms: probe_config.timeout_ms };
    let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime).map_err(|error| error.message)?;
    let upstream_ids = compiled.routes.iter().flat_map(|route| route.upstreams.iter()).filter(|upstream| upstream.id.starts_with(&format!("{}:", provider.id))).map(|upstream| upstream.id.clone()).collect::<Vec<_>>();
    let mut registry = state.probe_health.lock().await;
    let mut statuses = Vec::new();
    for upstream_id in upstream_ids {
        let endpoint = registry.entry(upstream_id.clone()).or_insert_with(EndpointHealth::unknown);
        endpoint.observe(&policy, observation).map_err(|error| error.message)?;
        statuses.push((upstream_id, endpoint.status));
    }
    drop(registry);
    if let Some(gateway) = state.gateway.lock().await.as_ref() {
        for (upstream_id, status) in statuses { gateway.set_health(&upstream_id, status).await; }
    }
    let _ = state.app.emit("desktop:provider-health-changed", &input.canvas_id);
    let _ = state.app.emit("desktop:canvas-runtime-changed", &input.canvas_id);
    Ok(report)
}

#[tauri::command]
async fn run_canvas_probe(input: CanvasProbeInput, state: State<'_, DesktopState>) -> Result<InspectionReport, String> {
    execute_canvas_probe(&state, &input).await
}

#[tauri::command]
async fn set_canvas_probe_schedule(input: CanvasProbeScheduleInput, state: State<'_, DesktopState>) -> Result<(), String> {
    configure_canvas_probe_schedule(&state, input, true).await
}

async fn configure_canvas_probe_schedule(state: &DesktopState, input: CanvasProbeScheduleInput, persist: bool) -> Result<(), String> {
    let key = format!("{}:{}:{}", input.project_id, input.canvas_id, input.probe_node_id);
    if let Some(task) = state.probe_tasks.lock().await.remove(&key) { task.abort(); }
    if persist {
        state.repository.write_setting(&format!("graph_v3.probe_schedule.{key}"), if input.enabled { "true" } else { "false" }).map_err(safe_error)?;
    }
    if !input.enabled { return Ok(()); }
    let workspace = load_workspace(&state.repository)?;
    let interval_seconds = workspace.projects.projects.get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .and_then(|canvas| canvas.graph.nodes.iter().find(|node| node.id == input.probe_node_id))
        .and_then(|node| match &node.config { NodeConfig::Probe(config) => Some(config.interval_seconds), _ => None })
        .ok_or_else(|| "健康探测节点不存在。".to_owned())?;
    let app = state.app.clone();
    let probe = CanvasProbeInput { project_id: input.project_id, canvas_id: input.canvas_id, probe_node_id: input.probe_node_id };
    let task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_seconds.max(30)));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let state = app.state::<DesktopState>();
            if state.is_quitting.load(Ordering::SeqCst) { break; }
            let _ = execute_canvas_probe(&state, &probe).await;
        }
    });
    state.probe_tasks.lock().await.insert(key, task);
    Ok(())
}

async fn restore_canvas_probe_schedules(state: &DesktopState) {
    let Ok(workspace) = load_workspace(&state.repository) else { return };
    for (project_id, project) in workspace.projects.projects {
        for (canvas_id, canvas) in project.canvases {
            for node in canvas.graph.nodes.into_iter().filter(|node| node.enabled && node.kind == NodeKind::Probe) {
                let key = format!("{project_id}:{canvas_id}:{}", node.id);
                let enabled = state.repository.read_setting(&format!("graph_v3.probe_schedule.{key}")).ok().flatten().is_some_and(|value| value == "true");
                if enabled {
                    let _ = configure_canvas_probe_schedule(state, CanvasProbeScheduleInput {
                        project_id: project_id.clone(),
                        canvas_id: canvas_id.clone(),
                        probe_node_id: node.id,
                        enabled: true,
                    }, false).await;
                }
            }
        }
    }
}

#[tauri::command]
async fn canvas_runtime_snapshot(input: CanvasActionInput, state: State<'_, DesktopState>) -> Result<CanvasRuntimeSnapshotView, String> {
    let workspace = load_workspace(&state.repository)?;
    let canvas = workspace.projects.projects.get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "编组方案不存在。".to_owned())?;
    let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime).map_err(|error| error.message)?;
    let registry = state.probe_health.lock().await;
    let providers = compiled.routes.iter().flat_map(|route| route.upstreams.iter()).map(|upstream| {
        let health = registry.get(&upstream.id).cloned().unwrap_or_else(EndpointHealth::unknown);
        ProviderRuntimeHealthView {
            provider_node_id: upstream.id.split(':').next().unwrap_or(&upstream.id).to_owned(),
            upstream_id: upstream.id.clone(),
            status: health.status,
            latency_ewma_ms: health.ewma_latency_ms,
            consecutive_failures: health.consecutive_failures,
        }
    }).collect::<Vec<_>>();
    let preferred_candidates = compiled.routes.iter().filter_map(|route| {
        let mut candidates = route.upstreams.iter().filter(|upstream| upstream.enabled).collect::<Vec<_>>();
        candidates.sort_by_key(|upstream| {
            let health = registry.get(&upstream.id).map_or(HealthStatus::Unknown, |item| item.status);
            let health_rank = match health { HealthStatus::Healthy => 0, HealthStatus::Degraded => 1, HealthStatus::Unknown => 2, HealthStatus::Unhealthy => 3, HealthStatus::Paused => 4 };
            let latency = registry.get(&upstream.id).and_then(|item| item.ewma_latency_ms).unwrap_or(u64::MAX);
            (health_rank, if route.policy.selection_strategy == SelectionStrategy::LowestLatency { latency } else { u64::from(upstream.priority) }, upstream.id.clone())
        });
        candidates.first().map(|candidate| (route.public_model.clone(), candidate.id.clone()))
    }).collect();
    let publisher_running = canvas.publisher_id.as_ref().is_some_and(|id| workspace.runtime_state.enabled_publishers.contains(id));
    Ok(CanvasRuntimeSnapshotView {
        project_id: input.project_id,
        canvas_id: input.canvas_id,
        draft_revision: canvas.draft_revision,
        applied_revision: canvas.applied_revision,
        stale_runtime: canvas.draft_revision != canvas.applied_revision,
        publisher_running,
        strategy: compiled.report.selected_strategy,
        providers,
        preferred_candidates,
        middleware: compiled.report.middleware_order,
    })
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
        let asset_id = node.provider_config().map(|config| config.asset_id.as_str()).filter(|value| !value.is_empty()).ok_or_else(|| format!("Provider {} 未绑定 API 钱包资产。", node.name))?;
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
    canvas.graph.nodes.push(Node { id: node_id.clone(), name: asset.name, kind: NodeKind::Provider, enabled: true, inputs: vec![], outputs: vec![Port { id: "candidate_out".to_owned(), data_type: PortType::Candidate }], config: NodeConfig::Provider(ProviderConfig { asset_id: asset.id.clone(), selected_models: vec!["default".to_owned()] }) });
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
            node: node_id.clone(),
            port: "candidate_out".to_owned(),
        },
        to: Endpoint {
            node: composer,
            port: "candidate_in".to_owned(),
        },
        enabled: true,
        label: None,
    });
    if let Some(composer_node) = canvas.graph.nodes.iter_mut().find(|node| node.kind == NodeKind::Composer)
        && let Some(config) = composer_node.composer_config_mut()
    {
        let priority = config.routes.iter().flat_map(|route| &route.candidates).count() as u32;
        let route = config.routes.iter_mut().find(|route| route.public_model == "default");
        let binding = CandidateBinding { provider_node_id: node_id.clone(), upstream_model: "default".to_owned(), priority, weight: 1 };
        if let Some(route) = route { route.candidates.push(binding); } else { config.routes.push(PublicModelRoute { public_model: "default".to_owned(), candidates: vec![binding] }); }
    }
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
                    weight: 1,
                    enabled,
                    conditions: Vec::new(),
                    billing: asset.billing.clone(),
                });
            }
        }
    }
    save_projects_workspace(&state, workspace).await
}
