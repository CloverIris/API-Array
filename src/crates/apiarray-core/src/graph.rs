use crate::routing::{SelectionStrategy, StandardError};
use crate::runtime::{ModelRoute, RuntimeConfig, UpstreamRoute};
use crate::workspace::{ApiWallet, WorkspacePackage};
use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const GRAPH_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub schema_version: u32,
    pub id: String,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
    pub config: NodeConfig,
}

impl Node {
    #[must_use]
    pub fn provider_config(&self) -> Option<&ProviderConfig> {
        match &self.config {
            NodeConfig::Provider(config) => Some(config),
            _ => None,
        }
    }

    #[must_use]
    pub fn provider_config_mut(&mut self) -> Option<&mut ProviderConfig> {
        match &mut self.config {
            NodeConfig::Provider(config) => Some(config),
            _ => None,
        }
    }

    #[must_use]
    pub fn composer_config_mut(&mut self) -> Option<&mut ComposerConfig> {
        match &mut self.config {
            NodeConfig::Composer(config) => Some(config),
            _ => None,
        }
    }

    #[must_use]
    pub fn publisher_config_mut(&mut self) -> Option<&mut PublisherNodeConfig> {
        match &mut self.config {
            NodeConfig::Publisher(config) => Some(config),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Provider,
    Composer,
    Middleware,
    Probe,
    Publisher,
    Group,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeConfig {
    Provider(ProviderConfig),
    Composer(ComposerConfig),
    Middleware(MiddlewareConfig),
    Probe(ProbeConfig),
    Publisher(PublisherNodeConfig),
    Group(GroupConfig),
}

impl NodeConfig {
    #[must_use]
    pub const fn kind(&self) -> NodeKind {
        match self {
            Self::Provider(_) => NodeKind::Provider,
            Self::Composer(_) => NodeKind::Composer,
            Self::Middleware(_) => NodeKind::Middleware,
            Self::Probe(_) => NodeKind::Probe,
            Self::Publisher(_) => NodeKind::Publisher,
            Self::Group(_) => NodeKind::Group,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub asset_id: String,
    #[serde(default = "default_models")]
    pub selected_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerConfig {
    #[serde(default)]
    pub strategy: SelectionStrategy,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_failover_errors")]
    pub failover_on: HashSet<StandardError>,
    #[serde(default)]
    pub allow_capability_degradation: bool,
    #[serde(default = "default_latency_hysteresis_ms")]
    pub latency_hysteresis_ms: u64,
    #[serde(default)]
    pub routes: Vec<PublicModelRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicModelRoute {
    pub public_model: String,
    #[serde(default)]
    pub candidates: Vec<CandidateBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateBinding {
    pub provider_node_id: String,
    pub upstream_model: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_weight")]
    pub weight: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiddlewareConfig {
    pub middleware: MiddlewareKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MiddlewareKind {
    RequestDefaults {
        #[serde(default)]
        temperature: Option<f64>,
        #[serde(default)]
        top_p: Option<f64>,
        #[serde(default)]
        max_output_tokens: Option<u32>,
    },
    ModelPolicy {
        #[serde(default)]
        allowed_models: Vec<String>,
        #[serde(default)]
        max_output_tokens: Option<u32>,
    },
    RateLimit {
        #[serde(default = "default_rate_limit")]
        requests_per_minute: u32,
        #[serde(default = "default_concurrency")]
        max_concurrent: u32,
    },
    BudgetMonitor {
        #[serde(default = "default_budget_thresholds")]
        warning_thresholds: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeConfig {
    #[serde(default)]
    pub provider_node_id: String,
    #[serde(default = "default_probe_interval")]
    pub interval_seconds: u64,
    #[serde(default = "default_probe_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_probe_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_probe_recovery_threshold")]
    pub recovery_threshold: u32,
    #[serde(default = "default_true")]
    pub safe_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PublisherNodeConfig {
    #[serde(default)]
    pub publisher_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GroupConfig {
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub id: String,
    pub data_type: PortType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Candidate,
    ServicePlan,
    HealthSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub from: Endpoint,
    pub to: Endpoint,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub enabled_node_count: usize,
    pub publisher_count: usize,
    pub topological_order: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeImpact {
    pub node_id: String,
    pub downstream_nodes: Vec<String>,
    pub affected_publishers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilationIssue {
    pub code: String,
    pub severity: IssueSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_target: Option<String>,
}

impl CompilationIssue {
    fn error(code: &str, message: impl Into<String>, node_id: Option<&str>) -> Self {
        Self {
            code: code.to_owned(),
            severity: IssueSeverity::Error,
            message: message.into(),
            node_id: node_id.map(ToOwned::to_owned),
            edge_id: None,
            field: None,
            fix_target: node_id.map(|id| format!("node:{id}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCandidate {
    pub provider_node_id: String,
    pub asset_id: String,
    pub public_model: String,
    pub upstream_model: String,
    pub priority: u32,
    pub weight: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasCompilationReport {
    pub valid: bool,
    pub public_models: Vec<String>,
    pub candidates: Vec<CompiledCandidate>,
    pub candidate_count: usize,
    pub selected_strategy: SelectionStrategy,
    pub middleware_order: Vec<String>,
    pub unreachable_nodes: Vec<String>,
    pub missing_secrets: Vec<String>,
    pub stale_runtime: bool,
    pub requires_confirmation: bool,
    pub warnings: Vec<CompilationIssue>,
    pub errors: Vec<CompilationIssue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledCanvasGraph {
    pub routes: Vec<ModelRoute>,
    pub middleware: Vec<MiddlewareConfig>,
    pub probes: Vec<ProbeConfig>,
    pub report: CanvasCompilationReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationInput {
    pub public_model: String,
    #[serde(default)]
    pub unavailable_provider_node_ids: BTreeSet<String>,
    #[serde(default)]
    pub latency_ms_by_provider_node: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationResult {
    pub public_model: String,
    pub strategy: SelectionStrategy,
    pub selected_provider_node_id: Option<String>,
    pub selected_upstream_model: Option<String>,
    pub ordered_candidates: Vec<String>,
    pub explanation: String,
}

/// Simulates a single routing decision without resolving secrets or issuing a
/// network request. Weighted routing reports the next deterministic preview
/// candidate; the live smooth-round-robin cursor remains runtime-owned.
pub fn simulate_route(
    compiled: &CompiledCanvasGraph,
    input: &RouteSimulationInput,
) -> Result<RouteSimulationResult, CoreError> {
    let mut candidates = compiled
        .report
        .candidates
        .iter()
        .filter(|candidate| candidate.public_model == input.public_model)
        .filter(|candidate| {
            !input
                .unavailable_provider_node_ids
                .contains(&candidate.provider_node_id)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(RouteSimulationResult {
            public_model: input.public_model.clone(),
            strategy: compiled.report.selected_strategy,
            selected_provider_node_id: None,
            selected_upstream_model: None,
            ordered_candidates: Vec::new(),
            explanation: "没有满足当前健康条件的候选 Provider。".to_owned(),
        });
    }
    match compiled.report.selected_strategy {
        SelectionStrategy::PriorityFailover => candidates
            .sort_by_key(|candidate| (candidate.priority, candidate.provider_node_id.clone())),
        SelectionStrategy::WeightedRoundRobin => candidates.sort_by_key(|candidate| {
            (
                std::cmp::Reverse(candidate.weight),
                candidate.priority,
                candidate.provider_node_id.clone(),
            )
        }),
        SelectionStrategy::LowestLatency => candidates.sort_by_key(|candidate| {
            (
                input
                    .latency_ms_by_provider_node
                    .get(&candidate.provider_node_id)
                    .copied()
                    .unwrap_or(u64::MAX),
                candidate.priority,
                candidate.provider_node_id.clone(),
            )
        }),
    }
    let selected = candidates.first().copied();
    let explanation = match compiled.report.selected_strategy {
        SelectionStrategy::PriorityFailover => "按健康过滤、优先级和稳定 ID 选择。",
        SelectionStrategy::WeightedRoundRobin => {
            "按权重预览下一候选；实时平滑轮询游标由 Runtime 独立维护。"
        }
        SelectionStrategy::LowestLatency
            if selected.is_some_and(|candidate| {
                input
                    .latency_ms_by_provider_node
                    .contains_key(&candidate.provider_node_id)
            }) =>
        {
            "按可用延迟样本选择最低延迟候选。"
        }
        SelectionStrategy::LowestLatency => "延迟样本不足，回退到优先级顺序。",
    };
    Ok(RouteSimulationResult {
        public_model: input.public_model.clone(),
        strategy: compiled.report.selected_strategy,
        selected_provider_node_id: selected.map(|candidate| candidate.provider_node_id.clone()),
        selected_upstream_model: selected.map(|candidate| candidate.upstream_model.clone()),
        ordered_candidates: candidates
            .into_iter()
            .map(|candidate| candidate.provider_node_id.clone())
            .collect(),
        explanation: explanation.to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDefinition {
    pub kind: NodeKind,
    pub label: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphTemplateKind {
    SingleProvider,
    PriorityFailover,
    MultiModel,
    WeightedRoundRobin,
    LowestLatency,
    FailoverWithProbe,
    GuardedFailover,
}

impl GraphTemplateKind {
    #[must_use]
    pub const fn required_assets(self) -> usize {
        match self {
            Self::SingleProvider => 1,
            Self::PriorityFailover
            | Self::MultiModel
            | Self::WeightedRoundRobin
            | Self::LowestLatency
            | Self::FailoverWithProbe
            | Self::GuardedFailover => 2,
        }
    }
}

#[must_use]
pub fn node_catalog() -> Vec<NodeDefinition> {
    [
        (NodeKind::Provider, "钱包 Provider", "引用一个 API 钱包资产"),
        (NodeKind::Composer, "Composer", "合并模型并执行候选选择策略"),
        (
            NodeKind::Middleware,
            "Middleware",
            "执行受控限流、默认值或预算策略",
        ),
        (NodeKind::Probe, "Probe", "执行无副作用健康探测"),
        (
            NodeKind::Publisher,
            "Publisher",
            "编组方案唯一 OpenAI-compatible 出口",
        ),
        (NodeKind::Group, "Group", "仅用于视觉组织"),
    ]
    .into_iter()
    .map(|(kind, label, description)| {
        let (inputs, outputs) = ports_for(kind);
        NodeDefinition {
            kind,
            label: label.to_owned(),
            description: description.to_owned(),
            inputs,
            outputs,
        }
    })
    .collect()
}

#[must_use]
pub fn new_node(id: impl Into<String>, name: impl Into<String>, kind: NodeKind) -> Node {
    let (inputs, outputs) = ports_for(kind);
    Node {
        id: id.into(),
        name: name.into(),
        kind,
        enabled: true,
        inputs,
        outputs,
        config: default_config(kind),
    }
}

#[must_use]
pub fn new_canvas_graph(id: &str) -> WorkflowGraph {
    let composer = new_node("composer", "Composer", NodeKind::Composer);
    let publisher = new_node("total-output", "本地总输出", NodeKind::Publisher);
    WorkflowGraph {
        schema_version: GRAPH_SCHEMA_VERSION,
        id: id.to_owned(),
        nodes: vec![composer, publisher],
        edges: vec![Edge {
            id: "composer-to-output".to_owned(),
            from: Endpoint {
                node: "composer".to_owned(),
                port: "service_plan_out".to_owned(),
            },
            to: Endpoint {
                node: "total-output".to_owned(),
                port: "service_plan_in".to_owned(),
            },
            enabled: true,
            label: None,
        }],
    }
}

/// Generates a deterministic Graph V3 draft from wallet asset identities.
/// Secret values and publisher tokens are deliberately not accepted by this API.
pub fn generate_graph_template(
    graph_id: &str,
    template: GraphTemplateKind,
    assets: &[(String, String)],
) -> Result<WorkflowGraph, CoreError> {
    if assets.len() < template.required_assets() {
        return Err(CoreError::new(
            ErrorCode::GraphInvalid,
            format!("模板至少需要 {} 个钱包资产", template.required_assets()),
        ));
    }
    let selected = &assets[..template.required_assets()];
    let mut graph = new_canvas_graph(graph_id);
    let composer_index = graph
        .nodes
        .iter()
        .position(|node| node.kind == NodeKind::Composer)
        .expect("default graph has composer");
    let composer_id = graph.nodes[composer_index].id.clone();
    let publisher_id = graph
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Publisher)
        .expect("default graph has publisher")
        .id
        .clone();
    let mut providers = Vec::new();
    for (index, (asset_id, name)) in selected.iter().enumerate() {
        let mut provider = new_node(format!("provider-{}", index + 1), name, NodeKind::Provider);
        provider.config = NodeConfig::Provider(ProviderConfig {
            asset_id: asset_id.clone(),
            selected_models: vec![if template == GraphTemplateKind::MultiModel {
                format!("model-{}", index + 1)
            } else {
                "default".to_owned()
            }],
        });
        graph.edges.push(Edge {
            id: format!("provider-{}-candidate", index + 1),
            from: Endpoint {
                node: provider.id.clone(),
                port: "candidate_out".to_owned(),
            },
            to: Endpoint {
                node: composer_id.clone(),
                port: "candidate_in".to_owned(),
            },
            enabled: true,
            label: None,
        });
        providers.push(provider);
    }
    let strategy = match template {
        GraphTemplateKind::WeightedRoundRobin => SelectionStrategy::WeightedRoundRobin,
        GraphTemplateKind::LowestLatency => SelectionStrategy::LowestLatency,
        _ => SelectionStrategy::PriorityFailover,
    };
    let routes = if template == GraphTemplateKind::MultiModel {
        providers
            .iter()
            .enumerate()
            .map(|(index, provider)| PublicModelRoute {
                public_model: format!("model-{}", index + 1),
                candidates: vec![CandidateBinding {
                    provider_node_id: provider.id.clone(),
                    upstream_model: format!("model-{}", index + 1),
                    priority: 0,
                    weight: 1,
                }],
            })
            .collect()
    } else {
        vec![PublicModelRoute {
            public_model: "default".to_owned(),
            candidates: providers
                .iter()
                .enumerate()
                .map(|(index, provider)| CandidateBinding {
                    provider_node_id: provider.id.clone(),
                    upstream_model: "default".to_owned(),
                    priority: u32::try_from(index).unwrap_or(u32::MAX),
                    weight: if template == GraphTemplateKind::WeightedRoundRobin && index == 0 {
                        70
                    } else if template == GraphTemplateKind::WeightedRoundRobin {
                        30
                    } else {
                        1
                    },
                })
                .collect(),
        }]
    };
    graph.nodes[composer_index].config = NodeConfig::Composer(ComposerConfig {
        strategy,
        routes,
        ..default_composer_config()
    });

    if matches!(
        template,
        GraphTemplateKind::FailoverWithProbe
            | GraphTemplateKind::GuardedFailover
            | GraphTemplateKind::LowestLatency
    ) {
        for (index, provider) in providers.iter().enumerate() {
            let mut probe = new_node(
                format!("probe-{}", index + 1),
                format!("{} 健康探测", provider.name),
                NodeKind::Probe,
            );
            probe.config = NodeConfig::Probe(ProbeConfig {
                provider_node_id: provider.id.clone(),
                ..default_probe_config()
            });
            graph.edges.push(Edge {
                id: format!("probe-{}-health", index + 1),
                from: Endpoint {
                    node: probe.id.clone(),
                    port: "health_out".to_owned(),
                },
                to: Endpoint {
                    node: composer_id.clone(),
                    port: "health_in".to_owned(),
                },
                enabled: true,
                label: Some("健康滞回".to_owned()),
            });
            graph.nodes.push(probe);
        }
    }

    if template == GraphTemplateKind::GuardedFailover {
        graph.edges.retain(|edge| edge.id != "composer-to-output");
        let mut rate_limit = new_node("middleware-rate-limit", "入口限流", NodeKind::Middleware);
        rate_limit.config = NodeConfig::Middleware(MiddlewareConfig {
            middleware: MiddlewareKind::RateLimit {
                requests_per_minute: 60,
                max_concurrent: 8,
            },
        });
        let mut budget = new_node("middleware-budget", "预算预警", NodeKind::Middleware);
        budget.config = NodeConfig::Middleware(MiddlewareConfig {
            middleware: MiddlewareKind::BudgetMonitor {
                warning_thresholds: vec![50, 80, 100],
            },
        });
        graph.edges.extend([
            Edge {
                id: "composer-to-rate-limit".to_owned(),
                from: Endpoint {
                    node: composer_id.clone(),
                    port: "service_plan_out".to_owned(),
                },
                to: Endpoint {
                    node: rate_limit.id.clone(),
                    port: "service_plan_in".to_owned(),
                },
                enabled: true,
                label: None,
            },
            Edge {
                id: "rate-limit-to-budget".to_owned(),
                from: Endpoint {
                    node: rate_limit.id.clone(),
                    port: "service_plan_out".to_owned(),
                },
                to: Endpoint {
                    node: budget.id.clone(),
                    port: "service_plan_in".to_owned(),
                },
                enabled: true,
                label: None,
            },
            Edge {
                id: "budget-to-publisher".to_owned(),
                from: Endpoint {
                    node: budget.id.clone(),
                    port: "service_plan_out".to_owned(),
                },
                to: Endpoint {
                    node: publisher_id,
                    port: "service_plan_in".to_owned(),
                },
                enabled: true,
                label: None,
            },
        ]);
        graph.nodes.extend([rate_limit, budget]);
    }
    graph.nodes.splice(0..0, providers);
    graph.validate()?;
    Ok(graph)
}

pub fn validate_runtime_semantics(
    workspace: &WorkspacePackage,
    project_id: &str,
    canvas_id: &str,
) -> Result<CanvasCompilationReport, CoreError> {
    let canvas = workspace
        .projects
        .projects
        .get(project_id)
        .and_then(|project| project.canvases.get(canvas_id))
        .ok_or_else(|| CoreError::new(ErrorCode::GraphInvalid, "编组方案不存在或不属于指定项目"))?;
    let mut report =
        compile_canvas_graph(&canvas.graph, &workspace.wallet, &workspace.runtime)?.report;
    report.stale_runtime = canvas.draft_revision != canvas.applied_revision;
    Ok(report)
}

#[allow(clippy::too_many_lines)]
pub fn compile_canvas_graph(
    graph: &WorkflowGraph,
    wallet: &ApiWallet,
    runtime: &RuntimeConfig,
) -> Result<CompiledCanvasGraph, CoreError> {
    graph.validate()?;
    let composer_nodes = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Composer)
        .collect::<Vec<_>>();
    let publisher_nodes = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Publisher)
        .collect::<Vec<_>>();
    let providers = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Provider && node.enabled)
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    let warnings: Vec<CompilationIssue> = Vec::new();
    if composer_nodes.len() != 1 {
        errors.push(CompilationIssue::error(
            "COMPOSER_COUNT",
            "每个编组方案必须且只能包含一个 Composer。",
            None,
        ));
    }
    if publisher_nodes.len() != 1 {
        errors.push(CompilationIssue::error(
            "PUBLISHER_COUNT",
            "每个编组方案必须且只能包含一个 Publisher。",
            None,
        ));
    }
    if providers.is_empty() {
        errors.push(CompilationIssue::error(
            "NO_PROVIDER",
            "Composer 至少需要一个启用的钱包 Provider。",
            None,
        ));
    }
    let composer_node = composer_nodes.first().copied();
    let composer = composer_node.and_then(|node| match &node.config {
        NodeConfig::Composer(config) => Some(config),
        _ => None,
    });
    let composer_id = composer_node.map(|node| node.id.as_str());
    if let Some(composer_id) = composer_id {
        for provider in &providers {
            if !graph.edges.iter().any(|edge| {
                edge.enabled && edge.from.node == provider.id && edge.to.node == composer_id
            }) {
                errors.push(CompilationIssue::error(
                    "PROVIDER_NOT_CONNECTED",
                    format!("Provider {} 尚未连接 Composer。", provider.name),
                    Some(&provider.id),
                ));
            }
        }
    }

    let mut provider_assets = BTreeMap::new();
    let mut provider_instances = BTreeMap::new();
    let mut missing_secrets = Vec::new();
    let mut asset_refs = HashSet::new();
    for node in &providers {
        let NodeConfig::Provider(config) = &node.config else {
            continue;
        };
        if config.asset_id.trim().is_empty() {
            errors.push(CompilationIssue::error(
                "PROVIDER_ASSET_REQUIRED",
                format!("Provider {} 未绑定钱包资产。", node.name),
                Some(&node.id),
            ));
            continue;
        }
        if !asset_refs.insert(config.asset_id.clone()) {
            errors.push(CompilationIssue::error(
                "DUPLICATE_ASSET",
                format!("钱包资产 {} 在同一编组方案中被重复引用。", config.asset_id),
                Some(&node.id),
            ));
        }
        let Some(asset) = wallet.assets.get(&config.asset_id) else {
            errors.push(CompilationIssue::error(
                "ASSET_NOT_FOUND",
                format!("Provider {} 引用了不存在的钱包资产。", node.name),
                Some(&node.id),
            ));
            continue;
        };
        let Some(instance) = runtime.providers.get(&asset.provider_instance_id) else {
            errors.push(CompilationIssue::error(
                "PROVIDER_INSTANCE_NOT_FOUND",
                format!("钱包资产 {} 缺少 Provider 实例。", asset.name),
                Some(&node.id),
            ));
            continue;
        };
        if !asset.enabled || !instance.enabled {
            errors.push(CompilationIssue::error(
                "PROVIDER_DISABLED",
                format!("钱包资产 {} 或其 Provider 实例已停用。", asset.name),
                Some(&node.id),
            ));
        }
        if instance.secret_refs.is_empty() {
            missing_secrets.push(config.asset_id.clone());
            errors.push(CompilationIssue::error(
                "SECRET_MISSING",
                format!("钱包资产 {} 缺少 Secret。", asset.name),
                Some(&node.id),
            ));
        }
        provider_assets.insert(node.id.clone(), (config, asset));
        provider_instances.insert(node.id.clone(), instance);
    }

    let composer_config = composer.cloned().unwrap_or_else(default_composer_config);
    let route_specs = if composer_config.routes.is_empty() {
        providers
            .iter()
            .flat_map(|node| {
                let NodeConfig::Provider(config) = &node.config else {
                    return Vec::new();
                };
                config
                    .selected_models
                    .iter()
                    .map(|model| PublicModelRoute {
                        public_model: model.clone(),
                        candidates: vec![CandidateBinding {
                            provider_node_id: node.id.clone(),
                            upstream_model: model.clone(),
                            priority: 0,
                            weight: 1,
                        }],
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    } else {
        composer_config.routes.clone()
    };

    let mut routes = Vec::new();
    let mut compiled_candidates = Vec::new();
    let mut public_names = HashSet::new();
    for route in route_specs {
        if route.public_model.trim().is_empty() || !public_names.insert(route.public_model.clone())
        {
            errors.push(CompilationIssue::error(
                "INVALID_PUBLIC_MODEL",
                "公开模型名不能为空且必须唯一。",
                composer_id,
            ));
            continue;
        }
        if route.candidates.is_empty() {
            errors.push(CompilationIssue::error(
                "ROUTE_WITHOUT_CANDIDATE",
                format!("公开模型 {} 没有候选上游。", route.public_model),
                composer_id,
            ));
            continue;
        }
        let mut upstreams = Vec::new();
        for candidate in route.candidates {
            if !(1..=100).contains(&candidate.weight) {
                errors.push(CompilationIssue::error(
                    "WEIGHT_OUT_OF_RANGE",
                    "候选权重必须在 1 到 100 之间。",
                    Some(&candidate.provider_node_id),
                ));
                continue;
            }
            let Some((provider_config, asset)) = provider_assets.get(&candidate.provider_node_id)
            else {
                errors.push(CompilationIssue::error(
                    "CANDIDATE_PROVIDER_NOT_FOUND",
                    format!("公开模型 {} 引用了无效 Provider 节点。", route.public_model),
                    Some(&candidate.provider_node_id),
                ));
                continue;
            };
            let Some(_instance) = provider_instances.get(&candidate.provider_node_id) else {
                continue;
            };
            let upstream_model = if candidate.upstream_model.trim().is_empty() {
                route.public_model.clone()
            } else {
                candidate.upstream_model.clone()
            };
            upstreams.push(UpstreamRoute {
                id: format!("{}:{}", candidate.provider_node_id, route.public_model),
                provider_instance: asset.provider_instance_id.clone(),
                upstream_model: upstream_model.clone(),
                priority: candidate.priority,
                weight: candidate.weight,
                enabled: true,
                conditions: Vec::new(),
                billing: asset.billing.clone(),
            });
            compiled_candidates.push(CompiledCandidate {
                provider_node_id: candidate.provider_node_id,
                asset_id: provider_config.asset_id.clone(),
                public_model: route.public_model.clone(),
                upstream_model,
                priority: candidate.priority,
                weight: candidate.weight,
            });
        }
        upstreams.sort_by_key(|candidate| (candidate.priority, candidate.id.clone()));
        // Different provider protocols are expected here. Every candidate is
        // normalized through its Canonical Adapter before Composer selection;
        // protocol identity alone is therefore not a capability conflict.
        // Verified capability differences are surfaced by inspection data at
        // the desktop/runtime boundary and require an explicit degradation
        // acknowledgement there.
        routes.push(ModelRoute {
            public_model: route.public_model,
            policy: crate::routing::RoutePolicy {
                schema_version: SCHEMA_VERSION,
                id: format!("{}-route-policy", graph.id),
                timeout_ms: composer_config.timeout_ms,
                max_retries: composer_config.max_retries,
                failover_on: composer_config.failover_on.clone(),
                selection_strategy: composer_config.strategy,
                latency_hysteresis_ms: composer_config.latency_hysteresis_ms,
            },
            upstreams,
        });
    }

    let unreachable_nodes = enabled_unreachable_nodes(graph);
    for node_id in &unreachable_nodes {
        errors.push(CompilationIssue::error(
            "UNREACHABLE_NODE",
            "启用节点无法到达 Publisher。",
            Some(node_id),
        ));
    }
    validate_service_plan_chain(graph, &mut errors);
    let probes = validate_probes(graph, &mut errors);
    let middleware_order = middleware_order(graph);
    let middleware = middleware_order
        .iter()
        .filter_map(|id| graph.nodes.iter().find(|node| node.id == *id))
        .filter_map(|node| match &node.config {
            NodeConfig::Middleware(config) => Some(config.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let public_models = routes
        .iter()
        .map(|route| route.public_model.clone())
        .collect::<Vec<_>>();
    let requires_confirmation = warnings
        .iter()
        .any(|issue| issue.code == "CAPABILITY_DEGRADATION");
    Ok(CompiledCanvasGraph {
        routes,
        middleware,
        probes,
        report: CanvasCompilationReport {
            valid: errors.is_empty(),
            public_models,
            candidate_count: compiled_candidates.len(),
            candidates: compiled_candidates,
            selected_strategy: composer_config.strategy,
            middleware_order,
            unreachable_nodes,
            missing_secrets,
            stale_runtime: false,
            requires_confirmation,
            warnings,
            errors,
        },
    })
}

fn validate_service_plan_chain(graph: &WorkflowGraph, issues: &mut Vec<CompilationIssue>) {
    let service_edges = graph
        .edges
        .iter()
        .filter(|edge| edge.enabled && edge_port_type(graph, edge) == Some(PortType::ServicePlan))
        .collect::<Vec<_>>();
    for node in graph.nodes.iter().filter(|node| node.enabled) {
        let incoming = service_edges
            .iter()
            .filter(|edge| edge.to.node == node.id)
            .count();
        let outgoing = service_edges
            .iter()
            .filter(|edge| edge.from.node == node.id)
            .count();
        match node.kind {
            NodeKind::Composer if outgoing != 1 => issues.push(CompilationIssue::error(
                "SERVICE_PLAN_BRANCH",
                "Composer 必须且只能输出一条 ServicePlan。",
                Some(&node.id),
            )),
            NodeKind::Middleware if incoming != 1 || outgoing != 1 => {
                issues.push(CompilationIssue::error(
                    "MIDDLEWARE_CHAIN",
                    "Middleware 必须位于单一 ServicePlan 链中。",
                    Some(&node.id),
                ))
            }
            NodeKind::Publisher if incoming != 1 => issues.push(CompilationIssue::error(
                "PUBLISHER_INPUT",
                "Publisher 必须且只能接收一条 ServicePlan。",
                Some(&node.id),
            )),
            _ => {}
        }
    }
}

fn validate_probes(graph: &WorkflowGraph, issues: &mut Vec<CompilationIssue>) -> Vec<ProbeConfig> {
    let provider_ids = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Provider)
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Probe && node.enabled)
        .filter_map(|node| {
            let NodeConfig::Probe(config) = &node.config else {
                return None;
            };
            if config.provider_node_id.is_empty()
                || !provider_ids.contains(config.provider_node_id.as_str())
            {
                issues.push(CompilationIssue::error(
                    "PROBE_BINDING",
                    "Probe 必须绑定一个存在的 Provider 节点。",
                    Some(&node.id),
                ));
            }
            if !(30..=86_400).contains(&config.interval_seconds)
                || !(1_000..=60_000).contains(&config.timeout_ms)
                || config.failure_threshold == 0
                || config.recovery_threshold == 0
            {
                issues.push(CompilationIssue::error(
                    "PROBE_RANGE",
                    "Probe 周期、超时和阈值超出安全范围。",
                    Some(&node.id),
                ));
            }
            Some(config.clone())
        })
        .collect()
}

fn middleware_order(graph: &WorkflowGraph) -> Vec<String> {
    topological_order(&graph.nodes, &graph.edges)
        .into_iter()
        .filter(|id| {
            graph
                .nodes
                .iter()
                .any(|node| node.id == *id && node.enabled && node.kind == NodeKind::Middleware)
        })
        .collect()
}

impl WorkflowGraph {
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<GraphSummary, CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != GRAPH_SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "编组图 Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new("id", "REQUIRED", "编组图 ID 不能为空"));
        }
        let mut nodes = HashMap::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if node.id.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("nodes[{index}].id"),
                    "REQUIRED",
                    "节点 ID 不能为空",
                ));
            } else if nodes.insert(node.id.as_str(), node).is_some() {
                issues.push(ValidationIssue::new(
                    format!("nodes[{index}].id"),
                    "DUPLICATE",
                    "节点 ID 必须唯一",
                ));
            }
            validate_ports(index, "inputs", &node.inputs, &mut issues);
            validate_ports(index, "outputs", &node.outputs, &mut issues);
            validate_node_contract(index, node, &mut issues);
            validate_node_config(index, node, &mut issues);
        }
        let mut edge_ids = HashSet::new();
        let mut edge_connections = HashSet::new();
        for (index, edge) in self.edges.iter().enumerate() {
            if edge.id.trim().is_empty() || !edge_ids.insert(&edge.id) {
                issues.push(ValidationIssue::new(
                    format!("edges[{index}].id"),
                    "DUPLICATE",
                    "连接 ID 不能为空且必须唯一",
                ));
            }
            if edge.enabled
                && !edge_connections.insert((
                    edge.from.node.as_str(),
                    edge.from.port.as_str(),
                    edge.to.node.as_str(),
                    edge.to.port.as_str(),
                ))
            {
                issues.push(ValidationIssue::new(
                    format!("edges[{index}]"),
                    "DUPLICATE_CONNECTION",
                    "同一对端口之间不允许重复连接",
                ));
            }
            let from_node = nodes.get(edge.from.node.as_str());
            let to_node = nodes.get(edge.to.node.as_str());
            if edge.from.node == edge.to.node {
                issues.push(ValidationIssue::new(
                    format!("edges[{index}]"),
                    "SELF_LOOP",
                    "节点不能连接到自身",
                ));
            }
            match (from_node, to_node) {
                (Some(from), Some(to)) => {
                    let output = from.outputs.iter().find(|port| port.id == edge.from.port);
                    let input = to.inputs.iter().find(|port| port.id == edge.to.port);
                    match (output, input) {
                        (Some(output), Some(input))
                            if output.data_type == input.data_type
                                && valid_connection(from.kind, to.kind, output.data_type) => {}
                        (Some(_), Some(_)) => issues.push(ValidationIssue::new(
                            format!("edges[{index}]"),
                            "PORT_TYPE_MISMATCH",
                            "连接两端端口不兼容",
                        )),
                        (None, _) => issues.push(ValidationIssue::new(
                            format!("edges[{index}].from.port"),
                            "PORT_NOT_FOUND",
                            "源输出端口不存在",
                        )),
                        (_, None) => issues.push(ValidationIssue::new(
                            format!("edges[{index}].to.port"),
                            "PORT_NOT_FOUND",
                            "目标输入端口不存在",
                        )),
                    }
                }
                (None, _) => issues.push(ValidationIssue::new(
                    format!("edges[{index}].from.node"),
                    "NODE_NOT_FOUND",
                    "源节点不存在",
                )),
                (_, None) => issues.push(ValidationIssue::new(
                    format!("edges[{index}].to.node"),
                    "NODE_NOT_FOUND",
                    "目标节点不存在",
                )),
            }
        }
        let order = topological_order(&self.nodes, &self.edges);
        if order.len() != self.nodes.len() {
            issues.push(ValidationIssue::new(
                "edges",
                "CYCLE_DETECTED",
                "编组图不允许环路",
            ));
        }
        if issues.is_empty() {
            Ok(GraphSummary {
                node_count: self.nodes.len(),
                edge_count: self.edges.len(),
                enabled_node_count: self.nodes.iter().filter(|node| node.enabled).count(),
                publisher_count: self
                    .nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Publisher)
                    .count(),
                topological_order: order,
            })
        } else {
            Err(CoreError::validation(
                ErrorCode::GraphInvalid,
                "编组图校验失败",
                issues,
            ))
        }
    }

    pub fn impact_of_node(&self, node_id: &str) -> Result<NodeImpact, CoreError> {
        self.validate()?;
        if !self.nodes.iter().any(|node| node.id == node_id) {
            return Err(CoreError::new(ErrorCode::GraphInvalid, "目标节点不存在"));
        }
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in self.edges.iter().filter(|edge| edge.enabled) {
            outgoing
                .entry(edge.from.node.as_str())
                .or_default()
                .push(edge.to.node.as_str());
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([node_id]);
        while let Some(current) = queue.pop_front() {
            if let Some(targets) = outgoing.get(current) {
                for target in targets {
                    if visited.insert(*target) {
                        queue.push_back(target);
                    }
                }
            }
        }
        let mut downstream_nodes = visited.iter().map(ToString::to_string).collect::<Vec<_>>();
        downstream_nodes.sort();
        let mut affected_publishers = self
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Publisher && visited.contains(node.id.as_str()))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        affected_publishers.sort();
        Ok(NodeImpact {
            node_id: node_id.to_owned(),
            downstream_nodes,
            affected_publishers,
        })
    }
}

fn ports_for(kind: NodeKind) -> (Vec<Port>, Vec<Port>) {
    let port = |id: &str, data_type| Port {
        id: id.to_owned(),
        data_type,
    };
    match kind {
        NodeKind::Provider => (vec![], vec![port("candidate_out", PortType::Candidate)]),
        NodeKind::Composer => (
            vec![
                port("candidate_in", PortType::Candidate),
                port("health_in", PortType::HealthSignal),
            ],
            vec![port("service_plan_out", PortType::ServicePlan)],
        ),
        NodeKind::Middleware => (
            vec![port("service_plan_in", PortType::ServicePlan)],
            vec![port("service_plan_out", PortType::ServicePlan)],
        ),
        NodeKind::Probe => (vec![], vec![port("health_out", PortType::HealthSignal)]),
        NodeKind::Publisher => (vec![port("service_plan_in", PortType::ServicePlan)], vec![]),
        NodeKind::Group => (vec![], vec![]),
    }
}

fn default_config(kind: NodeKind) -> NodeConfig {
    match kind {
        NodeKind::Provider => NodeConfig::Provider(ProviderConfig {
            asset_id: String::new(),
            selected_models: default_models(),
        }),
        NodeKind::Composer => NodeConfig::Composer(default_composer_config()),
        NodeKind::Middleware => NodeConfig::Middleware(MiddlewareConfig {
            middleware: MiddlewareKind::RequestDefaults {
                temperature: None,
                top_p: None,
                max_output_tokens: None,
            },
        }),
        NodeKind::Probe => NodeConfig::Probe(default_probe_config()),
        NodeKind::Publisher => NodeConfig::Publisher(PublisherNodeConfig::default()),
        NodeKind::Group => NodeConfig::Group(GroupConfig::default()),
    }
}

fn default_composer_config() -> ComposerConfig {
    ComposerConfig {
        strategy: SelectionStrategy::PriorityFailover,
        timeout_ms: default_timeout_ms(),
        max_retries: default_retries(),
        failover_on: default_failover_errors(),
        allow_capability_degradation: false,
        latency_hysteresis_ms: default_latency_hysteresis_ms(),
        routes: Vec::new(),
    }
}

fn default_probe_config() -> ProbeConfig {
    ProbeConfig {
        provider_node_id: String::new(),
        interval_seconds: default_probe_interval(),
        timeout_ms: default_probe_timeout(),
        failure_threshold: default_probe_failure_threshold(),
        recovery_threshold: default_probe_recovery_threshold(),
        safe_only: true,
    }
}

fn valid_connection(from: NodeKind, to: NodeKind, port: PortType) -> bool {
    matches!(
        (from, to, port),
        (NodeKind::Provider, NodeKind::Composer, PortType::Candidate)
            | (NodeKind::Probe, NodeKind::Composer, PortType::HealthSignal)
            | (
                NodeKind::Composer,
                NodeKind::Middleware | NodeKind::Publisher,
                PortType::ServicePlan
            )
            | (
                NodeKind::Middleware,
                NodeKind::Middleware | NodeKind::Publisher,
                PortType::ServicePlan
            )
    )
}

fn validate_node_contract(index: usize, node: &Node, issues: &mut Vec<ValidationIssue>) {
    let (inputs, outputs) = ports_for(node.kind);
    if node
        .inputs
        .iter()
        .map(|port| port.data_type)
        .collect::<Vec<_>>()
        != inputs.iter().map(|port| port.data_type).collect::<Vec<_>>()
        || node
            .outputs
            .iter()
            .map(|port| port.data_type)
            .collect::<Vec<_>>()
            != outputs
                .iter()
                .map(|port| port.data_type)
                .collect::<Vec<_>>()
    {
        issues.push(ValidationIssue::new(
            format!("nodes[{index}].ports"),
            "NODE_PORT_CONTRACT",
            "节点端口不符合 Graph V3 固定契约",
        ));
    }
}

fn validate_node_config(index: usize, node: &Node, issues: &mut Vec<ValidationIssue>) {
    if node.config.kind() != node.kind {
        issues.push(ValidationIssue::new(
            format!("nodes[{index}].config"),
            "CONFIG_KIND_MISMATCH",
            "节点类型和强类型配置不一致",
        ));
        return;
    }
    match &node.config {
        NodeConfig::Provider(config)
            if config.asset_id.trim().is_empty()
                || config
                    .selected_models
                    .iter()
                    .any(|model| model.trim().is_empty()) =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "REQUIRED",
                "Provider 必须绑定钱包资产，模型名称不能为空",
            ))
        }
        NodeConfig::Composer(config)
            if config.timeout_ms == 0
                || config.timeout_ms > 600_000
                || config.max_retries > 10
                || config.latency_hysteresis_ms > 60_000 =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "OUT_OF_RANGE",
                "Composer 超时、重试或延迟滞回超出安全范围",
            ))
        }
        NodeConfig::Middleware(MiddlewareConfig {
            middleware:
                MiddlewareKind::RequestDefaults {
                    temperature,
                    top_p,
                    max_output_tokens,
                },
        }) if temperature.is_some_and(|value| !(0.0..=2.0).contains(&value))
            || top_p.is_some_and(|value| !(0.0..=1.0).contains(&value))
            || max_output_tokens == &Some(0) =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "OUT_OF_RANGE",
                "请求默认值超出安全范围",
            ))
        }
        NodeConfig::Middleware(MiddlewareConfig {
            middleware:
                MiddlewareKind::ModelPolicy {
                    allowed_models,
                    max_output_tokens,
                },
        }) if allowed_models.iter().any(|model| model.trim().is_empty())
            || max_output_tokens == &Some(0) =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "OUT_OF_RANGE",
                "模型策略包含空模型或无效 Token 限制",
            ))
        }
        NodeConfig::Middleware(MiddlewareConfig {
            middleware:
                MiddlewareKind::RateLimit {
                    requests_per_minute,
                    max_concurrent,
                },
        }) if *requests_per_minute == 0
            || *requests_per_minute > 1_000_000
            || *max_concurrent == 0
            || *max_concurrent > 10_000 =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "OUT_OF_RANGE",
                "限流配置超出安全范围",
            ))
        }
        NodeConfig::Middleware(MiddlewareConfig {
            middleware: MiddlewareKind::BudgetMonitor { warning_thresholds },
        }) if warning_thresholds.is_empty()
            || warning_thresholds
                .iter()
                .any(|value| *value == 0 || *value > 100) =>
        {
            issues.push(ValidationIssue::new(
                format!("nodes[{index}].config"),
                "OUT_OF_RANGE",
                "预算阈值必须在 1 到 100 之间",
            ))
        }
        _ => {}
    }
}

fn validate_ports(
    node_index: usize,
    direction: &str,
    ports: &[Port],
    issues: &mut Vec<ValidationIssue>,
) {
    let mut ids = HashSet::new();
    for (port_index, port) in ports.iter().enumerate() {
        if port.id.trim().is_empty() || !ids.insert(&port.id) {
            issues.push(ValidationIssue::new(
                format!("nodes[{node_index}].{direction}[{port_index}].id"),
                "INVALID_OR_DUPLICATE",
                "端口 ID 不能为空且必须唯一",
            ));
        }
    }
}

fn enabled_unreachable_nodes(graph: &WorkflowGraph) -> Vec<String> {
    let Some(publisher) = graph
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Publisher && node.enabled)
    else {
        return graph
            .nodes
            .iter()
            .filter(|node| node.enabled && node.kind != NodeKind::Group)
            .map(|node| node.id.clone())
            .collect();
    };
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in graph.edges.iter().filter(|edge| edge.enabled) {
        reverse
            .entry(edge.to.node.as_str())
            .or_default()
            .push(edge.from.node.as_str());
    }
    let mut reachable = HashSet::new();
    let mut stack = vec![publisher.id.as_str()];
    while let Some(current) = stack.pop() {
        if !reachable.insert(current) {
            continue;
        }
        if let Some(parents) = reverse.get(current) {
            stack.extend(parents.iter().copied());
        }
    }
    let mut result = graph
        .nodes
        .iter()
        .filter(|node| {
            node.enabled && node.kind != NodeKind::Group && !reachable.contains(node.id.as_str())
        })
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn edge_port_type(graph: &WorkflowGraph, edge: &Edge) -> Option<PortType> {
    graph
        .nodes
        .iter()
        .find(|node| node.id == edge.from.node)?
        .outputs
        .iter()
        .find(|port| port.id == edge.from.port)
        .map(|port| port.data_type)
}

fn topological_order(nodes: &[Node], edges: &[Edge]) -> Vec<String> {
    let mut incoming: HashMap<&str, usize> =
        nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges.iter().filter(|edge| edge.enabled) {
        if incoming.contains_key(edge.from.node.as_str())
            && incoming.contains_key(edge.to.node.as_str())
        {
            *incoming.entry(edge.to.node.as_str()).or_default() += 1;
            outgoing
                .entry(edge.from.node.as_str())
                .or_default()
                .push(edge.to.node.as_str());
        }
    }
    let mut ready = nodes
        .iter()
        .filter(|node| incoming.get(node.id.as_str()) == Some(&0))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    ready.sort();
    let mut ready = VecDeque::from(ready);
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(node) = ready.pop_front() {
        order.push(node.to_owned());
        if let Some(targets) = outgoing.get(node) {
            let mut targets = targets.clone();
            targets.sort();
            for target in targets {
                if let Some(count) = incoming.get_mut(target) {
                    *count -= 1;
                    if *count == 0 {
                        ready.push_back(target);
                    }
                }
            }
        }
    }
    order
}

const fn default_true() -> bool {
    true
}
const fn default_timeout_ms() -> u64 {
    30_000
}
const fn default_retries() -> u32 {
    2
}
const fn default_weight() -> u16 {
    1
}
const fn default_latency_hysteresis_ms() -> u64 {
    25
}
const fn default_rate_limit() -> u32 {
    60
}
const fn default_concurrency() -> u32 {
    8
}
const fn default_probe_interval() -> u64 {
    300
}
const fn default_probe_timeout() -> u64 {
    10_000
}
const fn default_probe_failure_threshold() -> u32 {
    3
}
const fn default_probe_recovery_threshold() -> u32 {
    2
}
fn default_models() -> Vec<String> {
    vec!["default".to_owned()]
}
fn default_budget_thresholds() -> Vec<u8> {
    vec![50, 80, 100]
}
fn default_failover_errors() -> HashSet<StandardError> {
    HashSet::from([
        StandardError::ProviderTimeout,
        StandardError::NetworkUnreachable,
        StandardError::RateLimited,
    ])
}

#[cfg(test)]
mod graph_v3_tests {
    use super::*;

    #[test]
    fn all_core_templates_have_one_composer_and_publisher() -> Result<(), CoreError> {
        let assets = vec![
            ("a".to_owned(), "A".to_owned()),
            ("b".to_owned(), "B".to_owned()),
        ];
        for template in [
            GraphTemplateKind::SingleProvider,
            GraphTemplateKind::PriorityFailover,
            GraphTemplateKind::MultiModel,
            GraphTemplateKind::WeightedRoundRobin,
            GraphTemplateKind::LowestLatency,
            GraphTemplateKind::FailoverWithProbe,
            GraphTemplateKind::GuardedFailover,
        ] {
            let graph = generate_graph_template("main", template, &assets)?;
            assert_eq!(
                graph
                    .nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Composer)
                    .count(),
                1
            );
            assert_eq!(
                graph
                    .nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Publisher)
                    .count(),
                1
            );
            graph.validate()?;
        }
        Ok(())
    }

    #[test]
    fn route_simulation_filters_health_and_uses_latency() -> Result<(), CoreError> {
        let report = CanvasCompilationReport {
            valid: true,
            public_models: vec!["smart".to_owned()],
            candidates: vec![
                CompiledCandidate {
                    provider_node_id: "provider-a".to_owned(),
                    asset_id: "a".to_owned(),
                    public_model: "smart".to_owned(),
                    upstream_model: "model-a".to_owned(),
                    priority: 0,
                    weight: 1,
                },
                CompiledCandidate {
                    provider_node_id: "provider-b".to_owned(),
                    asset_id: "b".to_owned(),
                    public_model: "smart".to_owned(),
                    upstream_model: "model-b".to_owned(),
                    priority: 1,
                    weight: 1,
                },
            ],
            candidate_count: 2,
            selected_strategy: SelectionStrategy::LowestLatency,
            middleware_order: Vec::new(),
            unreachable_nodes: Vec::new(),
            missing_secrets: Vec::new(),
            stale_runtime: false,
            requires_confirmation: false,
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        let compiled = CompiledCanvasGraph {
            routes: Vec::new(),
            middleware: Vec::new(),
            probes: Vec::new(),
            report,
        };
        let result = simulate_route(
            &compiled,
            &RouteSimulationInput {
                public_model: "smart".to_owned(),
                unavailable_provider_node_ids: BTreeSet::from(["provider-a".to_owned()]),
                latency_ms_by_provider_node: BTreeMap::from([
                    ("provider-a".to_owned(), 20),
                    ("provider-b".to_owned(), 80),
                ]),
            },
        )?;
        assert_eq!(
            result.selected_provider_node_id.as_deref(),
            Some("provider-b")
        );
        Ok(())
    }
}
