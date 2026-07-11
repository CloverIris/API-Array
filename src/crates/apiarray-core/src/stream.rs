use crate::adapter::AdapterKind;
use crate::canonical::{FinishReason, Usage};
use crate::{CoreError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SseFrame {
    #[serde(default)]
    pub event: Option<String>,
    pub data: String,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Started {
        #[serde(default)]
        response_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    TextDelta {
        text: String,
    },
    ToolCallDelta {
        #[serde(default)]
        index: usize,
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        arguments_delta: String,
    },
    Usage {
        usage: Usage,
    },
    Finished {
        reason: FinishReason,
    },
}

impl SseDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 输入一段 UTF-8 SSE 文本，并返回目前已完整接收的帧。
    ///
    /// 未完成的尾帧保留在 Decoder 中等待下一个 chunk。
    pub fn push(&mut self, chunk: &str) -> Vec<SseFrame> {
        self.buffer.push_str(chunk);
        if self.buffer.contains("\r\n") {
            self.buffer = self.buffer.replace("\r\n", "\n");
        }
        let mut frames = Vec::new();
        while let Some(boundary) = self.buffer.find("\n\n") {
            let block = self.buffer[..boundary].to_owned();
            self.buffer.drain(..boundary + 2);
            if let Some(frame) = parse_frame(&block) {
                frames.push(frame);
            }
        }
        frames
    }

    /// 在输入结束时提取最后一个没有空行终止的帧。
    #[must_use]
    pub fn finish(&mut self) -> Option<SseFrame> {
        let remaining = std::mem::take(&mut self.buffer);
        parse_frame(remaining.trim_end_matches(['\r', '\n']))
    }
}

/// 将一组可能在任意位置切分的 SSE 文本转换为统一流事件。
///
/// # Errors
///
/// 任一完整数据帧不是有效 JSON，或其结构不符合指定 Adapter 协议时返回
/// [`ErrorCode::StreamInvalid`]。
pub fn decode_stream_chunks(
    adapter: AdapterKind,
    chunks: &[String],
) -> Result<Vec<StreamEvent>, CoreError> {
    let mut decoder = SseDecoder::new();
    let mut events = Vec::new();
    for chunk in chunks {
        for frame in decoder.push(chunk) {
            events.extend(parse_protocol_frame(adapter, &frame)?);
        }
    }
    if let Some(frame) = decoder.finish() {
        events.extend(parse_protocol_frame(adapter, &frame)?);
    }
    Ok(events)
}

/// 将一个已解码 SSE 帧转换为统一事件。
///
/// # Errors
///
/// 数据不是有效 JSON 或供应商帧缺少必要字段时返回错误。
pub fn parse_protocol_frame(
    adapter: AdapterKind,
    frame: &SseFrame,
) -> Result<Vec<StreamEvent>, CoreError> {
    if frame.data.trim().is_empty() {
        return Ok(Vec::new());
    }
    if frame.data.trim() == "[DONE]" {
        return Ok(vec![StreamEvent::Finished {
            reason: FinishReason::Stop,
        }]);
    }
    let value: Value = serde_json::from_str(&frame.data).map_err(|error| {
        CoreError::new(
            ErrorCode::StreamInvalid,
            format!("SSE Data 不是有效 JSON: {error}"),
        )
    })?;
    match adapter {
        AdapterKind::OpenaiCompatible => Ok(parse_openai_chunk(&value)),
        AdapterKind::AnthropicMessages => parse_anthropic_event(frame, &value),
        AdapterKind::GoogleGemini => Ok(parse_gemini_chunk(&value)),
    }
}

fn parse_frame(block: &str) -> Option<SseFrame> {
    if block.trim().is_empty() {
        return None;
    }
    let mut event = None;
    let mut id = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .map_or((line, ""), |(field, value)| (field, value.trim_start()));
        match field {
            "event" => event = Some(value.to_owned()),
            "id" => id = Some(value.to_owned()),
            "data" => data_lines.push(value),
            _ => {}
        }
    }
    if data_lines.is_empty() {
        None
    } else {
        Some(SseFrame {
            event,
            data: data_lines.join("\n"),
            id,
        })
    }
}

fn parse_openai_chunk(value: &Value) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    if value.get("id").is_some() || value.get("model").is_some() {
        events.push(StreamEvent::Started {
            response_id: optional_string(value, "/id"),
            model: optional_string(value, "/model"),
        });
    }
    if let Some(text) = value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
    {
        events.push(StreamEvent::TextDelta {
            text: text.to_owned(),
        });
    }
    if let Some(calls) = value
        .pointer("/choices/0/delta/tool_calls")
        .and_then(Value::as_array)
    {
        for call in calls {
            events.push(StreamEvent::ToolCallDelta {
                index: stream_index(call),
                id: optional_string(call, "/id"),
                name: optional_string(call, "/function/name"),
                arguments_delta: optional_string(call, "/function/arguments").unwrap_or_default(),
            });
        }
    }
    if let Some(usage) = value.get("usage") {
        events.push(StreamEvent::Usage {
            usage: Usage {
                input_tokens: unsigned_at(usage, "/prompt_tokens"),
                output_tokens: unsigned_at(usage, "/completion_tokens"),
                cached_input_tokens: usage
                    .pointer("/prompt_tokens_details/cached_tokens")
                    .and_then(Value::as_u64),
            },
        });
    }
    if let Some(reason) = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
    {
        events.push(StreamEvent::Finished {
            reason: map_finish_reason(Some(reason)),
        });
    }
    events
}

fn parse_anthropic_event(frame: &SseFrame, value: &Value) -> Result<Vec<StreamEvent>, CoreError> {
    let event_type = frame
        .event
        .as_deref()
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or_default();
    let events = match event_type {
        "message_start" => vec![
            StreamEvent::Started {
                response_id: optional_string(value, "/message/id"),
                model: optional_string(value, "/message/model"),
            },
            StreamEvent::Usage {
                usage: Usage {
                    input_tokens: unsigned_at(value, "/message/usage/input_tokens"),
                    output_tokens: 0,
                    cached_input_tokens: value
                        .pointer("/message/usage/cache_read_input_tokens")
                        .and_then(Value::as_u64),
                },
            },
        ],
        "content_block_delta" => match value.pointer("/delta/type").and_then(Value::as_str) {
            Some("text_delta") => vec![StreamEvent::TextDelta {
                text: optional_string(value, "/delta/text").unwrap_or_default(),
            }],
            Some("input_json_delta") => vec![StreamEvent::ToolCallDelta {
                index: stream_index(value),
                id: None,
                name: None,
                arguments_delta: optional_string(value, "/delta/partial_json").unwrap_or_default(),
            }],
            _ => Vec::new(),
        },
        "content_block_start"
            if value.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use") =>
        {
            vec![StreamEvent::ToolCallDelta {
                index: stream_index(value),
                id: optional_string(value, "/content_block/id"),
                name: optional_string(value, "/content_block/name"),
                arguments_delta: String::new(),
            }]
        }
        "message_delta" => {
            let mut result = Vec::new();
            if value.get("usage").is_some() {
                result.push(StreamEvent::Usage {
                    usage: Usage {
                        input_tokens: 0,
                        output_tokens: unsigned_at(value, "/usage/output_tokens"),
                        cached_input_tokens: None,
                    },
                });
            }
            if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                result.push(StreamEvent::Finished {
                    reason: map_finish_reason(Some(reason)),
                });
            }
            result
        }
        "message_stop" => vec![StreamEvent::Finished {
            reason: FinishReason::Stop,
        }],
        "error" => {
            return Err(CoreError::new(
                ErrorCode::StreamInvalid,
                optional_string(value, "/error/message")
                    .unwrap_or_else(|| "Anthropic 流返回错误".to_owned()),
            ));
        }
        _ => Vec::new(),
    };
    Ok(events)
}

fn parse_gemini_chunk(value: &Value) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    if value.get("responseId").is_some() || value.get("modelVersion").is_some() {
        events.push(StreamEvent::Started {
            response_id: optional_string(value, "/responseId"),
            model: optional_string(value, "/modelVersion"),
        });
    }
    if let Some(parts) = value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    {
        for (index, part) in parts.iter().enumerate() {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                events.push(StreamEvent::TextDelta {
                    text: text.to_owned(),
                });
            }
            if let Some(call) = part.get("functionCall") {
                events.push(StreamEvent::ToolCallDelta {
                    index,
                    id: None,
                    name: optional_string(call, "/name"),
                    arguments_delta: call.get("args").map(Value::to_string).unwrap_or_default(),
                });
            }
        }
    }
    if let Some(usage) = value.get("usageMetadata") {
        events.push(StreamEvent::Usage {
            usage: Usage {
                input_tokens: unsigned_at(usage, "/promptTokenCount"),
                output_tokens: unsigned_at(usage, "/candidatesTokenCount"),
                cached_input_tokens: usage.get("cachedContentTokenCount").and_then(Value::as_u64),
            },
        });
    }
    if let Some(reason) = value
        .pointer("/candidates/0/finishReason")
        .and_then(Value::as_str)
    {
        events.push(StreamEvent::Finished {
            reason: map_finish_reason(Some(reason)),
        });
    }
    events
}

fn stream_index(value: &Value) -> usize {
    value
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .unwrap_or(0)
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

fn map_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason.map(str::to_ascii_lowercase).as_deref() {
        Some("stop" | "end_turn") => FinishReason::Stop,
        Some("length" | "max_tokens") => FinishReason::Length,
        Some("tool_calls" | "tool_use") => FinishReason::ToolCall,
        Some("content_filter" | "safety") => FinishReason::ContentFilter,
        _ => FinishReason::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_handles_arbitrary_chunk_boundaries() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push("event: message\r\nda").is_empty());
        let frames = decoder.push("ta: {\"value\":1}\r\n\r\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event.as_deref(), Some("message"));
        assert_eq!(frames[0].data, "{\"value\":1}");
    }

    #[test]
    fn decodes_openai_text_and_done() -> Result<(), CoreError> {
        let events = decode_stream_chunks(
            AdapterKind::OpenaiCompatible,
            &[format!(
                "data: {}\n\ndata: [DONE]\n\n",
                serde_json::json!({
                    "id": "r1", "model": "m1",
                    "choices": [{"delta": {"content": "Hi"}, "finish_reason": null}]
                })
            )],
        )?;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::TextDelta { text } if text == "Hi"))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::Finished { .. }))
        );
        Ok(())
    }

    #[test]
    fn decodes_anthropic_delta() -> Result<(), CoreError> {
        let events = decode_stream_chunks(
            AdapterKind::AnthropicMessages,
            &["event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n".to_owned()],
        )?;
        assert_eq!(
            events,
            vec![StreamEvent::TextDelta {
                text: "Hi".to_owned()
            }]
        );
        Ok(())
    }
}
