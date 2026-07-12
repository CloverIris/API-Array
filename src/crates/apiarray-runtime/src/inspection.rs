use apiarray_core::{
    adapter::{HeaderPlan, HeaderValue, HttpMethod, QueryPlan, StreamProtocol, TransportPlan},
    capability::Evidence,
    inspection::{InspectionDimension, InspectionFinding, InspectionOverall, InspectionReport, InspectionStatus},
    provider::{AuthenticationType, DiscoveryKind, ProviderManifest},
    runtime::ProviderInstance,
};
use crate::RuntimeError;
use crate::secret::SecretResolver;
use crate::transport::{HttpExecutor, TransportConfig};
use serde_json::Value;
use std::{path::Path, time::{Instant, SystemTime, UNIX_EPOCH}};

#[derive(Clone)]
pub struct ProviderProbeRunner {
    executor: HttpExecutor,
}

impl ProviderProbeRunner {
    pub fn new(config: TransportConfig) -> Result<Self, RuntimeError> {
        Ok(Self { executor: HttpExecutor::new(config)? })
    }

    pub async fn run(
        &self,
        instance: &ProviderInstance,
        secrets: &dyn SecretResolver,
        allow_billable: bool,
    ) -> Result<InspectionReport, RuntimeError> {
        let now = unix_millis();
        let mut findings = Vec::new();
        findings.push(self.connectivity(instance, secrets).await);
        findings.push(self.authentication(instance, secrets).await);
        findings.push(self.models(instance, secrets).await);
        findings.push(self.static_capability(
            InspectionDimension::Streaming,
            &instance.manifest,
            "streaming",
        ));
        findings.push(self.static_capability(
            InspectionDimension::ProtocolCompatibility,
            &instance.manifest,
            "protocol",
        ));
        let billable = instance
            .manifest
            .probes
            .iter()
            .filter(|probe| matches!(probe.side_effect, apiarray_core::provider::ProbeSideEffect::Billable))
            .count();
        findings.push(InspectionFinding {
            dimension: InspectionDimension::ContextWindow,
            status: if billable > 0 && !allow_billable { InspectionStatus::AuthorizationRequired } else { InspectionStatus::NotRun },
            evidence: Evidence::ProviderDeclared,
            latency_ms: None,
            safe_summary: Some(if billable > 0 && !allow_billable { "存在需授权的计费探测，当前未执行" } else { "当前 Provider 未声明无副作用上下文探测" }.to_owned()),
            declared_value: None,
            verified_value: None,
        });
        let overall = if findings.iter().any(|finding| finding.status == InspectionStatus::Failed) {
            InspectionOverall::Degraded
        } else if findings.iter().any(|finding| matches!(finding.status, InspectionStatus::Unknown | InspectionStatus::Unsupported | InspectionStatus::AuthorizationRequired | InspectionStatus::Partial)) {
            InspectionOverall::Unknown
        } else {
            InspectionOverall::Healthy
        };
        InspectionReport::build(instance.id.clone(), now, findings, Some(now), 0)
            .map_err(RuntimeError::from)
            .map(|mut report| { report.overall = overall; report })
    }

    async fn connectivity(&self, instance: &ProviderInstance, secrets: &dyn SecretResolver) -> InspectionFinding {
        let started = Instant::now();
        let path = match model_discovery_path(instance) {
            Some(path) => path,
            None => return unsupported(InspectionDimension::Network, "Provider 未声明可用的网络探测路径"),
        };
        let plan = match discovery_plan(instance, path) {
            Ok(plan) => plan,
            Err(error) => return failed(InspectionDimension::Network, error.safe_message),
        };
        match self.executor.execute_json(&plan, secrets).await {
            Ok(_) => passed(InspectionDimension::Network, started.elapsed().as_millis() as u64, "端点返回了有效 JSON"),
            Err(error) => InspectionFinding {
                dimension: InspectionDimension::Network,
                status: InspectionStatus::Failed,
                evidence: Evidence::Verified,
                latency_ms: Some(started.elapsed().as_millis() as u64),
                safe_summary: Some(error.safe_message),
                declared_value: None,
                verified_value: None,
            },
        }
    }

    async fn authentication(&self, instance: &ProviderInstance, secrets: &dyn SecretResolver) -> InspectionFinding {
        let started = Instant::now();
        let Some(reference) = instance.secret_refs.values().next() else {
            return InspectionFinding {
                dimension: InspectionDimension::Authentication,
                status: InspectionStatus::AuthorizationRequired,
                evidence: Evidence::Unknown,
                latency_ms: None,
                safe_summary: Some("尚未绑定 Secret".to_owned()),
                declared_value: None,
                verified_value: None,
            };
        };
        match secrets.resolve(reference) {
            Ok(_) => passed(InspectionDimension::Authentication, started.elapsed().as_millis() as u64, "本机 Secret 已绑定"),
            Err(error) => failed(InspectionDimension::Authentication, error.safe_message),
        }
    }

    async fn models(&self, instance: &ProviderInstance, secrets: &dyn SecretResolver) -> InspectionFinding {
        let started = Instant::now();
        let path = match model_discovery_path(instance) {
            Some(path) => path,
            None if matches!(instance.manifest.discovery.models.strategy, DiscoveryKind::Static) => return passed(InspectionDimension::Models, 0, "Provider 使用静态模型声明"),
            None => return unsupported(InspectionDimension::Models, "Provider 未声明模型发现"),
        };
        let plan = match discovery_plan(instance, path) {
            Ok(plan) => plan,
            Err(error) => return failed(InspectionDimension::Models, error.safe_message),
        };
        match self.executor.execute_json(&plan, secrets).await {
            Ok(value) => {
                let count = value.get("data").and_then(Value::as_array).map_or_else(
                    || value.get("models").and_then(Value::as_array).map_or(0, Vec::len),
                    Vec::len,
                );
                passed(InspectionDimension::Models, started.elapsed().as_millis() as u64, &format!("发现 {count} 个模型"))
            }
            Err(error) => failed(InspectionDimension::Models, error.safe_message),
        }
    }

    fn static_capability(&self, dimension: InspectionDimension, manifest: &ProviderManifest, label: &str) -> InspectionFinding {
        let declared = manifest.adapter.id.clone();
        InspectionFinding {
            dimension,
            status: InspectionStatus::Partial,
            evidence: Evidence::AdapterInferred,
            latency_ms: None,
            safe_summary: Some(format!("由 Adapter {label} 能力声明推断")),
            declared_value: Some(declared),
            verified_value: None,
        }
    }
}

fn model_discovery_path(instance: &ProviderInstance) -> Option<&str> {
    match instance.manifest.discovery.models.strategy {
        DiscoveryKind::Endpoint => Some(instance.manifest.discovery.models.path.as_deref().unwrap_or("/models")),
        DiscoveryKind::Adapter if instance.manifest.adapter.id == "google-gemini" => Some("/v1beta/models"),
        DiscoveryKind::Adapter | DiscoveryKind::Unsupported | DiscoveryKind::Static => None,
    }
}

fn discovery_plan(instance: &ProviderInstance, path: &str) -> Result<TransportPlan, RuntimeError> {
    let base = instance.endpoint_override.as_deref().unwrap_or(&instance.manifest.endpoint.default_base_url).trim_end_matches('/');
    let url = if path == "/" { base.to_owned() } else { format!("{base}/{path}", path = path.trim_start_matches('/')) };
    let mut headers = instance.manifest.headers.iter().map(|header| HeaderPlan { name: header.name.clone(), value: HeaderValue::Static { value: header.value.clone() } }).collect::<Vec<_>>();
    let mut query = Vec::new();
    for field in &instance.manifest.authentication.fields {
        if !field.secret { continue; }
        let Some(reference) = instance.secret_refs.get(&field.id).cloned() else { continue; };
        let value = HeaderValue::SecretRef { reference, prefix: field.prefix.clone().unwrap_or_default() };
        match instance.manifest.authentication.kind {
            AuthenticationType::HeaderSecret => if let Some(name) = &field.header { headers.push(HeaderPlan { name: name.to_ascii_lowercase(), value }); },
            AuthenticationType::QuerySecret => if let Some(name) = &field.query { query.push(QueryPlan { name: name.clone(), value }); },
            AuthenticationType::None => {}
        }
    }
    Ok(TransportPlan { adapter: apiarray_core::adapter::AdapterKind::parse(&instance.manifest.adapter.id).map_err(RuntimeError::from)?, method: HttpMethod::Get, url, headers, query, body: Value::Null, stream_protocol: StreamProtocol::None })
}

fn passed(dimension: InspectionDimension, latency_ms: u64, summary: &str) -> InspectionFinding { InspectionFinding { dimension, status: InspectionStatus::Passed, evidence: Evidence::Verified, latency_ms: Some(latency_ms), safe_summary: Some(summary.to_owned()), declared_value: None, verified_value: None } }
fn failed(dimension: InspectionDimension, summary: String) -> InspectionFinding { InspectionFinding { dimension, status: InspectionStatus::Failed, evidence: Evidence::VerificationFailed, latency_ms: None, safe_summary: Some(summary), declared_value: None, verified_value: None } }
fn unsupported(dimension: InspectionDimension, summary: &str) -> InspectionFinding { InspectionFinding { dimension, status: InspectionStatus::Unsupported, evidence: Evidence::ProviderDeclared, latency_ms: None, safe_summary: Some(summary.to_owned()), declared_value: None, verified_value: None } }
fn unix_millis() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)) }

#[derive(Debug, Clone)]
pub struct InspectionRepository { repository: crate::persistence::WorkspaceRepository }

impl InspectionRepository {
    pub fn new(root: impl AsRef<Path>) -> Self { Self { repository: crate::persistence::WorkspaceRepository::new(root.as_ref()) } }
    #[must_use]
    pub fn from_repository(repository: crate::persistence::WorkspaceRepository) -> Self { Self { repository } }
    pub fn save(&self, report: &InspectionReport) -> Result<(), RuntimeError> {
        self.repository.save_inspection(report)
    }
    pub fn load(&self, provider_id: &str) -> Result<Option<InspectionReport>, RuntimeError> {
        self.repository.load_inspection(provider_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apiarray_core::inspection::{InspectionFinding, InspectionStatus};

    #[test]
    fn report_repository_round_trips_without_using_provider_id_as_a_path() -> Result<(), RuntimeError> {
        let root = std::env::temp_dir().join(format!("apiarray-inspection-{}", unix_millis()));
        let repository = InspectionRepository::new(&root);
        let report = InspectionReport::build(
            "provider/with spaces",
            1,
            vec![InspectionFinding {
                dimension: InspectionDimension::Network,
                status: InspectionStatus::Passed,
                evidence: Evidence::Verified,
                latency_ms: Some(4),
                safe_summary: Some("ok".to_owned()),
                declared_value: None,
                verified_value: None,
            }],
            Some(1),
            0,
        )?;
        repository.save(&report)?;
        let loaded = repository.load("provider/with spaces")?.expect("saved report");
        assert_eq!(loaded.provider_id, report.provider_id);
        let _ = std::fs::remove_dir_all(root);
        Ok(())
    }
}
