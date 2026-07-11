use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRequest {
    pub schema_version: u32,
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub tool_choice: ToolChoice,
    #[serde(default)]
    pub response_format: ResponseFormat,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        url: String,
        #[serde(default)]
        detail: ImageDetail,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    Low,
    High,
    #[default]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "name", rename_all = "snake_case")]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    #[default]
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        schema: Value,
        #[serde(default)]
        strict: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalResponse {
    pub schema_version: u32,
    pub provider_response_id: Option<String>,
    pub model: Option<String>,
    pub content: Vec<ContentPart>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCall,
    ContentFilter,
    Error,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
}

impl CanonicalRequest {
    /// 校验统一请求是否能够安全进入 Adapter 层。
    ///
    /// # Errors
    ///
    /// Schema、模型、消息、Token、温度或工具定义无效时返回
    /// [`ErrorCode::RequestInvalid`]。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "请求 Schema 版本不受支持",
            ));
        }
        if self.model.trim().is_empty() {
            issues.push(ValidationIssue::new("model", "REQUIRED", "模型不能为空"));
        }
        if self.messages.is_empty() {
            issues.push(ValidationIssue::new(
                "messages",
                "REQUIRED",
                "至少需要一条消息",
            ));
        }
        if self.max_output_tokens == 0 || self.max_output_tokens > 1_000_000 {
            issues.push(ValidationIssue::new(
                "max_output_tokens",
                "OUT_OF_RANGE",
                "输出 Token 必须在 1 到 1000000 之间",
            ));
        }
        if self
            .temperature
            .is_some_and(|temperature| !(0.0..=2.0).contains(&temperature))
        {
            issues.push(ValidationIssue::new(
                "temperature",
                "OUT_OF_RANGE",
                "温度必须在 0 到 2 之间",
            ));
        }
        for (index, message) in self.messages.iter().enumerate() {
            if message.content.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("messages[{index}].content"),
                    "REQUIRED",
                    "消息内容不能为空",
                ));
            }
            if message.role == Role::Tool && message.tool_call_id.is_none() {
                issues.push(ValidationIssue::new(
                    format!("messages[{index}].tool_call_id"),
                    "REQUIRED",
                    "Tool 消息必须引用 tool_call_id",
                ));
            }
        }
        for (index, tool) in self.tools.iter().enumerate() {
            if tool.name.trim().is_empty() || !tool.input_schema.is_object() {
                issues.push(ValidationIssue::new(
                    format!("tools[{index}]"),
                    "INVALID_TOOL",
                    "工具名称不能为空且 input_schema 必须是 JSON Object",
                ));
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(
                ErrorCode::RequestInvalid,
                "统一请求校验失败",
                issues,
            ))
        }
    }
}

impl Message {
    #[must_use]
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
            name: None,
            tool_call_id: None,
        }
    }
}

const fn default_max_output_tokens() -> u32 {
    1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_minimal_request() -> Result<(), CoreError> {
        CanonicalRequest {
            schema_version: 1,
            model: "smart".to_owned(),
            messages: vec![Message::text(Role::User, "hello")],
            max_output_tokens: 128,
            temperature: Some(0.2),
            stream: false,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            response_format: ResponseFormat::Text,
            metadata: BTreeMap::new(),
        }
        .validate()
    }

    #[test]
    fn rejects_tool_message_without_call_id() {
        let mut request = CanonicalRequest {
            schema_version: 1,
            model: "smart".to_owned(),
            messages: vec![Message::text(Role::Tool, "result")],
            max_output_tokens: 128,
            temperature: None,
            stream: false,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            response_format: ResponseFormat::Text,
            metadata: BTreeMap::new(),
        };
        assert!(request.validate().is_err());
        request.messages[0].tool_call_id = Some("call-1".to_owned());
        assert!(request.validate().is_ok());
    }
}
