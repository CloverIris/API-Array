use crate::{CoreError, ErrorCode, SCHEMA_VERSION, ValidationIssue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

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
    Adapter,
    Probe,
    Transform,
    Router,
    Guard,
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
    Request,
    Response,
    Capability,
    Health,
    Control,
    Error,
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
        if self.schema_version != SCHEMA_VERSION {
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
                        (Some(output), Some(input)) if output.data_type == input.data_type => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            enabled: true,
            inputs: vec![Port {
                id: "request_in".to_owned(),
                data_type: PortType::Request,
            }],
            outputs: vec![Port {
                id: "request_out".to_owned(),
                data_type: PortType::Request,
            }],
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
