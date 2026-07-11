#[tauri::command]
fn provider_instances(state: State<'_, DesktopState>) -> Result<Vec<ProviderInstanceItem>, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .runtime
        .providers
        .values()
        .map(|instance| {
            Ok(ProviderInstanceItem {
                id: instance.id.clone(),
                provider_id: instance.manifest.provider.id.clone(),
                name: instance.manifest.provider.name.clone(),
                endpoint_override: instance.endpoint_override.clone(),
                enabled: instance.enabled,
                secret_fields: instance.secret_refs.keys().cloned().collect(),
                inspection_available: state
                    .inspection_reports
                    .load(&instance.id)
                    .map_err(safe_error)?
                    .is_some(),
            })
        })
        .collect()
}

#[tauri::command]
fn validate_provider_yaml(input: ProviderYamlInput) -> ProviderYamlValidation {
    match apiarray_core::provider::ProviderManifest::from_yaml(&input.yaml) {
        Ok(manifest) => ProviderYamlValidation {
            valid: true,
            provider_id: Some(manifest.provider.id),
            name: Some(manifest.provider.name),
            adapter: Some(manifest.adapter.id),
            error: None,
        },
        Err(error) => ProviderYamlValidation {
            valid: false,
            provider_id: None,
            name: None,
            adapter: None,
            error: Some(error.message),
        },
    }
}

#[tauri::command]
async fn upsert_custom_provider_yaml(
    input: CustomProviderInput,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let manifest = apiarray_core::provider::ProviderManifest::from_yaml(&input.yaml)
        .map_err(|error| error.message)?;
    let instance_id = normalize_id(&input.instance_id, "custom-provider");
    let mut secret_refs = BTreeMap::new();
    for (field, reference) in input.secret_fields {
        secret_refs.insert(
            field,
            SecretRef::parse(reference).map_err(|error| error.message)?,
        );
    }
    let mut workspace = load_workspace(&state.repository)?;
    workspace.runtime.providers.insert(
        instance_id.clone(),
        ProviderInstance {
            id: instance_id.clone(),
            manifest,
            endpoint_override: input.endpoint_override,
            secret_refs,
            enabled: true,
        },
    );
    ensure_wallet_asset(&mut workspace, &instance_id);
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
async fn upsert_provider_instance(
    input: ProviderInstanceInput,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let manifest = builtin_provider_manifests()
        .map_err(|error| error.message)?
        .into_iter()
        .find(|manifest| manifest.provider.id == input.provider_id)
        .ok_or_else(|| "未找到内置 Provider；自定义 YAML 需要先通过校验。".to_owned())?;
    let instance_id = normalize_id(&input.instance_id, "provider");
    let mut secret_refs = BTreeMap::new();
    for (field, reference) in input.secret_fields {
        let reference = SecretRef::parse(reference).map_err(|error| error.message)?;
        secret_refs.insert(field, reference);
    }
    workspace.runtime.providers.insert(
        instance_id.clone(),
        ProviderInstance {
            id: instance_id.clone(),
            manifest,
            endpoint_override: input.endpoint_override,
            secret_refs,
            enabled: input.enabled.unwrap_or(true),
        },
    );
    ensure_wallet_asset(&mut workspace, &instance_id);
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
async fn delete_provider_instance(
    provider_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProviderInstanceItem>, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let removed = workspace
        .runtime
        .providers
        .remove(&provider_id)
        .ok_or_else(|| "Provider 实例不存在。".to_owned())?;
    workspace
        .wallet
        .assets
        .retain(|_, asset| asset.provider_instance_id != provider_id);
    for reference in removed.secret_refs.values() {
        let _ = state.secret_store.delete(reference);
    }
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    provider_instances(state)
}

#[tauri::command]
fn store_secret(input: SecretInput, state: State<'_, DesktopState>) -> Result<(), String> {
    let reference = SecretRef::parse(input.reference).map_err(|error| error.message)?;
    state
        .secret_store
        .put(&reference, SecretValue::new(input.value))
        .map_err(safe_error)
}

#[tauri::command]
fn delete_secret(reference: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let reference = SecretRef::parse(reference).map_err(|error| error.message)?;
    state.secret_store.delete(&reference).map_err(safe_error)
}

#[tauri::command]
async fn run_provider_probe(
    input: ProbeInput,
    state: State<'_, DesktopState>,
) -> Result<apiarray_core::inspection::InspectionReport, String> {
    if state
        .paused_probes
        .lock()
        .await
        .contains(&input.provider_id)
    {
        return Err("该 Provider 的体检已暂停。".to_owned());
    }
    let workspace = load_workspace(&state.repository)?;
    let instance = workspace
        .runtime
        .providers
        .get(&input.provider_id)
        .ok_or_else(|| "Provider 实例不存在。".to_owned())?
        .clone();
    let resolver = StoreSecretResolver::new(state.secret_store.clone());
    let report = state
        .probe_runner
        .run(&instance, &resolver, input.allow_billable)
        .await
        .map_err(safe_error)?;
    state.inspection_reports.save(&report).map_err(safe_error)?;
    Ok(report)
}

#[tauri::command]
fn inspection_report(
    provider_id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<apiarray_core::inspection::InspectionReport>, String> {
    state
        .inspection_reports
        .load(&provider_id)
        .map_err(safe_error)
}

#[tauri::command]
async fn pause_provider_probe(
    input: ProbePauseInput,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let mut paused = state.paused_probes.lock().await;
    if input.paused {
        paused.insert(input.provider_id);
    } else {
        paused.remove(&input.provider_id);
    }
    Ok(())
}
