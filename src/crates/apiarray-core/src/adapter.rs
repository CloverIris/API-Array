use crate::canonical::{
    CanonicalRequest, CanonicalResponse, ContentPart, FinishReason, ImageDetail, ResponseFormat,
    Role, ToolChoice, Usage,
};
use crate::provider::{AuthenticationType, ProviderManifest};
use crate::routing::StandardError;
use crate::secret::{SecretRef, redact_diagnostic};
use crate::{CoreError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterKind {
    OpenaiCompatible,
    AnthropicMessages,
    GoogleGemini,
}

impl AdapterKind {
    /// 将 Provider YAML 中稳定的 Adapter ID 转为实现类型。
    ///
    /// # Errors
    ///
    /// ID 未被当前 Core 实现时返回 [`ErrorCode::AdapterUnsupported`]。
    pub fn parse(id: &str) -> Result<Self, CoreError> {
        match id {
            "openai-compatible" => Ok(Self::OpenaiCompatible),
            "anthropic-messages" => Ok(Self::AnthropicMessages),
            "google-gemini" => Ok(Self::GoogleGemini),
            _ => Err(CoreError::new(
                ErrorCode::AdapterUnsupported,
                format!("尚未实现 Adapter: {id}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportPlan {
    pub adapter: AdapterKind,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<HeaderPlan>,
    #[serde(default)]
    pub query: Vec<QueryPlan>,
    pub body: Value,
    pub stream_protocol: StreamProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderPlan {
    pub name: String,
    pub value: HeaderValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPlan {
    pub name: String,
    pub value: HeaderValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HeaderValue {
    Static {
        value: String,
    },
    SecretRef {
        reference: SecretRef,
        #[serde(default)]
        prefix: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamProtocol {
    None,
    OpenaiSse,
    AnthropicSse,
    GeminiSse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedHttpError {
    pub code: StandardError,
    pub retryable: bool,
    pub failover_recommended: bool,
    pub safe_message: String,
}

/// 根据 Provider 声明和统一请求生成不包含 Secret 明文的 HTTP 计划。
///
/// # Errors
///
/// Provider、请求、Adapter 或 Secret 引用不合法时返回错误。
pub fn build_transport_plan(
    provider: &ProviderManifest,
    secret_refs: &BTreeMap<String, SecretRef>,
    request: &CanonicalRequest,
) -> Result<TransportPlan, CoreError> {
    provider.validate()?;
    request.validate()?;
    let adapter = AdapterKind::parse(&provider.adapter.id)?;
    let mut headers = build_headers(provider, secret_refs)?;
    let query = build_query(provider, secret_refs)?;
    headers.push(HeaderPlan {
        name: "content-type".to_owned(),
        value: HeaderValue::Static {
            value: "application/json".to_owned(),
        },
    });

    let (path, body, stream_protocol) = match adapter {
        AdapterKind::OpenaiCompatible => (
            "/chat/completions",
            openai_request(request)?,
            if request.stream {
                StreamProtocol::OpenaiSse
            } else {
                StreamProtocol::None
            },
        ),
        AdapterKind::AnthropicMessages => (
            "/v1/messages",
            anthropic_request(request),
            if request.stream {
                StreamProtocol::AnthropicSse
            } else {
                StreamProtocol::None
            },
        ),
        AdapterKind::GoogleGemini => {
            let action = if request.stream {
                "streamGenerateContent?alt=sse"
            } else {
                "generateContent"
            };
            let path = format!("/v1beta/models/{}:{action}", request.model);
            return Ok(TransportPlan {
                adapter,
                method: HttpMethod::Post,
                url: join_url(&provider.endpoint.default_base_url, &path),
                headers,
                query,
                body: gemini_request(request)?,
                stream_protocol: if request.stream {
                    StreamProtocol::GeminiSse
                } else {
                    StreamProtocol::None
                },
            });
        }
    };

    Ok(TransportPlan {
        adapter,
        method: HttpMethod::Post,
        url: join_url(&provider.endpoint.default_base_url, path),
        headers,
        query,
        body,
        stream_protocol,
    })
}

/// 将非流式供应商响应归一化。
///
/// # Errors
///
/// 响应缺少协议要求的结构或包含无法解析的工具参数时返回
/// [`ErrorCode::ResponseInvalid`]。
pub fn parse_response(
    adapter: AdapterKind,
    response: &Value,
) -> Result<CanonicalResponse, CoreError> {
    match adapter {
        AdapterKind::OpenaiCompatible => parse_openai_response(response),
        AdapterKind::AnthropicMessages => parse_anthropic_response(response),
        AdapterKind::GoogleGemini => parse_gemini_response(response),
    }
}

#[must_use]
pub fn classify_http_error(status: u16, body: &Value) -> NormalizedHttpError {
    let body_message = find_error_message(body);
    let lower = body_message.to_ascii_lowercase();
    let code = match status {
        401 | 403 => StandardError::AuthFailed,
        402 => StandardError::BalanceExhausted,
        408 | 504 => StandardError::ProviderTimeout,
        429 if lower.contains("balance") || lower.contains("quota") => {
            StandardError::BalanceExhausted
        }
        429 => StandardError::RateLimited,
        404 if lower.contains("model") => StandardError::ModelUnavailable,
        400 | 404 | 405 | 409 | 413 | 422 => StandardError::InvalidRequest,
        _ => StandardError::InvalidResponse,
    };
    let retryable = matches!(
        code,
        StandardError::RateLimited
            | StandardError::ProviderTimeout
            | StandardError::NetworkUnreachable
            | StandardError::InvalidResponse
    );
    let failover_recommended = matches!(
        code,
        StandardError::RateLimited
            | StandardError::BalanceExhausted
            | StandardError::ModelUnavailable
            | StandardError::ProviderTimeout
            | StandardError::InvalidResponse
    );
    let safe_message = redact_diagnostic(&truncate_message(&body_message));
    NormalizedHttpError {
        code,
        retryable,
        failover_recommended,
        safe_message,
    }
}

fn build_headers(
    provider: &ProviderManifest,
    secret_refs: &BTreeMap<String, SecretRef>,
) -> Result<Vec<HeaderPlan>, CoreError> {
    let mut headers: Vec<HeaderPlan> = provider
        .headers
        .iter()
        .map(|header| HeaderPlan {
            name: header.name.clone(),
            value: HeaderValue::Static {
                value: header.value.clone(),
            },
        })
        .collect();
    if provider.authentication.kind == AuthenticationType::None {
        return Ok(headers);
    }
    for field in &provider.authentication.fields {
        if !field.secret {
            continue;
        }
        let reference = secret_refs.get(&field.id).ok_or_else(|| {
            CoreError::new(
                ErrorCode::SecretInvalid,
                format!("缺少鉴权字段 {} 的 Secret 引用", field.id),
            )
        })?;
        if let Some(name) = &field.header {
            headers.push(HeaderPlan {
                name: name.clone(),
                value: HeaderValue::SecretRef {
                    reference: reference.clone(),
                    prefix: field.prefix.clone().unwrap_or_default(),
                },
            });
        }
    }
    Ok(headers)
}

fn build_query(
    provider: &ProviderManifest,
    secret_refs: &BTreeMap<String, SecretRef>,
) -> Result<Vec<QueryPlan>, CoreError> {
    if provider.authentication.kind != AuthenticationType::QuerySecret {
        return Ok(Vec::new());
    }
    provider
        .authentication
        .fields
        .iter()
        .filter(|field| field.secret)
        .map(|field| {
            let reference = secret_refs.get(&field.id).ok_or_else(|| {
                CoreError::new(
                    ErrorCode::SecretInvalid,
                    format!("缺少鉴权字段 {} 的 Secret 引用", field.id),
                )
            })?;
            let name = field.query.clone().ok_or_else(|| {
                CoreError::new(
                    ErrorCode::ProviderInvalid,
                    format!("Query Secret {} 缺少参数名称", field.id),
                )
            })?;
            Ok(QueryPlan {
                name,
                value: HeaderValue::SecretRef {
                    reference: reference.clone(),
                    prefix: field.prefix.clone().unwrap_or_default(),
                },
            })
        })
        .collect()
}

fn openai_request(request: &CanonicalRequest) -> Result<Value, CoreError> {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(openai_message)
        .collect::<Result<_, _>>()?;
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_output_tokens,
        "stream": request.stream,
    });
    insert_optional_number(&mut body, "temperature", request.temperature);
    insert_optional_number(&mut body, "top_p", request.top_p);
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.input_schema,
                        }
                    })
                })
                .collect(),
        );
        body["tool_choice"] = openai_tool_choice(&request.tool_choice);
    }
    match &request.response_format {
        ResponseFormat::Text => {}
        ResponseFormat::JsonObject => body["response_format"] = json!({"type": "json_object"}),
        ResponseFormat::JsonSchema {
            name,
            schema,
            strict,
        } => {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {"name": name, "schema": schema, "strict": strict}
            });
        }
    }
    Ok(body)
}

fn openai_message(message: &crate::canonical::Message) -> Result<Value, CoreError> {
    if message.role == Role::Tool {
        return Ok(json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id,
            "content": collect_text(&message.content),
        }));
    }
    let mut value = json!({"role": role_name(message.role)});
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();
    for part in &message.content {
        match part {
            ContentPart::Text { text } => content.push(json!({"type": "text", "text": text})),
            ContentPart::ImageUrl { url, detail } => content.push(json!({
                "type": "image_url",
                "image_url": {"url": url, "detail": image_detail(*detail)}
            })),
            ContentPart::ToolCall {
                id,
                name,
                arguments,
            } => tool_calls.push(json!({
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": arguments.to_string()}
            })),
            ContentPart::ToolResult { .. } => {
                return Err(CoreError::new(
                    ErrorCode::RequestInvalid,
                    "OpenAI ToolResult 必须使用 Tool 角色消息",
                ));
            }
        }
    }
    if content.len() == 1 && content[0]["type"] == "text" {
        value["content"] = content[0]["text"].clone();
    } else if !content.is_empty() {
        value["content"] = Value::Array(content);
    } else {
        value["content"] = Value::Null;
    }
    if !tool_calls.is_empty() {
        value["tool_calls"] = Value::Array(tool_calls);
    }
    if let Some(name) = &message.name {
        value["name"] = Value::String(name.clone());
    }
    Ok(value)
}

fn anthropic_request(request: &CanonicalRequest) -> Value {
    let system = request
        .messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| collect_text(&message.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let messages = request
        .messages
        .iter()
        .filter(|message| message.role != Role::System)
        .map(anthropic_message)
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_output_tokens,
        "stream": request.stream,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system);
    }
    insert_optional_number(&mut body, "temperature", request.temperature);
    insert_optional_number(&mut body, "top_p", request.top_p);
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.input_schema,
                    })
                })
                .collect(),
        );
        body["tool_choice"] = anthropic_tool_choice(&request.tool_choice);
    }
    body
}

fn anthropic_message(message: &crate::canonical::Message) -> Value {
    let role = if message.role == Role::Assistant {
        "assistant"
    } else {
        "user"
    };
    let mut content = Vec::new();
    for part in &message.content {
        match part {
            ContentPart::Text { text } => content.push(json!({"type": "text", "text": text})),
            ContentPart::ImageUrl { url, .. } => content.push(json!({
                "type": "image",
                "source": {"type": "url", "url": url}
            })),
            ContentPart::ToolCall {
                id,
                name,
                arguments,
            } => content.push(json!({
                "type": "tool_use", "id": id, "name": name, "input": arguments
            })),
            ContentPart::ToolResult {
                tool_call_id,
                content: result,
                is_error,
            } => content.push(json!({
                "type": "tool_result",
                "tool_use_id": tool_call_id,
                "content": result,
                "is_error": is_error,
            })),
        }
    }
    json!({"role": role, "content": content})
}

fn gemini_request(request: &CanonicalRequest) -> Result<Value, CoreError> {
    let mut system_parts = Vec::new();
    for message in request
        .messages
        .iter()
        .filter(|message| message.role == Role::System)
    {
        system_parts.extend(gemini_parts(&message.content)?);
    }
    let contents: Vec<Value> = request
        .messages
        .iter()
        .filter(|message| message.role != Role::System)
        .map(|message| {
            let role = if message.role == Role::Assistant {
                "model"
            } else {
                "user"
            };
            Ok(json!({"role": role, "parts": gemini_parts(&message.content)?}))
        })
        .collect::<Result<_, CoreError>>()?;
    let mut generation = json!({"maxOutputTokens": request.max_output_tokens});
    insert_optional_number(&mut generation, "temperature", request.temperature);
    insert_optional_number(&mut generation, "topP", request.top_p);
    if matches!(
        request.response_format,
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. }
    ) {
        generation["responseMimeType"] = Value::String("application/json".to_owned());
    }
    let mut body = json!({"contents": contents, "generationConfig": generation});
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({"parts": system_parts});
    }
    if !request.tools.is_empty() {
        body["tools"] = json!([{"functionDeclarations": request.tools.iter().map(|tool| {
            json!({"name": tool.name, "description": tool.description, "parameters": tool.input_schema})
        }).collect::<Vec<_>>()}]);
    }
    Ok(body)
}

fn gemini_parts(parts: &[ContentPart]) -> Result<Vec<Value>, CoreError> {
    parts
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => Ok(json!({"text": text})),
            ContentPart::ImageUrl { url, .. } => Ok(json!({"fileData": {"fileUri": url}})),
            ContentPart::ToolCall {
                name, arguments, ..
            } => Ok(json!({"functionCall": {"name": name, "args": arguments}})),
            ContentPart::ToolResult {
                tool_call_id,
                content,
                is_error,
            } => Ok(json!({"functionResponse": {
                "name": tool_call_id,
                "response": {"content": content, "is_error": is_error}
            }})),
        })
        .collect()
}

fn parse_openai_response(response: &Value) -> Result<CanonicalResponse, CoreError> {
    let message = response
        .pointer("/choices/0/message")
        .ok_or_else(|| invalid_response("OpenAI 响应缺少 choices[0].message"))?;
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        content.push(ContentPart::Text {
            text: text.to_owned(),
        });
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            content.push(ContentPart::ToolCall {
                id: string_at(call, "/id")?,
                name: string_at(call, "/function/name")?,
                arguments,
            });
        }
    }
    Ok(CanonicalResponse {
        schema_version: crate::SCHEMA_VERSION,
        provider_response_id: optional_string(response, "/id"),
        model: optional_string(response, "/model"),
        content,
        finish_reason: finish_reason(
            response
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str),
        ),
        usage: Usage {
            input_tokens: unsigned_at(response, "/usage/prompt_tokens"),
            output_tokens: unsigned_at(response, "/usage/completion_tokens"),
            cached_input_tokens: response
                .pointer("/usage/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
        },
    })
}

fn parse_anthropic_response(response: &Value) -> Result<CanonicalResponse, CoreError> {
    let blocks = response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Anthropic 响应缺少 content"))?;
    let mut content = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => content.push(ContentPart::Text {
                text: string_at(block, "/text")?,
            }),
            Some("tool_use") => content.push(ContentPart::ToolCall {
                id: string_at(block, "/id")?,
                name: string_at(block, "/name")?,
                arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
            }),
            _ => {}
        }
    }
    Ok(CanonicalResponse {
        schema_version: crate::SCHEMA_VERSION,
        provider_response_id: optional_string(response, "/id"),
        model: optional_string(response, "/model"),
        content,
        finish_reason: finish_reason(response.get("stop_reason").and_then(Value::as_str)),
        usage: Usage {
            input_tokens: unsigned_at(response, "/usage/input_tokens"),
            output_tokens: unsigned_at(response, "/usage/output_tokens"),
            cached_input_tokens: response
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64),
        },
    })
}

fn parse_gemini_response(response: &Value) -> Result<CanonicalResponse, CoreError> {
    let parts = response
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Gemini 响应缺少 candidates[0].content.parts"))?;
    let mut content = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            content.push(ContentPart::Text {
                text: text.to_owned(),
            });
        }
        if let Some(call) = part.get("functionCall") {
            content.push(ContentPart::ToolCall {
                id: format!("gemini-call-{index}"),
                name: string_at(call, "/name")?,
                arguments: call.get("args").cloned().unwrap_or_else(|| json!({})),
            });
        }
    }
    Ok(CanonicalResponse {
        schema_version: crate::SCHEMA_VERSION,
        provider_response_id: optional_string(response, "/responseId"),
        model: optional_string(response, "/modelVersion"),
        content,
        finish_reason: finish_reason(
            response
                .pointer("/candidates/0/finishReason")
                .and_then(Value::as_str),
        ),
        usage: Usage {
            input_tokens: unsigned_at(response, "/usageMetadata/promptTokenCount"),
            output_tokens: unsigned_at(response, "/usageMetadata/candidatesTokenCount"),
            cached_input_tokens: response
                .pointer("/usageMetadata/cachedContentTokenCount")
                .and_then(Value::as_u64),
        },
    })
}

fn openai_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Named(name) => json!({"type": "function", "function": {"name": name}}),
    }
}

fn anthropic_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!({"type": "auto"}),
        ToolChoice::None => json!({"type": "none"}),
        ToolChoice::Required => json!({"type": "any"}),
        ToolChoice::Named(name) => json!({"type": "tool", "name": name}),
    }
}

fn collect_text(parts: &[ContentPart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => Some(text.as_str()),
            ContentPart::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

const fn image_detail(detail: ImageDetail) -> &'static str {
    match detail {
        ImageDetail::Low => "low",
        ImageDetail::High => "high",
        ImageDetail::Auto => "auto",
    }
}

fn insert_optional_number(object: &mut Value, key: &str, value: Option<f32>) {
    if let Some(value) = value.and_then(|number| serde_json::Number::from_f64(f64::from(number))) {
        object[key] = Value::Number(value);
    }
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn string_at(value: &Value, pointer: &str) -> Result<String, CoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid_response(format!("响应缺少字符串字段 {pointer}")))
}

fn optional_string(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn unsigned_at(value: &Value, pointer: &str) -> u64 {
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

fn finish_reason(reason: Option<&str>) -> FinishReason {
    match reason.map(str::to_ascii_lowercase).as_deref() {
        Some("stop" | "end_turn") => FinishReason::Stop,
        Some("length" | "max_tokens" | "max_tokens_reached") => FinishReason::Length,
        Some("tool_calls" | "tool_use") => FinishReason::ToolCall,
        Some("content_filter" | "safety") => FinishReason::ContentFilter,
        Some("error") => FinishReason::Error,
        _ => FinishReason::Unknown,
    }
}

fn invalid_response(message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorCode::ResponseInvalid, message)
}

fn find_error_message(body: &Value) -> String {
    for pointer in ["/error/message", "/error/status", "/message", "/detail"] {
        if let Some(message) = body.pointer(pointer).and_then(Value::as_str) {
            return message.to_owned();
        }
    }
    "上游返回未识别错误".to_owned()
}

fn truncate_message(message: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut chars = message.chars();
    let shortened: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{Message, ToolDefinition};

    fn request() -> CanonicalRequest {
        CanonicalRequest {
            schema_version: 1,
            model: "model-a".to_owned(),
            messages: vec![
                Message::text(Role::System, "Be concise"),
                Message::text(Role::User, "Hello"),
            ],
            max_output_tokens: 128,
            temperature: Some(0.2),
            top_p: None,
            stream: true,
            tools: vec![ToolDefinition {
                name: "weather".to_owned(),
                description: "Get weather".to_owned(),
                input_schema: json!({"type": "object"}),
            }],
            tool_choice: ToolChoice::Auto,
            response_format: ResponseFormat::Text,
            metadata: BTreeMap::new(),
        }
    }

    fn provider(adapter: &str, base: &str) -> ProviderManifest {
        ProviderManifest::from_yaml(&format!(
            r#"
schema_version: 1
provider: {{ id: test, name: Test, category: ai-model }}
adapter: {{ id: {adapter}, min_version: 1 }}
endpoint: {{ default_base_url: {base}, allowed_schemes: [https] }}
authentication:
  type: header-secret
  fields:
    - {{ id: api_key, label: API Key, secret: true, required: true, header: Authorization, prefix: "Bearer " }}
"#
        ))
        .expect("valid provider")
    }

    fn refs() -> BTreeMap<String, SecretRef> {
        BTreeMap::from([(
            "api_key".to_owned(),
            SecretRef::parse("secret://test/key").expect("valid ref"),
        )])
    }

    #[test]
    fn openai_plan_contains_reference_not_secret() -> Result<(), CoreError> {
        let plan = build_transport_plan(
            &provider("openai-compatible", "https://example.invalid/v1"),
            &refs(),
            &request(),
        )?;
        assert_eq!(plan.url, "https://example.invalid/v1/chat/completions");
        assert_eq!(plan.stream_protocol, StreamProtocol::OpenaiSse);
        let json = serde_json::to_string(&plan)?;
        assert!(json.contains("secret://test/key"));
        assert!(!json.contains("Bearer secret"));
        Ok(())
    }

    #[test]
    fn translates_anthropic_system_and_tools() -> Result<(), CoreError> {
        let plan = build_transport_plan(
            &provider("anthropic-messages", "https://example.invalid"),
            &refs(),
            &request(),
        )?;
        assert_eq!(plan.body["system"], "Be concise");
        assert_eq!(plan.body["tools"][0]["name"], "weather");
        Ok(())
    }

    #[test]
    fn translates_gemini_path_and_system_instruction() -> Result<(), CoreError> {
        let plan = build_transport_plan(
            &provider("google-gemini", "https://example.invalid"),
            &refs(),
            &request(),
        )?;
        assert!(plan.url.ends_with("model-a:streamGenerateContent?alt=sse"));
        assert_eq!(
            plan.body["systemInstruction"]["parts"][0]["text"],
            "Be concise"
        );
        Ok(())
    }

    #[test]
    fn parses_openai_response() -> Result<(), CoreError> {
        let response = parse_response(
            AdapterKind::OpenaiCompatible,
            &json!({
                "id": "r1", "model": "m1",
                "choices": [{"message": {"content": "hello"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 3, "completion_tokens": 2}
            }),
        )?;
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.usage.input_tokens, 3);
        Ok(())
    }

    #[test]
    fn classifies_rate_limit_and_sanitizes_message() {
        let error = classify_http_error(429, &json!({"error": {"message": "too many requests"}}));
        assert_eq!(error.code, StandardError::RateLimited);
        assert!(error.retryable);
    }

    #[test]
    fn query_auth_is_kept_as_unresolved_secret_reference() -> Result<(), CoreError> {
        let provider = ProviderManifest::from_yaml(
            r"
schema_version: 1
provider: { id: query-auth, name: Query Auth, category: ai-model }
adapter: { id: openai-compatible, min_version: 1 }
endpoint: { default_base_url: https://example.invalid/v1, allowed_schemes: [https] }
authentication:
  type: query-secret
  fields:
    - { id: api_key, label: API Key, secret: true, required: true, query: key }
",
        )?;
        let plan = build_transport_plan(&provider, &refs(), &request())?;
        assert!(plan.headers.iter().all(|header| header.name != "key"));
        assert_eq!(plan.query.len(), 1);
        assert_eq!(plan.query[0].name, "key");
        assert!(matches!(plan.query[0].value, HeaderValue::SecretRef { .. }));
        Ok(())
    }
}
