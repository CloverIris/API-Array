use crate::canonical::{
    CanonicalRequest, CanonicalResponse, ContentPart, FinishReason, ImageDetail, Message,
    ResponseFormat, Role, ToolChoice, ToolDefinition,
};
use crate::stream::StreamEvent;
use crate::{CoreError, ErrorCode, SCHEMA_VERSION};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// 将 OpenAI-compatible Chat Completions 请求转换为内部统一请求。
///
/// # Errors
///
/// 请求缺少模型、消息或包含不支持的内容结构时返回 [`ErrorCode::RequestInvalid`]。
pub fn parse_chat_completions_request(value: &Value) -> Result<CanonicalRequest, CoreError> {
    let model = required_string(value, "/model")?;
    let raw_messages = value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| request_error("OpenAI 请求缺少 messages"))?;
    let messages = raw_messages
        .iter()
        .map(parse_message)
        .collect::<Result<Vec<_>, _>>()?;
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| tools.iter().map(parse_tool).collect())
        .transpose()?
        .unwrap_or_default();
    let request = CanonicalRequest {
        schema_version: SCHEMA_VERSION,
        model,
        messages,
        max_output_tokens: value
            .get("max_completion_tokens")
            .or_else(|| value.get("max_tokens"))
            .and_then(Value::as_u64)
            .and_then(|tokens| u32::try_from(tokens).ok())
            .unwrap_or(1024),
        temperature: value
            .get("temperature")
            .map(|temperature| {
                serde_json::from_value::<f32>(temperature.clone())
                    .map_err(|error| request_error(format!("temperature 无效: {error}")))
            })
            .transpose()?,
        top_p: value
            .get("top_p")
            .map(|top_p| {
                serde_json::from_value::<f32>(top_p.clone())
                    .map_err(|error| request_error(format!("top_p 无效: {error}")))
            })
            .transpose()?,
        stream: value
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        tools,
        tool_choice: parse_tool_choice(value.get("tool_choice"))?,
        response_format: parse_response_format(value.get("response_format"))?,
        metadata: parse_metadata(value.get("metadata"))?,
    };
    request.validate()?;
    Ok(request)
}

fn parse_metadata(value: Option<&Value>) -> Result<BTreeMap<String, String>, CoreError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| request_error("metadata 必须是字符串键值对象"))?;
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.clone(), value.to_owned()))
                .ok_or_else(|| request_error("metadata 的值必须是字符串"))
        })
        .collect()
}

/// 将统一非流式响应编码为 OpenAI-compatible Chat Completions 响应。
#[must_use]
pub fn encode_chat_completions_response(response: &CanonicalResponse) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": collect_response_text(&response.content),
    });
    let tool_calls = response
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::ToolCall {
                id,
                name,
                arguments,
            } => Some(json!({
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": arguments.to_string()}
            })),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    json!({
        "id": response.provider_response_id.clone().unwrap_or_else(|| "apiarray-response".to_owned()),
        "object": "chat.completion",
        "model": response.model.clone().unwrap_or_else(|| "unknown".to_owned()),
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason_name(response.finish_reason),
        }],
        "usage": {
            "prompt_tokens": response.usage.input_tokens,
            "completion_tokens": response.usage.output_tokens,
            "total_tokens": response.usage.input_tokens.saturating_add(response.usage.output_tokens),
            "prompt_tokens_details": {"cached_tokens": response.usage.cached_input_tokens.unwrap_or(0)},
        }
    })
}

#[derive(Debug, Clone)]
pub struct OpenAiStreamEncoder {
    response_id: String,
    model: String,
}

impl OpenAiStreamEncoder {
    #[must_use]
    pub fn new(response_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            model: model.into(),
        }
    }

    /// 编码一个统一流事件。返回的每个字符串都是完整 SSE 帧。
    #[must_use]
    pub fn encode(&mut self, event: &StreamEvent) -> Vec<String> {
        match event {
            StreamEvent::Started { response_id, model } => {
                if let Some(id) = response_id {
                    self.response_id.clone_from(id);
                }
                if let Some(model) = model {
                    self.model.clone_from(model);
                }
                vec![self.chunk(&json!({"role": "assistant"}), &Value::Null, None)]
            }
            StreamEvent::TextDelta { text } => {
                vec![self.chunk(&json!({"content": text}), &Value::Null, None)]
            }
            StreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => vec![self.chunk(
                &json!({"tool_calls": [{
                    "index": index,
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments_delta}
                }]}),
                &Value::Null,
                None,
            )],
            StreamEvent::Usage { usage } => vec![self.chunk(
                &json!({}),
                &Value::Null,
                Some(&json!({
                    "prompt_tokens": usage.input_tokens,
                    "completion_tokens": usage.output_tokens,
                    "total_tokens": usage.input_tokens.saturating_add(usage.output_tokens),
                })),
            )],
            StreamEvent::Finished { reason } => vec![
                self.chunk(
                    &json!({}),
                    &Value::String(finish_reason_name(*reason).to_owned()),
                    None,
                ),
                "data: [DONE]\n\n".to_owned(),
            ],
        }
    }

    fn chunk(&self, delta: &Value, finish_reason: &Value, usage: Option<&Value>) -> String {
        let mut chunk = json!({
            "id": self.response_id,
            "object": "chat.completion.chunk",
            "model": self.model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}],
        });
        if let Some(usage) = usage {
            chunk["usage"] = usage.clone();
        }
        format!("data: {chunk}\n\n")
    }
}

fn parse_message(value: &Value) -> Result<Message, CoreError> {
    let role = match required_string(value, "/role")?.as_str() {
        "system" | "developer" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => return Err(request_error(format!("不支持 OpenAI 消息角色: {other}"))),
    };
    let mut content = parse_content(value.get("content"))?;
    if let Some(calls) = value.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let arguments = required_string(call, "/function/arguments")?;
            content.push(ContentPart::ToolCall {
                id: required_string(call, "/id")?,
                name: required_string(call, "/function/name")?,
                arguments: serde_json::from_str(&arguments).map_err(|error| {
                    request_error(format!("Tool arguments 不是有效 JSON: {error}"))
                })?,
            });
        }
    }
    Ok(Message {
        role,
        content,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        tool_call_id: value
            .get("tool_call_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    })
}

fn parse_content(value: Option<&Value>) -> Result<Vec<ContentPart>, CoreError> {
    match value {
        Some(Value::String(text)) => Ok(vec![ContentPart::Text { text: text.clone() }]),
        Some(Value::Array(parts)) => parts.iter().map(parse_content_part).collect(),
        Some(Value::Null) | None => Ok(Vec::new()),
        _ => Err(request_error("OpenAI message.content 类型无效")),
    }
}

fn parse_content_part(value: &Value) -> Result<ContentPart, CoreError> {
    match value.get("type").and_then(Value::as_str) {
        Some("text" | "input_text") => Ok(ContentPart::Text {
            text: required_string(value, "/text")?,
        }),
        Some("image_url") => {
            let image = value
                .get("image_url")
                .ok_or_else(|| request_error("缺少 image_url"))?;
            let (url, detail) = match image {
                Value::String(url) => (url.clone(), ImageDetail::Auto),
                Value::Object(_) => (
                    required_string(image, "/url")?,
                    match image.get("detail").and_then(Value::as_str) {
                        Some("low") => ImageDetail::Low,
                        Some("high") => ImageDetail::High,
                        _ => ImageDetail::Auto,
                    },
                ),
                _ => return Err(request_error("image_url 类型无效")),
            };
            Ok(ContentPart::ImageUrl { url, detail })
        }
        other => Err(request_error(format!("不支持的内容类型: {other:?}"))),
    }
}

fn parse_tool(value: &Value) -> Result<ToolDefinition, CoreError> {
    let function = value
        .get("function")
        .ok_or_else(|| request_error("工具缺少 function"))?;
    Ok(ToolDefinition {
        name: required_string(function, "/name")?,
        description: function
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        input_schema: function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"})),
    })
}

fn parse_tool_choice(value: Option<&Value>) -> Result<ToolChoice, CoreError> {
    match value {
        None => Ok(ToolChoice::Auto),
        Some(Value::String(choice)) if choice == "auto" => Ok(ToolChoice::Auto),
        Some(Value::String(choice)) if choice == "none" => Ok(ToolChoice::None),
        Some(Value::String(choice)) if choice == "required" => Ok(ToolChoice::Required),
        Some(object @ Value::Object(_)) => Ok(ToolChoice::Named(required_string(
            object,
            "/function/name",
        )?)),
        Some(other) => Err(request_error(format!("无效 tool_choice: {other}"))),
    }
}

fn parse_response_format(value: Option<&Value>) -> Result<ResponseFormat, CoreError> {
    match value
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
    {
        None | Some("text") => Ok(ResponseFormat::Text),
        Some("json_object") => Ok(ResponseFormat::JsonObject),
        Some("json_schema") => {
            let definition = value
                .and_then(|value| value.get("json_schema"))
                .ok_or_else(|| request_error("response_format 缺少 json_schema"))?;
            Ok(ResponseFormat::JsonSchema {
                name: required_string(definition, "/name")?,
                schema: definition
                    .get("schema")
                    .cloned()
                    .ok_or_else(|| request_error("json_schema 缺少 schema"))?,
                strict: definition
                    .get("strict")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        Some(other) => Err(request_error(format!("不支持 response_format: {other}"))),
    }
}

fn collect_response_text(parts: &[ContentPart]) -> Value {
    let text = parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() {
        Value::Null
    } else {
        Value::String(text)
    }
}

fn finish_reason_name(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop | FinishReason::Error | FinishReason::Unknown => "stop",
        FinishReason::Length => "length",
        FinishReason::ToolCall => "tool_calls",
        FinishReason::ContentFilter => "content_filter",
    }
}

fn required_string(value: &Value, pointer: &str) -> Result<String, CoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| request_error(format!("缺少字符串字段 {pointer}")))
}

fn request_error(message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorCode::RequestInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::Usage;

    #[test]
    fn parses_openai_request_with_tool_and_image() -> Result<(), CoreError> {
        let request = parse_chat_completions_request(&json!({
            "model": "smart",
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "https://example.invalid/a.png", "detail": "low"}}
            ]}],
            "tools": [{"type": "function", "function": {
                "name": "weather", "description": "weather", "parameters": {"type": "object"}
            }}],
            "stream": true,
            "metadata": {"tier": "premium"}
        }))?;
        assert_eq!(request.model, "smart");
        assert_eq!(request.messages[0].content.len(), 2);
        assert_eq!(request.tools.len(), 1);
        assert!(request.stream);
        assert_eq!(
            request.metadata.get("tier").map(String::as_str),
            Some("premium")
        );
        Ok(())
    }

    #[test]
    fn encodes_response_and_stream() {
        let response = CanonicalResponse {
            schema_version: 1,
            provider_response_id: Some("r1".to_owned()),
            model: Some("smart".to_owned()),
            content: vec![ContentPart::Text {
                text: "hello".to_owned(),
            }],
            finish_reason: FinishReason::Stop,
            usage: Usage {
                input_tokens: 2,
                output_tokens: 1,
                cached_input_tokens: Some(1),
            },
        };
        let encoded = encode_chat_completions_response(&response);
        assert_eq!(encoded["choices"][0]["message"]["content"], "hello");

        let mut stream = OpenAiStreamEncoder::new("r1", "smart");
        let frames = stream.encode(&StreamEvent::Finished {
            reason: FinishReason::Stop,
        });
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1], "data: [DONE]\n\n");
    }
}
