use apiarray_core::routing::StandardError;
use apiarray_core::{CoreError, ErrorCode};
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeErrorCode {
    CoreRejected,
    EnvironmentInvalid,
    SecretUnavailable,
    TransportBuildFailed,
    UpstreamFailed,
    ResponseTooLarge,
    ResponseInvalid,
    PublisherUnauthorized,
    PublisherBindFailed,
    PublisherServeFailed,
    AuditUnavailable,
    SecretStoreUnavailable,
    WorkspaceStorageUnavailable,
    WorkspaceConflict,
    PublisherAlreadyRunning,
    PublisherNotRunning,
    RateLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub safe_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_error: Option<StandardError>,
}

impl RuntimeError {
    #[must_use]
    pub fn new(code: RuntimeErrorCode, safe_message: impl Into<String>) -> Self {
        Self {
            code,
            safe_message: safe_message.into(),
            upstream_error: None,
        }
    }

    #[must_use]
    pub fn upstream(error: StandardError, safe_message: impl Into<String>) -> Self {
        Self {
            code: RuntimeErrorCode::UpstreamFailed,
            safe_message: safe_message.into(),
            upstream_error: Some(error),
        }
    }
}

impl Display for RuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.safe_message)
    }
}

impl Error for RuntimeError {}

impl From<CoreError> for RuntimeError {
    fn from(error: CoreError) -> Self {
        let message = match error.code {
            ErrorCode::SecretInvalid => "Secret 引用无效或缺失".to_owned(),
            _ => error.message,
        };
        Self::new(RuntimeErrorCode::CoreRejected, message)
    }
}
