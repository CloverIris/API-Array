use apiarray_core::canonical::{CanonicalRequest, Message, ResponseFormat, Role, ToolChoice};
use apiarray_core::provider::ProviderManifest;
use apiarray_core::publisher::PublisherConfig;
use apiarray_core::routing::{HealthStatus, RoutePolicy, StandardError};
use apiarray_core::runtime::{
    DispatchRequest, ModelRoute, ProviderInstance, RouteCondition, RuntimeConfig, RuntimePublisher,
    UpstreamRoute,
};
use apiarray_core::secret::SecretRef;
use std::collections::{BTreeMap, HashSet};

fn provider_instance(id: &str, endpoint: &str) -> ProviderInstance {
    ProviderInstance {
        id: id.to_owned(),
        manifest: ProviderManifest::from_yaml(include_str!("../../../providers/openai.yaml"))
            .expect("built-in provider must be valid"),
        endpoint_override: Some(endpoint.to_owned()),
        secret_refs: BTreeMap::from([(
            "api_key".to_owned(),
            SecretRef::parse(format!("secret://workspace/{id}/api-key")).expect("valid secret ref"),
        )]),
        enabled: true,
    }
}

fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        schema_version: 1,
        id: "desktop-runtime".to_owned(),
        providers: BTreeMap::from([
            (
                "primary".to_owned(),
                provider_instance("primary", "https://primary.invalid/v1"),
            ),
            (
                "backup".to_owned(),
                provider_instance("backup", "https://backup.invalid/v1"),
            ),
        ]),
        publishers: BTreeMap::from([(
            "local-ai".to_owned(),
            RuntimePublisher {
                config: PublisherConfig {
                    schema_version: 1,
                    id: "local-ai".to_owned(),
                    name: "Local AI".to_owned(),
                    listen_address: "127.0.0.1".parse().expect("valid loopback"),
                    port: 6188,
                    base_path: "/v1".to_owned(),
                    require_token: true,
                    token_ref: Some(
                        SecretRef::parse("secret://publisher/local-ai").expect("valid secret ref"),
                    ),
                },
                routes: vec![ModelRoute {
                    public_model: "smart".to_owned(),
                    policy: RoutePolicy {
                        schema_version: 1,
                        id: "smart-ha".to_owned(),
                        timeout_ms: 30_000,
                        max_retries: 2,
                        failover_on: HashSet::from([
                            StandardError::ProviderTimeout,
                            StandardError::RateLimited,
                        ]),
                    },
                    upstreams: vec![
                        UpstreamRoute {
                            id: "primary-route".to_owned(),
                            provider_instance: "primary".to_owned(),
                            upstream_model: "primary-model".to_owned(),
                            priority: 0,
                            enabled: true,
                            conditions: Vec::new(),
                        },
                        UpstreamRoute {
                            id: "backup-route".to_owned(),
                            provider_instance: "backup".to_owned(),
                            upstream_model: "backup-model".to_owned(),
                            priority: 10,
                            enabled: true,
                            conditions: Vec::new(),
                        },
                    ],
                }],
            },
        )]),
    }
}

fn request() -> CanonicalRequest {
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
fn unhealthy_primary_routes_to_healthy_backup() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = runtime_config().compile()?;
    let plan = runtime.plan_dispatch(&DispatchRequest {
        publisher_id: "local-ai".to_owned(),
        request: request(),
        health: BTreeMap::from([
            ("primary-route".to_owned(), HealthStatus::Unhealthy),
            ("backup-route".to_owned(), HealthStatus::Healthy),
        ]),
        excluded_upstreams: HashSet::new(),
        previous_error: None,
    })?;

    assert_eq!(plan.route.candidate_id, "backup-route");
    assert_eq!(plan.provider_instance, "backup");
    assert_eq!(plan.upstream_model, "backup-model");
    assert_eq!(plan.transport.body["model"], "backup-model");
    assert_eq!(
        plan.transport.url,
        "https://backup.invalid/v1/chat/completions"
    );
    Ok(())
}

#[test]
fn previous_timeout_and_exclusion_produce_explicit_failover()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = runtime_config().compile()?;
    let plan = runtime.plan_dispatch(&DispatchRequest {
        publisher_id: "local-ai".to_owned(),
        request: request(),
        health: BTreeMap::from([
            ("primary-route".to_owned(), HealthStatus::Healthy),
            ("backup-route".to_owned(), HealthStatus::Healthy),
        ]),
        excluded_upstreams: HashSet::from(["primary-route".to_owned()]),
        previous_error: Some(StandardError::ProviderTimeout),
    })?;
    assert_eq!(plan.route.candidate_id, "backup-route");
    assert_eq!(format!("{:?}", plan.route.reason), "FailoverAfterError");
    Ok(())
}

#[test]
fn compile_rejects_missing_required_secret() {
    let mut config = runtime_config();
    config
        .providers
        .get_mut("primary")
        .expect("provider")
        .secret_refs
        .clear();
    let error = config.compile().expect_err("missing secret must fail");
    assert!(
        error
            .issues
            .iter()
            .any(|issue| issue.code == "SECRET_REFERENCE_REQUIRED")
    );
}

#[test]
fn metadata_condition_selects_only_matching_upstream() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = runtime_config();
    config
        .publishers
        .get_mut("local-ai")
        .expect("publisher")
        .routes[0]
        .upstreams[0]
        .conditions = vec![RouteCondition::MetadataEquals {
        key: "tier".to_owned(),
        value: "premium".to_owned(),
    }];
    let runtime = config.compile()?;

    let without_metadata = runtime.plan_dispatch(&DispatchRequest {
        publisher_id: "local-ai".to_owned(),
        request: request(),
        health: BTreeMap::new(),
        excluded_upstreams: HashSet::new(),
        previous_error: None,
    })?;
    assert_eq!(without_metadata.route.candidate_id, "backup-route");

    let mut matching = request();
    matching
        .metadata
        .insert("tier".to_owned(), "premium".to_owned());
    let with_metadata = runtime.plan_dispatch(&DispatchRequest {
        publisher_id: "local-ai".to_owned(),
        request: matching,
        health: BTreeMap::new(),
        excluded_upstreams: HashSet::new(),
        previous_error: None,
    })?;
    assert_eq!(with_metadata.route.candidate_id, "primary-route");
    Ok(())
}
