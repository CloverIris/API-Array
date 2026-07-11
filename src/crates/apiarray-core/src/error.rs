use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// 对 CLI、UI 和审计都稳定的错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    UnsupportedSchema,
    ProviderInvalid,
    AdapterUnsupported,
    RequestInvalid,
    ResponseInvalid,
    StreamInvalid,
    TemplateInvalid,
    RuntimeConfigInvalid,
    GraphInvalid,
    RouteUnavailable,
    PublisherInvalid,
    SecretInvalid,
    SerializationFailed,
    InternalRuntimeError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}

impl ValidationIssue {
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
    pub issues: Vec<ValidationIssue>,
}

impl CoreError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            issues: Vec::new(),
        }
    }

    #[must_use]
    pub fn validation(
        code: ErrorCode,
        message: impl Into<String>,
        issues: Vec<ValidationIssue>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            issues,
        }
    }
}

impl Display for CoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for CoreError {}
