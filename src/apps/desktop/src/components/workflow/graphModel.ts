import type { Edge, Node, XYPosition } from "@xyflow/react";
import type { CanvasRuntimeSnapshot, WorkflowGraph, WorkflowNodeConfig, WorkspaceUiState } from "../../lib/desktop";

export type PortType = "candidate" | "service_plan" | "health_signal";
export type WorkflowNode = WorkflowGraph["nodes"][number];
export type ApiArrayNodeData = { model: WorkflowNode; runtime?: CanvasRuntimeSnapshot["providers"][number]; onToggle?: (id: string) => void };
export type CanvasNode = Node<ApiArrayNodeData, "apiArray">;
export type CanvasEdge = Edge<{ dataType: PortType; enabled: boolean; label?: string | null }, "workflow">;

export function graphToCanvas(graph: WorkflowGraph, uiState?: WorkspaceUiState): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const saved = uiState?.workflows[graph.id]?.nodePositions ?? {};
  const fallback = layeredPositions(graph);
  const parentByMember = new Map<string, string>();
  for (const group of graph.nodes) {
    if (group.config.type !== "group") continue;
    for (const memberId of group.config.member_ids) parentByMember.set(memberId, group.id);
  }
  const nodes = graph.nodes.map((model) => {
    const parentId = parentByMember.get(model.id);
    return {
      id: model.id,
      type: "apiArray" as const,
      position: saved[model.id] ?? fallback[model.id] ?? { x: 0, y: 0 },
      parentId,
      extent: parentId ? "parent" as const : undefined,
      data: { model },
      ariaLabel: `${model.kind} 节点：${model.name}`,
      domAttributes: { "aria-roledescription": "服务编组节点" },
    };
  });
  const nodeMap = new Map(graph.nodes.map((node) => [node.id, node]));
  const edges = graph.edges.map((edge) => {
    const source = nodeMap.get(edge.from.node);
    const dataType = (source?.outputs.find((port) => port.id === edge.from.port)?.data_type ?? "service_plan") as PortType;
    return {
      id: edge.id,
      type: "workflow" as const,
      source: edge.from.node,
      sourceHandle: edge.from.port,
      target: edge.to.node,
      targetHandle: edge.to.port,
      data: { dataType, enabled: edge.enabled !== false, label: edge.label },
      ariaLabel: `${edge.from.node} 到 ${edge.to.node} 的${portLabel(dataType)}连接`,
    };
  });
  return { nodes, edges };
}

export function canvasToGraph(base: WorkflowGraph, nodes: CanvasNode[], edges: CanvasEdge[]): WorkflowGraph {
  const membersByGroup = new Map<string, string[]>();
  for (const node of nodes) {
    if (node.parentId) membersByGroup.set(node.parentId, [...(membersByGroup.get(node.parentId) ?? []), node.id]);
  }
  return {
    ...base,
    nodes: nodes.map((node) => {
      if (node.data.model.config.type !== "group") return node.data.model;
      return { ...node.data.model, config: { ...node.data.model.config, member_ids: (membersByGroup.get(node.id) ?? []).sort() } };
    }),
    edges: edges.map((edge) => ({
      id: edge.id,
      from: { node: edge.source, port: edge.sourceHandle ?? "" },
      to: { node: edge.target, port: edge.targetHandle ?? "" },
      enabled: edge.data?.enabled !== false,
      label: edge.data?.label ?? null,
    })),
  };
}

export function positionsFromNodes(nodes: CanvasNode[]): Record<string, XYPosition> {
  return Object.fromEntries(nodes.map((node) => [node.id, node.position]));
}

export function layeredPositions(graph: WorkflowGraph): Record<string, XYPosition> {
  const runtimeNodes = graph.nodes.filter((node) => node.kind !== "group");
  const indegree = new Map(runtimeNodes.map((node) => [node.id, 0]));
  const outgoing = new Map(runtimeNodes.map((node) => [node.id, [] as string[]]));
  for (const edge of graph.edges.filter((edge) => edge.enabled !== false)) {
    if (!indegree.has(edge.from.node) || !indegree.has(edge.to.node)) continue;
    indegree.set(edge.to.node, (indegree.get(edge.to.node) ?? 0) + 1);
    outgoing.get(edge.from.node)?.push(edge.to.node);
  }
  const queue = runtimeNodes.filter((node) => indegree.get(node.id) === 0).map((node) => node.id).sort();
  const level = new Map(queue.map((id) => [id, 0]));
  while (queue.length) {
    const id = queue.shift()!;
    for (const next of (outgoing.get(id) ?? []).sort()) {
      level.set(next, Math.max(level.get(next) ?? 0, (level.get(id) ?? 0) + 1));
      indegree.set(next, (indegree.get(next) ?? 1) - 1);
      if (indegree.get(next) === 0) queue.push(next);
    }
  }
  const rows = new Map<number, number>();
  const result: Record<string, XYPosition> = {};
  for (const [index, node] of runtimeNodes.entries()) {
    const column = level.get(node.id) ?? index;
    const row = rows.get(column) ?? 0;
    rows.set(column, row + 1);
    result[node.id] = { x: 80 + column * 320, y: 90 + row * 210 };
  }
  graph.nodes.filter((node) => node.kind === "group").forEach((node, index) => { result[node.id] = { x: 40 + index * 40, y: 40 + index * 40 }; });
  return result;
}

export function connectionCreatesCycle(edges: CanvasEdge[], source: string, target: string) {
  if (source === target) return true;
  const outgoing = new Map<string, string[]>();
  for (const edge of edges.filter((item) => item.data?.enabled !== false)) outgoing.set(edge.source, [...(outgoing.get(edge.source) ?? []), edge.target]);
  outgoing.set(source, [...(outgoing.get(source) ?? []), target]);
  const stack = [target];
  const visited = new Set<string>();
  while (stack.length) {
    const current = stack.pop()!;
    if (current === source) return true;
    if (visited.has(current)) continue;
    visited.add(current);
    stack.push(...(outgoing.get(current) ?? []));
  }
  return false;
}

export function newWorkflowNode(kind: WorkflowNode["kind"], index: number): WorkflowNode {
  const id = `${kind}-${Date.now().toString(36)}-${index}`;
  const ports: Record<WorkflowNode["kind"], { inputs: WorkflowNode["inputs"]; outputs: WorkflowNode["outputs"] }> = {
    provider: { inputs: [], outputs: [{ id: "candidate_out", data_type: "candidate" }] },
    composer: { inputs: [{ id: "candidate_in", data_type: "candidate" }, { id: "health_in", data_type: "health_signal" }], outputs: [{ id: "service_plan_out", data_type: "service_plan" }] },
    middleware: { inputs: [{ id: "service_plan_in", data_type: "service_plan" }], outputs: [{ id: "service_plan_out", data_type: "service_plan" }] },
    probe: { inputs: [], outputs: [{ id: "health_out", data_type: "health_signal" }] },
    publisher: { inputs: [{ id: "service_plan_in", data_type: "service_plan" }], outputs: [] },
    group: { inputs: [], outputs: [] },
  };
  return { id, name: nodeName(kind), kind, enabled: true, ...ports[kind], config: defaultNodeConfig(kind) };
}

export function defaultNodeConfig(kind: WorkflowNode["kind"]): WorkflowNodeConfig {
  switch (kind) {
    case "provider": return { type: "provider", asset_id: "", selected_models: ["default"] };
    case "composer": return { type: "composer", strategy: "priority_failover", timeout_ms: 30_000, max_retries: 2, failover_on: ["PROVIDER_TIMEOUT", "NETWORK_UNREACHABLE", "RATE_LIMITED", "MODEL_UNAVAILABLE"], allow_capability_degradation: false, latency_hysteresis_ms: 25, routes: [] };
    case "middleware": return { type: "middleware", middleware: { kind: "request_defaults", temperature: null, top_p: null, max_output_tokens: null } };
    case "probe": return { type: "probe", provider_node_id: "", interval_seconds: 300, timeout_ms: 5_000, failure_threshold: 3, recovery_threshold: 2, safe_only: true };
    case "publisher": return { type: "publisher", publisher_id: null };
    case "group": return { type: "group", member_ids: [], collapsed: false };
  }
}

function nodeName(kind: WorkflowNode["kind"]) {
  return ({ provider: "钱包 Provider", composer: "Composer 综合器", middleware: "策略中间件", probe: "健康探测", publisher: "本地总输出", group: "节点组" })[kind];
}

function portLabel(type: PortType) {
  return ({ candidate: "候选", service_plan: "服务计划", health_signal: "健康信号" })[type];
}
