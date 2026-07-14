use apiarray_core::graph::{
    Edge, Endpoint, GRAPH_SCHEMA_VERSION, Node, NodeConfig, NodeKind, Port, PortType,
    ProviderConfig, WorkflowGraph, new_node,
};

fn node(id: &str, kind: NodeKind) -> Node {
    let mut node = new_node(id, id, kind);
    let (inputs, outputs) = match kind {
        NodeKind::Provider => (
            vec![],
            vec![Port {
                id: "candidate_out".into(),
                data_type: PortType::Candidate,
            }],
        ),
        NodeKind::Composer => (
            vec![
                Port {
                    id: "candidate_in".into(),
                    data_type: PortType::Candidate,
                },
                Port {
                    id: "health_in".into(),
                    data_type: PortType::HealthSignal,
                },
            ],
            vec![Port {
                id: "service_plan_out".into(),
                data_type: PortType::ServicePlan,
            }],
        ),
        NodeKind::Middleware => (
            vec![Port {
                id: "service_plan_in".into(),
                data_type: PortType::ServicePlan,
            }],
            vec![Port {
                id: "service_plan_out".into(),
                data_type: PortType::ServicePlan,
            }],
        ),
        NodeKind::Probe => (
            vec![],
            vec![Port {
                id: "health_out".into(),
                data_type: PortType::HealthSignal,
            }],
        ),
        NodeKind::Publisher => (
            vec![Port {
                id: "service_plan_in".into(),
                data_type: PortType::ServicePlan,
            }],
            vec![],
        ),
        NodeKind::Group => (vec![], vec![]),
    };
    node.inputs = inputs;
    node.outputs = outputs;
    if kind == NodeKind::Provider {
        node.config = NodeConfig::Provider(ProviderConfig {
            asset_id: "wallet-asset".to_owned(),
            selected_models: vec!["default".to_owned()],
        });
    }
    node
}

#[test]
fn accepts_provider_composer_publisher_service_plan() {
    let graph = WorkflowGraph {
        schema_version: GRAPH_SCHEMA_VERSION,
        id: "main".into(),
        nodes: vec![
            node("provider", NodeKind::Provider),
            node("composer", NodeKind::Composer),
            node("publisher", NodeKind::Publisher),
        ],
        edges: vec![
            Edge {
                id: "candidate".into(),
                from: Endpoint {
                    node: "provider".into(),
                    port: "candidate_out".into(),
                },
                to: Endpoint {
                    node: "composer".into(),
                    port: "candidate_in".into(),
                },
                enabled: true,
                label: None,
            },
            Edge {
                id: "publish".into(),
                from: Endpoint {
                    node: "composer".into(),
                    port: "service_plan_out".into(),
                },
                to: Endpoint {
                    node: "publisher".into(),
                    port: "service_plan_in".into(),
                },
                enabled: true,
                label: None,
            },
        ],
    };
    assert!(graph.validate().is_ok());
}

#[test]
fn rejects_candidate_connected_directly_to_publisher() {
    let graph = WorkflowGraph {
        schema_version: GRAPH_SCHEMA_VERSION,
        id: "main".into(),
        nodes: vec![
            node("provider", NodeKind::Provider),
            node("publisher", NodeKind::Publisher),
        ],
        edges: vec![Edge {
            id: "invalid".into(),
            from: Endpoint {
                node: "provider".into(),
                port: "candidate_out".into(),
            },
            to: Endpoint {
                node: "publisher".into(),
                port: "service_plan_in".into(),
            },
            enabled: true,
            label: None,
        }],
    };
    assert!(graph.validate().is_err());
}

#[test]
fn rejects_group_with_runtime_ports() {
    let mut group = node("group", NodeKind::Group);
    group.outputs.push(Port {
        id: "candidate_out".into(),
        data_type: PortType::Candidate,
    });
    let graph = WorkflowGraph {
        schema_version: GRAPH_SCHEMA_VERSION,
        id: "main".into(),
        nodes: vec![group],
        edges: vec![],
    };
    assert!(graph.validate().is_err());
}
