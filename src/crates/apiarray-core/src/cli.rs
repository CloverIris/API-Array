use crate::SCHEMA_VERSION;
use crate::capability::CapabilityManifest;
use crate::error::{CoreError, ErrorCode, ValidationIssue};
use crate::graph::WorkflowGraph;
use crate::provider::ProviderManifest;
use crate::publisher::PublisherConfig;
use crate::routing::{RouteCandidate, RoutePolicy, RouteRequest};
use crate::secret::redact_diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Deserialize)]
pub struct CliRequest {
    pub schema_version: u32,
    #[serde(flatten)]
    pub command: CliCommand,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum CliCommand {
    ValidateProvider {
        yaml: String,
    },
    ValidateGraph {
        graph: WorkflowGraph,
    },
    DecideRoute {
        policy: RoutePolicy,
        request: RouteRequest,
        candidates: Vec<RouteCandidate>,
    },
    MergeCapabilities {
        source_id: String,
        manifests: Vec<CapabilityManifest>,
    },
    ValidatePublisher {
        publisher: PublisherConfig,
    },
    RedactDiagnostic {
        value: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CliResponse {
    pub schema_version: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CliError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ValidationIssue>,
}

impl CliResponse {
    #[must_use]
    pub fn success(result: Value) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(error: CoreError) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ok: false,
            result: None,
            error: Some(CliError {
                code: error.code,
                message: error.message,
                issues: error.issues,
            }),
        }
    }
}

pub fn parse_and_dispatch(line: &str) -> CliResponse {
    match serde_json::from_str::<CliRequest>(line) {
        Ok(request) => dispatch(request).unwrap_or_else(CliResponse::failure),
        Err(error) => CliResponse::failure(CoreError::new(
            ErrorCode::InvalidInput,
            format!("CLI JSON 无法解析: {error}"),
        )),
    }
}

/// 执行一个已解析的版本化 CLI 请求。
///
/// # Errors
///
/// Schema 不受支持或对应核心操作校验失败时返回稳定的 [`CoreError`]。
pub fn dispatch(request: CliRequest) -> Result<CliResponse, CoreError> {
    if request.schema_version != SCHEMA_VERSION {
        return Err(CoreError::new(
            ErrorCode::UnsupportedSchema,
            format!(
                "CLI 只支持 Schema {SCHEMA_VERSION}，收到 {}",
                request.schema_version
            ),
        ));
    }

    let result = match request.command {
        CliCommand::ValidateProvider { yaml } => {
            let manifest = ProviderManifest::from_yaml(&yaml)?;
            json!({
                "provider_id": manifest.provider.id,
                "provider_name": manifest.provider.name,
                "adapter_id": manifest.adapter.id,
                "probe_count": manifest.probes.len(),
            })
        }
        CliCommand::ValidateGraph { graph } => serde_json::to_value(graph.validate()?)?,
        CliCommand::DecideRoute {
            policy,
            request,
            candidates,
        } => serde_json::to_value(policy.decide(&request, &candidates)?)?,
        CliCommand::MergeCapabilities {
            source_id,
            manifests,
        } => serde_json::to_value(CapabilityManifest::merge(source_id, &manifests)?)?,
        CliCommand::ValidatePublisher { publisher } => serde_json::to_value(publisher.validate()?)?,
        CliCommand::RedactDiagnostic { value } => {
            json!({ "redacted": redact_diagnostic(&value) })
        }
    };
    Ok(CliResponse::success(result))
}

impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(
            ErrorCode::SerializationFailed,
            format!("JSON 序列化失败: {error}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_json_returns_machine_readable_failure() {
        let response = parse_and_dispatch("not json");
        assert!(!response.ok);
        assert_eq!(response.error.expect("error").code, ErrorCode::InvalidInput);
    }

    #[test]
    fn redaction_command_does_not_echo_auth_value() {
        let response = parse_and_dispatch(
            r#"{"schema_version":1,"operation":"redact_diagnostic","value":"Authorization: Bearer secret"}"#,
        );
        assert!(response.ok);
        assert_eq!(response.result.expect("result")["redacted"], "[REDACTED]");
    }
}
