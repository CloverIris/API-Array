use crate::SCHEMA_VERSION;
use crate::adapter::{AdapterKind, build_transport_plan, classify_http_error, parse_response};
use crate::canonical::CanonicalRequest;
use crate::capability::CapabilityManifest;
use crate::error::{CoreError, ErrorCode, ValidationIssue};
use crate::graph::WorkflowGraph;
use crate::health::{EndpointHealth, HealthObservation, HealthPolicy};
use crate::inspection::{InspectionFinding, InspectionReport, ProbeAuthorization, authorize_probe};
use crate::provider::{ProbeDefinition, ProviderManifest};
use crate::publisher::PublisherConfig;
use crate::routing::{RouteCandidate, RoutePolicy, RouteRequest};
use crate::runtime::{DispatchRequest, RuntimeConfig};
use crate::secret::SecretRef;
use crate::secret::redact_diagnostic;
use crate::stream::decode_stream_chunks;
use crate::templates::{TemplateContext, generate_templates};
use crate::workspace::{WorkspacePackage, load_workspace_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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
    AnalyzeNodeImpact {
        graph: WorkflowGraph,
        node_id: String,
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
    BuildTransportPlan {
        provider_yaml: String,
        secret_refs: BTreeMap<String, String>,
        request: CanonicalRequest,
    },
    ParseResponse {
        adapter: AdapterKind,
        response: Value,
    },
    DecodeStream {
        adapter: AdapterKind,
        chunks: Vec<String>,
    },
    ClassifyHttpError {
        status: u16,
        body: Value,
    },
    GenerateTemplates {
        context: TemplateContext,
    },
    CompileRuntime {
        config: RuntimeConfig,
    },
    PlanDispatch {
        config: RuntimeConfig,
        dispatch: DispatchRequest,
    },
    ObserveHealth {
        state: EndpointHealth,
        policy: HealthPolicy,
        observation: HealthObservation,
    },
    AuthorizeProbe {
        probe: ProbeDefinition,
        #[serde(default)]
        authorization: ProbeAuthorization,
    },
    BuildInspectionReport {
        provider_id: String,
        generated_at_unix_ms: u64,
        findings: Vec<InspectionFinding>,
        #[serde(default)]
        last_success_at_unix_ms: Option<u64>,
        #[serde(default)]
        consecutive_failures: u32,
    },
    ExportWorkspace {
        workspace: WorkspacePackage,
    },
    LoadWorkspace {
        json: String,
    },
    WorkspaceSecretStatus {
        workspace: WorkspacePackage,
        #[serde(default)]
        available_secret_refs: BTreeSet<String>,
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
#[allow(clippy::too_many_lines)]
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
        CliCommand::AnalyzeNodeImpact { graph, node_id } => {
            serde_json::to_value(graph.impact_of_node(&node_id)?)?
        }
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
        CliCommand::BuildTransportPlan {
            provider_yaml,
            secret_refs,
            request,
        } => {
            let provider = ProviderManifest::from_yaml(&provider_yaml)?;
            let refs = secret_refs
                .into_iter()
                .map(|(field, reference)| Ok((field, SecretRef::parse(reference)?)))
                .collect::<Result<BTreeMap<_, _>, CoreError>>()?;
            serde_json::to_value(build_transport_plan(&provider, &refs, &request)?)?
        }
        CliCommand::ParseResponse { adapter, response } => {
            serde_json::to_value(parse_response(adapter, &response)?)?
        }
        CliCommand::DecodeStream { adapter, chunks } => {
            serde_json::to_value(decode_stream_chunks(adapter, &chunks)?)?
        }
        CliCommand::ClassifyHttpError { status, body } => {
            serde_json::to_value(classify_http_error(status, &body))?
        }
        CliCommand::GenerateTemplates { context } => {
            serde_json::to_value(generate_templates(&context)?)?
        }
        CliCommand::CompileRuntime { config } => serde_json::to_value(config.compile()?.summary())?,
        CliCommand::PlanDispatch { config, dispatch } => {
            serde_json::to_value(config.compile()?.plan_dispatch(&dispatch)?)?
        }
        CliCommand::ObserveHealth {
            mut state,
            policy,
            observation,
        } => {
            let change = state.observe(&policy, observation)?;
            json!({ "state": state, "change": change })
        }
        CliCommand::AuthorizeProbe {
            probe,
            authorization,
        } => {
            authorize_probe(&probe, authorization)?;
            json!({ "authorized": true, "probe_id": probe.id })
        }
        CliCommand::BuildInspectionReport {
            provider_id,
            generated_at_unix_ms,
            findings,
            last_success_at_unix_ms,
            consecutive_failures,
        } => serde_json::to_value(InspectionReport::build(
            provider_id,
            generated_at_unix_ms,
            findings,
            last_success_at_unix_ms,
            consecutive_failures,
        )?)?,
        CliCommand::ExportWorkspace { workspace } => {
            json!({ "json": workspace.export_json()? })
        }
        CliCommand::LoadWorkspace { json } => serde_json::to_value(load_workspace_json(&json)?)?,
        CliCommand::WorkspaceSecretStatus {
            workspace,
            available_secret_refs,
        } => serde_json::to_value(workspace.secret_status(&available_secret_refs))?,
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

    #[test]
    fn template_command_returns_eight_languages() {
        let response = parse_and_dispatch(
            r#"{"schema_version":1,"operation":"generate_templates","context":{"base_url":"http://127.0.0.1:6188/v1","model":"smart","stream":false}}"#,
        );
        assert!(response.ok);
        assert_eq!(
            response.result.expect("result").as_array().map(Vec::len),
            Some(8)
        );
    }

    #[test]
    fn observe_health_command_returns_transition() {
        let response = parse_and_dispatch(
            r#"{"schema_version":1,"operation":"observe_health","state":{"status":"unknown","consecutive_successes":0,"consecutive_failures":0,"last_latency_ms":null,"last_error":null,"observation_count":0},"policy":{"schema_version":1,"failure_threshold":3,"recovery_threshold":2,"degraded_latency_ms":10000},"observation":{"success":true,"latency_ms":20,"error":null}}"#,
        );
        assert!(response.ok);
        assert_eq!(
            response.result.expect("result")["state"]["status"],
            "healthy"
        );
    }
}
