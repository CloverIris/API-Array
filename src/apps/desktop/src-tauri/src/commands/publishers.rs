#[tauri::command]
async fn create_canvas_publisher(
    input: PublisherInput,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let canvas = workspace
        .projects
        .projects
        .get(&input.project_id)
        .and_then(|project| project.canvases.get(&input.canvas_id))
        .ok_or_else(|| "Canvas not found".to_owned())?;
    if canvas.publisher_id.is_some() {
        return Err("Canvas already has a Publisher".to_owned());
    }
    let compiled = compile_graph(&canvas.graph, &workspace.wallet, &workspace.runtime).map_err(|error| error.message)?;
    if input.token.trim().is_empty() {
        return Err("Publisher Token 不能为空。".to_owned());
    }
    let publisher_id = normalize_id(&input.id, "publisher");
    if workspace.runtime.publishers.contains_key(&publisher_id) {
        return Err("Publisher ID already exists".to_owned());
    }
    let token_ref = SecretRef::parse(format!("secret://publisher/{publisher_id}"))
        .map_err(|error| error.message)?;
    let base_path = input.base_path.unwrap_or_else(|| "/v1".to_owned());
    workspace.runtime.publishers.insert(
        publisher_id.clone(),
        RuntimePublisher {
            config: apiarray_core::publisher::PublisherConfig {
                schema_version: SCHEMA_VERSION,
                id: publisher_id.clone(),
                name: input.name,
                listen_address: "127.0.0.1".parse().map_err(|_| "回环地址无效。")?,
                port: input.port,
                base_path,
                require_token: true,
                token_ref: Some(token_ref.clone()),
            },
            routes: compiled.routes,
        },
    );
    let canvas = workspace
        .projects
        .projects
        .get_mut(&input.project_id)
        .and_then(|project| project.canvases.get_mut(&input.canvas_id))
        .expect("checked");
    canvas.publisher_id = Some(publisher_id.clone());
    canvas.draft_revision = canvas.draft_revision.saturating_add(1);
    if let Some(output) = canvas
        .graph
        .nodes
        .iter_mut()
        .find(|node| node.kind == NodeKind::Publisher)
    {
        output.config = serde_json::json!({"publisher_id": publisher_id, "status": "ready"});
    }
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state
        .secret_store
        .put(&token_ref, SecretValue::new(input.token))
        .map_err(safe_error)?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    snapshot(&state).await
}

#[tauri::command]
async fn delete_publisher(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    let mut workspace = load_workspace(&state.repository)?;
    let removed = workspace
        .runtime
        .publishers
        .remove(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    if let Some(reference) = removed.config.token_ref.as_ref() {
        let _ = state.secret_store.delete(reference);
    }
    workspace
        .runtime_state
        .enabled_publishers
        .remove(&publisher_id);
    workspace.validate().map_err(|error| error.message)?;
    ensure_no_running_publishers(&state).await?;
    state.repository.save(&workspace).map_err(safe_error)?;
    reload_control_plane(&state).await?;
    snapshot(&state).await
}

#[tauri::command]
fn publisher_preview(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<PublisherSummary, String> {
    let workspace = load_workspace(&state.repository)?;
    workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?
        .config
        .validate()
        .map_err(|error| error.message)
}

#[tauri::command]
fn publisher_templates(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<CodeTemplate>, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher = workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    let summary = publisher.config.validate().map_err(|error| error.message)?;
    generate_templates(&TemplateContext {
        base_url: summary.base_url,
        model: publisher
            .routes
            .first()
            .map(|route| route.public_model.clone())
            .unwrap_or_else(|| "default".to_owned()),
        stream: false,
        token_placeholder: "${APIARRAY_PUBLISHER_TOKEN}".to_owned(),
    })
    .map_err(|error| error.message)
}

#[tauri::command]
fn canvas_live_document(publisher_id: String, language: TemplateLanguage, state: State<'_, DesktopState>) -> Result<LiveDocument, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher = workspace.runtime.publishers.get(&publisher_id).ok_or_else(|| "Publisher 不存在。".to_owned())?;
    let summary = publisher.config.validate().map_err(|error| error.message)?;
    let status = if workspace.runtime_state.enabled_publishers.contains(&publisher_id) { "运行中" } else { "已停止" };
    generate_live_document(&TemplateContext { base_url: summary.base_url, model: publisher.routes.first().map(|route| route.public_model.clone()).unwrap_or_else(|| "default".to_owned()), stream: false, token_placeholder: "${APIARRAY_PUBLISHER_TOKEN}".to_owned() }, language, status).map_err(|error| error.message)
}

#[tauri::command]
fn save_live_document_markdown(input: LiveDocumentExportInput, state: State<'_, DesktopState>) -> Result<String, String> {
    let workspace = load_workspace(&state.repository)?;
    let document = match input.subject_type.as_str() {
        "direct" => {
            let endpoint = workspace.direct_endpoints.get(&input.subject_id).ok_or_else(|| "审计直出端点不存在。".to_owned())?;
            generate_live_document(&TemplateContext { base_url: direct_endpoint_url(&workspace, &endpoint.alias), model: endpoint.models.first().map_or_else(|| "default".to_owned(), |mapping| mapping.public_model.clone()), stream: false, token_placeholder: format!("${{APIARRAY_DIRECT_{}_TOKEN}}", endpoint.alias.to_ascii_uppercase().replace('-', "_")) }, input.language, if endpoint.enabled { "运行中" } else { "已暂停" }).map_err(|error| error.message)?
        }
        "canvas" => {
            let publisher = workspace.runtime.publishers.get(&input.subject_id).ok_or_else(|| "Publisher 不存在。".to_owned())?;
            let summary = publisher.config.validate().map_err(|error| error.message)?;
            generate_live_document(&TemplateContext { base_url: summary.base_url, model: publisher.routes.first().map(|route| route.public_model.clone()).unwrap_or_else(|| "default".to_owned()), stream: false, token_placeholder: "${APIARRAY_PUBLISHER_TOKEN}".to_owned() }, input.language, if workspace.runtime_state.enabled_publishers.contains(&input.subject_id) { "运行中" } else { "已停止" }).map_err(|error| error.message)?
        }
        _ => return Err("不支持的活文档主体。".to_owned()),
    };
    let stem = input.filename.trim().trim_end_matches(".md");
    if stem.is_empty() || stem.len() > 80 || !stem.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ' ')) { return Err("Markdown 文件名只能包含字母、数字、空格、短横线和下划线。".to_owned()); }
    let directory = state.repository.root().join("exports");
    std::fs::create_dir_all(&directory).map_err(|_| "无法创建活文档导出目录。".to_owned())?;
    let path = directory.join(format!("{stem}.md"));
    std::fs::write(&path, document.markdown.as_bytes()).map_err(|_| "无法写入 Markdown 文档。".to_owned())?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn save_markdown_document(input: MarkdownContentExportInput, state: State<'_, DesktopState>) -> Result<String, String> {
    if input.markdown.is_empty() || input.markdown.len() > 1_000_000 { return Err("Markdown 文档为空或超过 1 MB。".to_owned()); }
    let stem = input.filename.trim().trim_end_matches(".md");
    if stem.is_empty() || stem.len() > 80 || !stem.chars().all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '·')) { return Err("Markdown 文件名包含不安全字符。".to_owned()); }
    let directory = state.repository.root().join("exports"); std::fs::create_dir_all(&directory).map_err(|_| "无法创建导出目录。".to_owned())?;
    let path = directory.join(format!("{stem}.md")); std::fs::write(&path, input.markdown.as_bytes()).map_err(|_| "无法写入 Markdown 文档。".to_owned())?; Ok(path.display().to_string())
}

#[tauri::command]
async fn test_publisher_connection(
    publisher_id: String,
    state: State<'_, DesktopState>,
) -> Result<PublisherConnectionTest, String> {
    let workspace = load_workspace(&state.repository)?;
    let publisher = workspace
        .runtime
        .publishers
        .get(&publisher_id)
        .ok_or_else(|| "Publisher 不存在。".to_owned())?;
    let summary = publisher.config.validate().map_err(|error| error.message)?;
    let token = match publisher.config.token_ref.as_ref() {
        Some(reference) => Some(state.secret_store.get(reference).map_err(safe_error)?),
        None => None,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "无法创建本地 Publisher 自检客户端。".to_owned())?;
    let started = Instant::now();
    let mut request = client.get(format!("{}/models", summary.base_url.trim_end_matches('/')));
    if let Some(token) = token.as_ref() {
        request = request.bearer_auth(token.expose());
    }
    let response = request
        .send()
        .await
        .map_err(|_| "无法连接本地 Publisher。请确认它已启动且端口未被其他程序占用。".to_owned())?;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !response.status().is_success() {
        return Err(format!(
            "本地 Publisher 返回 HTTP {}。",
            response.status().as_u16()
        ));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|_| "本地 Publisher 返回了无法识别的模型列表。".to_owned())?;
    let model_count = model_count_from_payload(&payload);
    Ok(PublisherConnectionTest {
        reachable: true,
        latency_ms,
        model_count,
        safe_summary: "本地端点可访问，鉴权与 /v1/models 已通过。".to_owned(),
    })
}

fn model_count_from_payload(payload: &Value) -> usize {
    payload
        .get("data")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}
