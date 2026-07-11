use crate::adapter::{AdapterKind, TransportPlan, build_transport_plan};
use crate::canonical::CanonicalRequest;
use crate::provider::ProviderManifest;
use crate::publisher::{PublisherConfig, PublisherSummary};
use crate::routing::{
    HealthStatus, RouteCandidate, RouteDecision, RoutePolicy, RouteRequest, StandardError,
};
use crate::secret::SecretRef;
use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    pub id: String,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderInstance>,
    #[serde(default)]
    pub publishers: BTreeMap<String, RuntimePublisher>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInstance {
    pub id: String,
    pub manifest: ProviderManifest,
    #[serde(default)]
    pub endpoint_override: Option<String>,
    #[serde(default)]
    pub secret_refs: BTreeMap<String, SecretRef>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimePublisher {
    pub config: PublisherConfig,
    pub routes: Vec<ModelRoute>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub public_model: String,
    pub policy: RoutePolicy,
    pub upstreams: Vec<UpstreamRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamRoute {
    pub id: String,
    pub provider_instance: String,
    pub upstream_model: String,
    pub priority: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub conditions: Vec<RouteCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteCondition {
    MetadataEquals { key: String, value: String },
    MetadataPresent { key: String },
}

impl RouteCondition {
    fn matches(&self, metadata: &BTreeMap<String, String>) -> bool {
        match self {
            Self::MetadataEquals { key, value } => metadata.get(key) == Some(value),
            Self::MetadataPresent { key } => metadata.contains_key(key),
        }
    }

    fn validate(&self) -> bool {
        match self {
            Self::MetadataEquals { key, value } => {
                !key.trim().is_empty() && key.len() <= 64 && value.len() <= 256
            }
            Self::MetadataPresent { key } => !key.trim().is_empty() && key.len() <= 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSummary {
    pub runtime_id: String,
    pub provider_count: usize,
    pub enabled_provider_count: usize,
    pub publisher_count: usize,
    pub route_count: usize,
    pub upstream_count: usize,
    pub publishers: BTreeMap<String, PublisherSummary>,
}

#[derive(Debug, Clone)]
pub struct CompiledRuntime {
    config: RuntimeConfig,
    summary: RuntimeSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchRequest {
    pub publisher_id: String,
    pub request: CanonicalRequest,
    #[serde(default)]
    pub health: BTreeMap<String, HealthStatus>,
    #[serde(default)]
    pub excluded_upstreams: HashSet<String>,
    #[serde(default)]
    pub previous_error: Option<StandardError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchPlan {
    pub runtime_id: String,
    pub publisher_id: String,
    pub public_model: String,
    pub upstream_model: String,
    pub provider_instance: String,
    pub route: RouteDecision,
    pub transport: TransportPlan,
}

impl RuntimeConfig {
    /// 编译并校验完整 Runtime 配置，不执行任何网络操作。
    ///
    /// # Errors
    ///
    /// Provider、Publisher、模型路由或交叉引用无效时，返回包含全部已发现问题的
    /// [`ErrorCode::RuntimeConfigInvalid`]。
    #[allow(clippy::too_many_lines)]
    pub fn compile(self) -> Result<CompiledRuntime, CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "Runtime Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new(
                "id",
                "REQUIRED",
                "Runtime ID 不能为空",
            ));
        }

        for (key, instance) in &self.providers {
            let path = format!("providers.{key}");
            if key != &instance.id {
                issues.push(ValidationIssue::new(
                    format!("{path}.id"),
                    "KEY_MISMATCH",
                    "Provider Instance 的映射键必须与 ID 一致",
                ));
            }
            if let Err(error) = instance.manifest.validate() {
                append_nested_issues(&mut issues, &format!("{path}.manifest"), error);
            }
            if AdapterKind::parse(&instance.manifest.adapter.id).is_err() {
                issues.push(ValidationIssue::new(
                    format!("{path}.manifest.adapter.id"),
                    "ADAPTER_UNSUPPORTED",
                    "Provider 引用了尚未实现的 Adapter",
                ));
            }
            if let Some(endpoint) = &instance.endpoint_override
                && !endpoint_allowed(&instance.manifest, endpoint)
            {
                issues.push(ValidationIssue::new(
                    format!("{path}.endpoint_override"),
                    "SCHEME_NOT_ALLOWED",
                    "Endpoint Override 的 Scheme 不在 Provider 允许列表中",
                ));
            }
            for field in instance
                .manifest
                .authentication
                .fields
                .iter()
                .filter(|field| field.secret && field.required)
            {
                if !instance.secret_refs.contains_key(&field.id) {
                    issues.push(ValidationIssue::new(
                        format!("{path}.secret_refs.{}", field.id),
                        "SECRET_REFERENCE_REQUIRED",
                        "缺少必需的 Secret 引用",
                    ));
                }
            }
        }

        let mut summaries = BTreeMap::new();
        let mut route_count = 0;
        let mut upstream_count = 0;
        for (key, publisher) in &self.publishers {
            let path = format!("publishers.{key}");
            if key != &publisher.config.id {
                issues.push(ValidationIssue::new(
                    format!("{path}.config.id"),
                    "KEY_MISMATCH",
                    "Publisher 的映射键必须与配置 ID 一致",
                ));
            }
            match publisher.config.validate() {
                Ok(summary) => {
                    summaries.insert(key.clone(), summary);
                }
                Err(error) => append_nested_issues(&mut issues, &format!("{path}.config"), error),
            }
            if publisher.routes.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.routes"),
                    "REQUIRED",
                    "Publisher 至少需要一个模型路由",
                ));
            }
            let mut public_models = HashSet::new();
            for (route_index, route) in publisher.routes.iter().enumerate() {
                route_count += 1;
                let route_path = format!("{path}.routes[{route_index}]");
                if route.public_model.trim().is_empty()
                    || !public_models.insert(&route.public_model)
                {
                    issues.push(ValidationIssue::new(
                        format!("{route_path}.public_model"),
                        "INVALID_OR_DUPLICATE",
                        "公开模型名不能为空且在 Publisher 内必须唯一",
                    ));
                }
                if let Err(error) = route.policy.validate() {
                    append_nested_issues(&mut issues, &format!("{route_path}.policy"), error);
                }
                if route.upstreams.is_empty() {
                    issues.push(ValidationIssue::new(
                        format!("{route_path}.upstreams"),
                        "REQUIRED",
                        "模型路由至少需要一个上游",
                    ));
                }
                let mut upstream_ids = HashSet::new();
                for (upstream_index, upstream) in route.upstreams.iter().enumerate() {
                    upstream_count += 1;
                    let upstream_path = format!("{route_path}.upstreams[{upstream_index}]");
                    if upstream.id.trim().is_empty() || !upstream_ids.insert(&upstream.id) {
                        issues.push(ValidationIssue::new(
                            format!("{upstream_path}.id"),
                            "INVALID_OR_DUPLICATE",
                            "上游 ID 不能为空且在模型路由内必须唯一",
                        ));
                    }
                    if upstream.upstream_model.trim().is_empty() {
                        issues.push(ValidationIssue::new(
                            format!("{upstream_path}.upstream_model"),
                            "REQUIRED",
                            "上游模型名不能为空",
                        ));
                    }
                    match self.providers.get(&upstream.provider_instance) {
                        Some(provider) if provider.enabled => {}
                        Some(_) if upstream.enabled => issues.push(ValidationIssue::new(
                            format!("{upstream_path}.provider_instance"),
                            "PROVIDER_DISABLED",
                            "启用的上游不能引用已停用 Provider",
                        )),
                        Some(_) => {}
                        None => issues.push(ValidationIssue::new(
                            format!("{upstream_path}.provider_instance"),
                            "PROVIDER_NOT_FOUND",
                            "上游引用的 Provider Instance 不存在",
                        )),
                    }
                    for (condition_index, condition) in upstream.conditions.iter().enumerate() {
                        if !condition.validate() {
                            issues.push(ValidationIssue::new(
                                format!("{upstream_path}.conditions[{condition_index}]"),
                                "INVALID_CONDITION",
                                "简单路由条件的键值为空或超过长度限制",
                            ));
                        }
                    }
                }
            }
        }

        if !issues.is_empty() {
            return Err(CoreError::validation(
                ErrorCode::RuntimeConfigInvalid,
                "Runtime 配置编译失败",
                issues,
            ));
        }

        let summary = RuntimeSummary {
            runtime_id: self.id.clone(),
            provider_count: self.providers.len(),
            enabled_provider_count: self
                .providers
                .values()
                .filter(|provider| provider.enabled)
                .count(),
            publisher_count: self.publishers.len(),
            route_count,
            upstream_count,
            publishers: summaries,
        };
        Ok(CompiledRuntime {
            config: self,
            summary,
        })
    }
}

impl CompiledRuntime {
    #[must_use]
    pub const fn summary(&self) -> &RuntimeSummary {
        &self.summary
    }

    #[must_use]
    pub fn publisher_config(&self, publisher_id: &str) -> Option<&PublisherConfig> {
        self.config
            .publishers
            .get(publisher_id)
            .map(|publisher| &publisher.config)
    }

    /// 将 Publisher 请求编译为一个不包含明文密钥的上游 Transport Plan。
    ///
    /// # Errors
    ///
    /// Publisher、公开模型、候选上游不存在，或所有候选均不可用时返回错误。
    pub fn plan_dispatch(&self, dispatch: &DispatchRequest) -> Result<DispatchPlan, CoreError> {
        dispatch.request.validate()?;
        let publisher = self
            .config
            .publishers
            .get(&dispatch.publisher_id)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::RouteUnavailable,
                    format!("Publisher {} 不存在", dispatch.publisher_id),
                )
            })?;
        let model_route = publisher
            .routes
            .iter()
            .find(|route| route.public_model == dispatch.request.model)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::RouteUnavailable,
                    format!("公开模型 {} 没有路由", dispatch.request.model),
                )
            })?;
        let candidates = model_route
            .upstreams
            .iter()
            .map(|upstream| RouteCandidate {
                id: upstream.id.clone(),
                priority: upstream.priority,
                enabled: upstream.enabled
                    && upstream
                        .conditions
                        .iter()
                        .all(|condition| condition.matches(&dispatch.request.metadata)),
                health: dispatch
                    .health
                    .get(&upstream.id)
                    .copied()
                    .unwrap_or(HealthStatus::Unknown),
                models: HashSet::from([model_route.public_model.clone()]),
            })
            .collect::<Vec<_>>();
        let decision = model_route.policy.decide(
            &RouteRequest {
                model: model_route.public_model.clone(),
                excluded_candidates: dispatch.excluded_upstreams.clone(),
                previous_error: dispatch.previous_error,
            },
            &candidates,
        )?;
        let upstream = model_route
            .upstreams
            .iter()
            .find(|upstream| upstream.id == decision.candidate_id)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::InternalRuntimeError,
                    "路由决策引用了不存在的上游",
                )
            })?;
        let provider = self
            .config
            .providers
            .get(&upstream.provider_instance)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::InternalRuntimeError,
                    "上游引用的 Provider 在编译后丢失",
                )
            })?;
        let mut manifest = provider.manifest.clone();
        if let Some(endpoint) = &provider.endpoint_override {
            manifest.endpoint.default_base_url.clone_from(endpoint);
        }
        let mut upstream_request = dispatch.request.clone();
        upstream_request.model.clone_from(&upstream.upstream_model);
        let transport = build_transport_plan(&manifest, &provider.secret_refs, &upstream_request)?;

        Ok(DispatchPlan {
            runtime_id: self.config.id.clone(),
            publisher_id: dispatch.publisher_id.clone(),
            public_model: model_route.public_model.clone(),
            upstream_model: upstream.upstream_model.clone(),
            provider_instance: upstream.provider_instance.clone(),
            route: decision,
            transport,
        })
    }
}

fn append_nested_issues(issues: &mut Vec<ValidationIssue>, prefix: &str, error: CoreError) {
    if error.issues.is_empty() {
        issues.push(ValidationIssue::new(
            prefix,
            format!("{:?}", error.code).to_ascii_uppercase(),
            error.message,
        ));
    } else {
        issues.extend(error.issues.into_iter().map(|issue| {
            ValidationIssue::new(
                format!("{prefix}.{}", issue.path),
                issue.code,
                issue.message,
            )
        }));
    }
}

fn endpoint_allowed(manifest: &ProviderManifest, endpoint: &str) -> bool {
    endpoint
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .is_some_and(|scheme| manifest.endpoint.allowed_schemes.contains(&scheme))
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{Message, ResponseFormat, Role, ToolChoice};
    use std::collections::BTreeMap;

    fn runtime_config() -> RuntimeConfig {
        let manifest = ProviderManifest::from_yaml(include_str!("../../../providers/openai.yaml"))
            .expect("built-in provider must be valid");
        let provider = ProviderInstance {
            id: "openai-main".to_owned(),
            manifest,
            endpoint_override: None,
            secret_refs: BTreeMap::from([(
                "api_key".to_owned(),
                SecretRef::parse("secret://workspace/openai/main").expect("valid ref"),
            )]),
            enabled: true,
        };
        RuntimeConfig {
            schema_version: 1,
            id: "desktop".to_owned(),
            providers: BTreeMap::from([("openai-main".to_owned(), provider)]),
            publishers: BTreeMap::from([(
                "local-ai".to_owned(),
                RuntimePublisher {
                    config: PublisherConfig {
                        schema_version: 1,
                        id: "local-ai".to_owned(),
                        name: "Local AI".to_owned(),
                        listen_address: "127.0.0.1".parse().expect("valid ip"),
                        port: 6188,
                        base_path: "/v1".to_owned(),
                        require_token: true,
                        token_ref: Some(
                            SecretRef::parse("secret://publisher/local-ai")
                                .expect("valid secret ref"),
                        ),
                    },
                    routes: vec![ModelRoute {
                        public_model: "smart".to_owned(),
                        policy: RoutePolicy {
                            schema_version: 1,
                            id: "smart-failover".to_owned(),
                            timeout_ms: 30_000,
                            max_retries: 2,
                            failover_on: HashSet::from([StandardError::ProviderTimeout]),
                        },
                        upstreams: vec![UpstreamRoute {
                            id: "openai-primary".to_owned(),
                            provider_instance: "openai-main".to_owned(),
                            upstream_model: "upstream-model".to_owned(),
                            priority: 0,
                            enabled: true,
                            conditions: Vec::new(),
                        }],
                    }],
                },
            )]),
        }
    }

    fn canonical_request() -> CanonicalRequest {
        CanonicalRequest {
            schema_version: 1,
            model: "smart".to_owned(),
            messages: vec![Message::text(Role::User, "hello")],
            max_output_tokens: 128,
            temperature: None,
            stream: true,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            response_format: ResponseFormat::Text,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn compiles_and_plans_dispatch_without_secret_value() -> Result<(), CoreError> {
        let runtime = runtime_config().compile()?;
        assert_eq!(runtime.summary().upstream_count, 1);
        let plan = runtime.plan_dispatch(&DispatchRequest {
            publisher_id: "local-ai".to_owned(),
            request: canonical_request(),
            health: BTreeMap::from([("openai-primary".to_owned(), HealthStatus::Healthy)]),
            excluded_upstreams: HashSet::new(),
            previous_error: None,
        })?;
        assert_eq!(plan.public_model, "smart");
        assert_eq!(plan.upstream_model, "upstream-model");
        assert_eq!(plan.transport.body["model"], "upstream-model");
        let serialized = serde_json::to_string(&plan)?;
        assert!(serialized.contains("secret://workspace/openai/main"));
        assert!(!serialized.contains("sk-"));
        Ok(())
    }

    #[test]
    fn compile_collects_cross_reference_errors() {
        let mut config = runtime_config();
        config
            .publishers
            .get_mut("local-ai")
            .expect("publisher")
            .routes[0]
            .upstreams[0]
            .provider_instance = "missing".to_owned();
        let error = config.compile().expect_err("missing provider must fail");
        assert_eq!(error.code, ErrorCode::RuntimeConfigInvalid);
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.code == "PROVIDER_NOT_FOUND")
        );
    }
}
