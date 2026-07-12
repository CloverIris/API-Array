use crate::{CoreError, ErrorCode, ValidationIssue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use crate::{routing::{RoutePolicy, StandardError}, runtime::{ModelRoute, RuntimeConfig, UpstreamRoute}, workspace::ApiWallet, SCHEMA_VERSION};

pub const GRAPH_SCHEMA_VERSION: u32 = 2;

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
    #[serde(default)]
    pub config: Value,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasCompilationReport {
    pub valid: bool,
    pub public_models: Vec<String>,
    pub candidate_count: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledCanvasGraph {
    pub routes: Vec<ModelRoute>,
    pub report: CanvasCompilationReport,
}

pub fn compile_canvas_graph(graph: &WorkflowGraph, wallet: &ApiWallet, runtime: &RuntimeConfig) -> Result<CompiledCanvasGraph, CoreError> {
    graph.validate()?;
    let composers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Composer).collect::<Vec<_>>();
    let publishers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Publisher).count();
    let providers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Provider && node.enabled).collect::<Vec<_>>();
    let mut errors = Vec::new();
    if composers.len() != 1 { errors.push("每个 Canvas 必须且只能包含一个 Composer。".to_owned()); }
    if publishers != 1 { errors.push("每个 Canvas 必须且只能包含一个 Publisher。".to_owned()); }
    if providers.is_empty() { errors.push("Composer 至少需要一个启用的钱包 Provider。".to_owned()); }
    let composer = composers.first();
    if let Some(composer) = composer {
        for provider in &providers { if !graph.edges.iter().any(|edge| edge.from.node == provider.id && edge.to.node == composer.id) { errors.push(format!("Provider {} 尚未连接 Composer。", provider.name)); } }
        let publisher_id = graph.nodes.iter().find(|node| node.kind == NodeKind::Publisher).map(|node| node.id.as_str());
        if let Some(publisher_id) = publisher_id { if !path_exists(graph, &composer.id, publisher_id) { errors.push("Composer 的 ServicePlan 尚未到达 Publisher。".to_owned()); } }
    }
    let timeout_ms = composer.and_then(|node| node.config.get("timeout_ms")).and_then(Value::as_u64).unwrap_or(30_000);
    let max_retries = composer.and_then(|node| node.config.get("max_retries")).and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()).unwrap_or(2);
    let allow_degradation = composer.and_then(|node| node.config.get("allow_capability_degradation")).and_then(Value::as_bool).unwrap_or(false);
    let mut grouped: BTreeMap<String, Vec<UpstreamRoute>> = BTreeMap::new();
    let mut model_adapters: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut adapters = HashSet::new();
    for node in providers {
        let Some(asset_id) = node.config.get("asset_id").and_then(Value::as_str) else { errors.push(format!("Provider {} 未绑定钱包资产。", node.name)); continue; };
        let Some(asset) = wallet.assets.get(asset_id) else { errors.push(format!("Provider {} 引用了不存在的钱包资产。", node.name)); continue; };
        let Some(instance) = runtime.providers.get(&asset.provider_instance_id) else { errors.push(format!("钱包资产 {} 缺少 Provider 实例。", asset.name)); continue; };
        let public_model = node.config.get("public_model").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).unwrap_or("default").to_owned();
        let upstream_model = node.config.get("upstream_model").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).unwrap_or(&public_model).to_owned();
        let priority = node.config.get("priority").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()).unwrap_or(0);
        adapters.insert(instance.manifest.adapter.id.clone());
        model_adapters.entry(public_model.clone()).or_default().insert(instance.manifest.adapter.id.clone());
        grouped.entry(public_model).or_default().push(UpstreamRoute { id: node.id.clone(), provider_instance: instance.id.clone(), upstream_model, priority, enabled: asset.enabled && instance.enabled, conditions: Vec::new() });
    }
    for upstreams in grouped.values_mut() { upstreams.sort_by_key(|upstream| (upstream.priority, upstream.id.clone())); }
    for (model, kinds) in &model_adapters { if kinds.len() > 1 && !allow_degradation { errors.push(format!("公开模型 {model} 包含不同协议的主备候选；请显式允许能力降级或使用不同公开模型名。")); } }
    if !errors.is_empty() { return Err(CoreError::new(ErrorCode::GraphInvalid, errors.join(" "))); }
    let routes = grouped.into_iter().map(|(public_model, upstreams)| ModelRoute { policy: RoutePolicy { schema_version: SCHEMA_VERSION, id: format!("{}-{public_model}-policy", graph.id), timeout_ms, max_retries, failover_on: HashSet::from([StandardError::ProviderTimeout, StandardError::NetworkUnreachable, StandardError::RateLimited]) }, public_model, upstreams }).collect::<Vec<_>>();
    let mut warnings = if adapters.len() > 1 { vec!["多个上游协议将通过 Canonical Adapter 统一为 OpenAI-compatible。".to_owned()] } else { Vec::new() };
    if allow_degradation && model_adapters.values().any(|kinds| kinds.len() > 1) { warnings.push("跨协议主备已允许能力降级；工具、图像或 JSON 能力可能缩减。".to_owned()); }
    Ok(CompiledCanvasGraph { report: CanvasCompilationReport { valid: true, public_models: routes.iter().map(|route| route.public_model.clone()).collect(), candidate_count: routes.iter().map(|route| route.upstreams.len()).sum(), warnings, errors: Vec::new() }, routes })
}

fn path_exists(graph: &WorkflowGraph, source: &str, target: &str) -> bool {
    let mut stack = vec![source]; let mut visited = HashSet::new();
    while let Some(current) = stack.pop() { if current == target { return true; } if !visited.insert(current) { continue; } for edge in graph.edges.iter().filter(|edge| edge.from.node == current) { stack.push(edge.to.node.as_str()); } }
    false
}

impl WorkflowGraph {
    /// 校验节点、端口、连接类型与无环约束，并生成拓扑摘要。
    ///
    /// # Errors
    ///
    /// 图结构包含重复 ID、缺失节点/端口、类型错误或环路时，返回包含全部
    /// 已发现问题的 [`ErrorCode::GraphInvalid`]。
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<GraphSummary, CoreError> {
        let mut issues = Vec::new();
        if self.schema_version != GRAPH_SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                "UNSUPPORTED_SCHEMA",
                "工作流 Schema 版本不受支持",
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(ValidationIssue::new("id", "REQUIRED", "工作流 ID 不能为空"));
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
        }

        let mut edge_ids = HashSet::new();
        for (index, edge) in self.edges.iter().enumerate() {
            if !edge_ids.insert(&edge.id) {
                issues.push(ValidationIssue::new(
                    format!("edges[{index}].id"),
                    "DUPLICATE",
                    "连接 ID 必须唯一",
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
                        (Some(output), Some(input)) if output.data_type == input.data_type && valid_connection(from.kind, to.kind, output.data_type) => {}
                        (Some(_), Some(_)) => issues.push(ValidationIssue::new(
                            format!("edges[{index}]"),
                            "PORT_TYPE_MISMATCH",
                            "连接两端的端口类型不一致",
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
                "首版工作流不允许环路",
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
                "工作流图校验失败",
                issues,
            ))
        }
    }

    /// 计算禁用或删除节点会影响的下游节点与 Publisher。
    ///
    /// # Errors
    ///
    /// 图无效或目标节点不存在时返回错误。
    pub fn impact_of_node(&self, node_id: &str) -> Result<NodeImpact, CoreError> {
        self.validate()?;
        if !self.nodes.iter().any(|node| node.id == node_id) {
            return Err(CoreError::new(ErrorCode::GraphInvalid, "目标节点不存在"));
        }
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &self.edges {
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

fn valid_connection(from: NodeKind, to: NodeKind, port: PortType) -> bool {
    matches!((from, to, port),
        (NodeKind::Provider, NodeKind::Composer, PortType::Candidate)
        | (NodeKind::Probe, NodeKind::Composer, PortType::HealthSignal)
        | (NodeKind::Composer, NodeKind::Middleware | NodeKind::Publisher, PortType::ServicePlan)
        | (NodeKind::Middleware, NodeKind::Middleware | NodeKind::Publisher, PortType::ServicePlan))
}

fn validate_node_contract(index: usize, node: &Node, issues: &mut Vec<ValidationIssue>) {
    let expected: (&[PortType], &[PortType]) = match node.kind {
        NodeKind::Provider => (&[], &[PortType::Candidate]),
        NodeKind::Composer => (&[PortType::Candidate, PortType::HealthSignal], &[PortType::ServicePlan]),
        NodeKind::Middleware => (&[PortType::ServicePlan], &[PortType::ServicePlan]),
        NodeKind::Probe => (&[], &[PortType::HealthSignal]),
        NodeKind::Publisher => (&[PortType::ServicePlan], &[]),
        NodeKind::Group => (&[], &[]),
    };
    let inputs = node.inputs.iter().map(|port| port.data_type).collect::<Vec<_>>();
    let outputs = node.outputs.iter().map(|port| port.data_type).collect::<Vec<_>>();
    if inputs != expected.0 || outputs != expected.1 {
        issues.push(ValidationIssue::new(format!("nodes[{index}].ports"), "NODE_PORT_CONTRACT", "节点端口不符合 Graph V2 固定契约"));
    }
}

const fn default_true() -> bool {
    true
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
                "端口 ID 不能为空且在同一方向必须唯一",
            ));
        }
    }
}

fn topological_order(nodes: &[Node], edges: &[Edge]) -> Vec<String> {
    let mut incoming: HashMap<&str, usize> =
        nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
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
    let mut ready: VecDeque<&str> = nodes
        .iter()
        .filter(|node| incoming.get(node.id.as_str()) == Some(&0))
        .map(|node| node.id.as_str())
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(node) = ready.pop_front() {
        order.push(node.to_owned());
        if let Some(targets) = outgoing.get(node) {
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

#[cfg(any())]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> Node {
        let (inputs, outputs) = match kind {
            NodeKind::Provider => (vec![], vec![Port { id: "candidate_out".to_owned(), data_type: PortType::Candidate }]),
            NodeKind::Composer => (vec![Port { id: "candidate_in".to_owned(), data_type: PortType::Candidate }, Port { id: "health_in".to_owned(), data_type: PortType::HealthSignal }], vec![Port { id: "service_plan_out".to_owned(), data_type: PortType::ServicePlan }]),
            NodeKind::Middleware => (vec![Port { id: "service_plan_in".to_owned(), data_type: PortType::ServicePlan }], vec![Port { id: "service_plan_out".to_owned(), data_type: PortType::ServicePlan }]),
            NodeKind::Probe => (vec![], vec![Port { id: "health_out".to_owned(), data_type: PortType::HealthSignal }]),
            NodeKind::Publisher => (vec![Port { id: "service_plan_in".to_owned(), data_type: PortType::ServicePlan }], vec![]),
            NodeKind::Group => (vec![], vec![]),
        };
        Node {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            enabled: true,
            inputs,
            outputs,
            config: Value::Null,
        }
    }

    #[test]
    fn validates_acyclic_typed_graph() -> Result<(), CoreError> {
        let graph = WorkflowGraph {
            schema_version: 1,
            id: "main".to_owned(),
            nodes: vec![
                node("router", NodeKind::Router),
                node("adapter", NodeKind::Adapter),
            ],
            edges: vec![Edge {
                id: "route".to_owned(),
                from: Endpoint {
                    node: "router".to_owned(),
                    port: "request_out".to_owned(),
                },
                to: Endpoint {
                    node: "adapter".to_owned(),
                    port: "request_in".to_owned(),
                },
            }],
        };
        assert_eq!(graph.validate()?.topological_order, ["router", "adapter"]);
        Ok(())
    }

    #[test]
    fn rejects_cycle() {
        let mut graph = WorkflowGraph {
            schema_version: 1,
            id: "main".to_owned(),
            nodes: vec![node("a", NodeKind::Router), node("b", NodeKind::Adapter)],
            edges: Vec::new(),
        };
        for (id, from, to) in [("ab", "a", "b"), ("ba", "b", "a")] {
            graph.edges.push(Edge {
                id: id.to_owned(),
                from: Endpoint {
                    node: from.to_owned(),
                    port: "request_out".to_owned(),
                },
                to: Endpoint {
                    node: to.to_owned(),
                    port: "request_in".to_owned(),
                },
            });
        }
        let error = graph.validate().expect_err("cycle must fail");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.code == "CYCLE_DETECTED")
        );
    }

    #[test]
    fn reports_downstream_publisher_impact() -> Result<(), CoreError> {
        let graph = WorkflowGraph {
            schema_version: 1,
            id: "main".to_owned(),
            nodes: vec![
                node("adapter", NodeKind::Adapter),
                node("publisher", NodeKind::Publisher),
            ],
            edges: vec![Edge {
                id: "publish".to_owned(),
                from: Endpoint {
                    node: "adapter".to_owned(),
                    port: "request_out".to_owned(),
                },
                to: Endpoint {
                    node: "publisher".to_owned(),
                    port: "request_in".to_owned(),
                },
            }],
        };
        let impact = graph.impact_of_node("adapter")?;
        assert_eq!(impact.affected_publishers, ["publisher"]);
        Ok(())
    }
}
