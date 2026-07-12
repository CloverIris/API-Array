"use client";

import "@xyflow/react/dist/style.css";
import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  MiniMap,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
  type Connection,
  type EdgeChange,
  type NodeChange,
  type OnSelectionChangeParams,
  type Viewport,
} from "@xyflow/react";
import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Branch } from "@openai/apps-sdk-ui/components/Icon";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getCanvasGraph,
  getCanvasNodeImpact,
  saveCanvasGraph,
  validateWorkflowGraph,
  type WorkflowGraph,
  type WalletCard,
  type WorkflowValidationResult,
  type WorkspaceUiState,
} from "../../lib/desktop";
import { EmptyPage, readError } from "../shared";
import { ApiArrayNode } from "./ApiArrayNode";
import { WorkflowEdge } from "./WorkflowEdge";
import { WorkflowToolbar, type SaveState } from "./WorkflowToolbar";
import type { WorkflowTemplate } from "./NodePalette";
import { WorkflowValidationPanel } from "./WorkflowValidationPanel";
import {
  canvasToGraph,
  connectionCreatesCycle,
  graphToCanvas,
  layeredPositions,
  newWorkflowNode,
  positionsFromNodes,
  type CanvasEdge,
  type CanvasNode,
  type WorkflowNode,
} from "./graphModel";
import type { SelectedWorkflowItem } from "./WorkflowInspector";

const nodeTypes = { apiArray: ApiArrayNode };
const edgeTypes = { workflow: WorkflowEdge };
const ariaLabels = {
  "node.a11yDescription.default": "按 Enter 或空格选择节点，方向键移动，Delete 删除，Escape 取消。",
  "edge.a11yDescription.default": "按 Enter 或空格选择连接，Delete 删除，Escape 取消。",
  "controls.ariaLabel": "画布控制",
  "controls.zoomIn.ariaLabel": "放大",
  "controls.zoomOut.ariaLabel": "缩小",
  "controls.fitView.ariaLabel": "适应画布",
  "controls.interactive.ariaLabel": "切换画布交互",
  "minimap.ariaLabel": "工作流缩略图",
  "handle.ariaLabel": "类型化端口",
};

type HistoryEntry = { nodes: CanvasNode[]; edges: CanvasEdge[] };

export function WorkflowCanvas({ projectId, canvasId, wallet, uiState, publisherRunning, onUiState, onSelection, onGraphChange }: { projectId: string; canvasId: string; wallet: WalletCard[]; uiState: WorkspaceUiState; publisherRunning: boolean; onUiState: (next: WorkspaceUiState) => void; onSelection: (selected: SelectedWorkflowItem) => void; onGraphChange?: (graph: WorkflowGraph) => void }) {
  return <ReactFlowProvider><WorkflowCanvasInner projectId={projectId} canvasId={canvasId} wallet={wallet} uiState={uiState} publisherRunning={publisherRunning} onUiState={onUiState} onSelection={onSelection} onGraphChange={onGraphChange} /></ReactFlowProvider>;
}

function WorkflowCanvasInner({ projectId, canvasId, wallet, uiState, publisherRunning, onUiState, onSelection, onGraphChange }: { projectId: string; canvasId: string; wallet: WalletCard[]; uiState: WorkspaceUiState; publisherRunning: boolean; onUiState: (next: WorkspaceUiState) => void; onSelection: (selected: SelectedWorkflowItem) => void; onGraphChange?: (graph: WorkflowGraph) => void }) {
  const [graph, setGraph] = useState<WorkflowGraph | null>(null);
  const [nodes, setNodes] = useState<CanvasNode[]>([]);
  const [edges, setEdges] = useState<CanvasEdge[]>([]);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [validation, setValidation] = useState<WorkflowValidationResult | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [loadingError, setLoadingError] = useState<string | null>(null);
  const history = useRef<HistoryEntry[]>([]);
  const future = useRef<HistoryEntry[]>([]);
  const { fitView, screenToFlowPosition } = useReactFlow<CanvasNode, CanvasEdge>();

  useEffect(() => {
    void getCanvasGraph(projectId, canvasId).then((nextGraph) => {
      const canvas = graphToCanvas(nextGraph, uiState);
      setGraph(nextGraph);
      setNodes(canvas.nodes);
      setEdges(canvas.edges);
      onGraphChange?.(nextGraph);
    }).catch((reason) => setLoadingError(readError(reason)));
  // uiState is intentionally read only during initial graph hydration.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const snapshotHistory = useCallback(() => {
    history.current.push({ nodes, edges });
    if (history.current.length > 50) history.current.shift();
    future.current = [];
  }, [nodes, edges]);

  const markDirty = useCallback(() => { setSaveState("dirty"); setValidation(null); }, []);

  const updateNode = useCallback((id: string, patch: Partial<WorkflowNode>) => {
    snapshotHistory();
    setNodes((current) => current.map((node) => node.id === id ? { ...node, data: { ...node.data, model: { ...node.data.model, ...patch } } } : node));
    markDirty();
  }, [markDirty, snapshotHistory]);

  const toggleNode = useCallback(async (id: string) => {
    const current = nodes.find((item) => item.id === id);
    if (!current) return;
    if (current.data.model.kind === "publisher") { setLocalError("Canvas 总输出器不能停用。"); return; }
    if (current.data.model.enabled && graph?.nodes.some((item) => item.id === id)) {
      const impact = await getCanvasNodeImpact(projectId, canvasId, id).catch(() => null);
      const detail = impact
        ? [`下游节点：${impact.downstream_nodes.join("、") || "无"}`, `受影响 Publisher：${impact.affected_publishers.join("、") || "无"}`].join("\n")
        : "暂时无法读取完整影响范围。";
      if (!window.confirm(`停用 ${current.data.model.name} 可能中断请求路径。\n${detail}\n\n是否继续？`)) return;
    }
    updateNode(id, { enabled: !current.data.model.enabled });
  }, [canvasId, graph, nodes, projectId, updateNode]);

  const displayedNodes = useMemo(() => nodes.map((node) => ({ ...node, data: { ...node.data, onToggle: (id: string) => void toggleNode(id) } })), [nodes, toggleNode]);

  const onNodesChange = useCallback((changes: NodeChange<CanvasNode>[]) => {
    setNodes((current) => applyNodeChanges(changes, current));
    if (changes.some((change) => change.type === "add" || change.type === "remove")) markDirty();
  }, [markDirty]);
  const onEdgesChange = useCallback((changes: EdgeChange<CanvasEdge>[]) => {
    if (changes.some((change) => change.type === "remove")) snapshotHistory();
    setEdges((current) => applyEdgeChanges(changes, current));
    if (changes.some((change) => change.type === "add" || change.type === "remove")) markDirty();
  }, [markDirty, snapshotHistory]);

  const connectionError = useCallback((connection: Connection | CanvasEdge) => {
    if (!connection.source || !connection.target || !connection.sourceHandle || !connection.targetHandle) return "连接缺少节点或端口。";
    if (connectionCreatesCycle(edges, connection.source, connection.target)) return "默认工作流禁止自连接或环路。";
    const source = nodes.find((node) => node.id === connection.source)?.data.model;
    const target = nodes.find((node) => node.id === connection.target)?.data.model;
    const output = source?.outputs.find((port) => port.id === connection.sourceHandle);
    const input = target?.inputs.find((port) => port.id === connection.targetHandle);
    if (!source || !target) return "连接引用了不存在的节点。";
    if (!output || !input) return "连接引用了不存在的端口。";
    if (output.data_type !== input.data_type) return `端口类型不兼容：${output.data_type} 不能连接到 ${input.data_type}。`;
    const allowed = (source.kind === "provider" && target?.kind === "composer" && output.data_type === "candidate") || (source.kind === "probe" && target?.kind === "composer" && output.data_type === "health_signal") || ((source.kind === "composer" || source.kind === "middleware") && (target?.kind === "middleware" || target?.kind === "publisher") && output.data_type === "service_plan");
    if (!allowed) return `${source.kind} 不能通过 ${output.data_type} 连接到 ${target?.kind ?? "未知节点"}。`;
    return null;
  }, [edges, nodes]);

  const connect = useCallback((connection: Connection) => {
    const error = connectionError(connection);
    if (error) { setLocalError(error); return; }
    snapshotHistory();
    const source = nodes.find((node) => node.id === connection.source)?.data.model;
    const dataType = source?.outputs.find((port) => port.id === connection.sourceHandle)?.data_type ?? "service_plan";
    setEdges((current) => addEdge({ ...connection, id: `edge-${Date.now().toString(36)}`, type: "workflow", data: { dataType }, markerEnd: { type: MarkerType.ArrowClosed } }, current));
    setLocalError(null);
    markDirty();
  }, [connectionError, markDirty, nodes, snapshotHistory]);

  const addNode = (kind: WorkflowNode["kind"]) => {
    if (kind === "publisher" && nodes.some((node) => node.data.model.kind === "publisher")) { setLocalError("每个 Canvas 只能有一个总输出器。"); return; }
    snapshotHistory();
    const model = newWorkflowNode(kind, nodes.length);
    const position = screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
    setNodes((current) => [...current, { id: model.id, type: "apiArray", position, data: { model }, ariaLabel: `${kind} 节点：${model.name}` }]);
    if (graph) setGraph({ ...graph, nodes: [...graph.nodes, model] });
    markDirty();
  };

  const addAsset = (asset: WalletCard) => {
    if (asset.source !== "asset" || !asset.providerInstanceId) { setLocalError("请先在 API 钱包中完成资产配置。"); return; }
    snapshotHistory();
    const model = newWorkflowNode("provider", nodes.length);
    model.name = asset.name;
    model.config = { asset_id: asset.id, upstream_model: "default", public_model: "default", priority: 0 };
    const position = screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
    const composer = nodes.find((node) => node.data.model.kind === "composer");
    setNodes((current) => [...current, { id: model.id, type: "apiArray", position, data: { model }, ariaLabel: `钱包 Provider：${asset.name}` }]);
    if (composer) setEdges((current) => [...current, { id: `edge-${model.id}-composer`, type: "workflow", source: model.id, sourceHandle: "candidate_out", target: composer.id, targetHandle: "candidate_in", data: { dataType: "candidate" }, markerEnd: { type: MarkerType.ArrowClosed } }]);
    setLocalError(asset.configured ? null : `${asset.name} 尚未配置 Key；可以保存草稿，但运行会被阻止。`);
    markDirty();
  };

  const applyTemplate = (template: WorkflowTemplate) => {
    const usable = wallet.filter((asset) => asset.source === "asset" && asset.providerInstanceId);
    const required = template === "single" ? 1 : 2;
    if (usable.length < required) { setLocalError(`该模板至少需要 ${required} 个钱包资产。`); return; }
    snapshotHistory();
    const publisher = nodes.find((node) => node.data.model.kind === "publisher")?.data.model ?? newWorkflowNode("publisher", 0);
    const providers = usable.slice(0, required).map((asset, index) => { const node = newWorkflowNode("provider", index + 1); node.name = asset.name; node.config = { asset_id: asset.id, upstream_model: "default", public_model: template === "single" ? "default" : `model-${index + 1}`, priority: index }; return node; });
    const composer = newWorkflowNode("composer", required + 1); composer.config = { strategy: "priority_failover", timeout_ms: 30000, max_retries: 2, allow_capability_degradation: false };
    const extra = template === "guarded" ? [newWorkflowNode("probe", required + 2), newWorkflowNode("middleware", required + 3)] : [];
    if (extra[0]) extra[0].config = { safe_only: true, interval_seconds: 300, failure_threshold: 3 };
    if (extra[1]) extra[1].config = { budget_warning_percent: 80, rate_limit_per_minute: 60, allowed_models: ["default"] };
    const models = [publisher, ...providers, composer, ...extra];
    const graphEdges = providers.map((provider, index) => ({ id: `provider-${index}-composer`, from: { node: provider.id, port: "candidate_out" }, to: { node: composer.id, port: "candidate_in" } }));
    const middleware = extra.find((node) => node.kind === "middleware");
    graphEdges.push({ id: "composer-service", from: { node: composer.id, port: "service_plan_out" }, to: { node: middleware?.id ?? publisher.id, port: middleware ? "service_plan_in" : "service_plan_in" } });
    if (middleware) graphEdges.push({ id: "middleware-publisher", from: { node: middleware.id, port: "service_plan_out" }, to: { node: publisher.id, port: "service_plan_in" } });
    const probe = extra.find((node) => node.kind === "probe");
    if (probe) graphEdges.push({ id: "probe-health", from: { node: probe.id, port: "health_out" }, to: { node: composer.id, port: "health_in" } });
    const generated = graphToCanvas({ ...graph!, nodes: models, edges: graphEdges }, uiState);
    setNodes(generated.nodes);
    setEdges(generated.edges);
    setLocalError("模板已创建。请连接类型化端口并完成配置后保存草稿。");
    markDirty();
  };

  const undo = () => { const previous = history.current.pop(); if (!previous) return; future.current.push({ nodes, edges }); setNodes(previous.nodes); setEdges(previous.edges); markDirty(); };
  const redo = () => { const next = future.current.pop(); if (!next) return; history.current.push({ nodes, edges }); setNodes(next.nodes); setEdges(next.edges); markDirty(); };
  const autoLayout = () => { if (!graph) return; snapshotHistory(); const currentGraph = canvasToGraph(graph, nodes, edges); const positions = layeredPositions(currentGraph); setNodes((current) => current.map((node) => ({ ...node, position: positions[node.id] ?? node.position }))); markDirty(); window.setTimeout(() => void fitView({ padding: 0.18, duration: 250 }), 0); };
  const currentGraph = useCallback(() => graph ? canvasToGraph(graph, nodes, edges) : null, [edges, graph, nodes]);
  const validate = async () => { const candidate = currentGraph(); if (!candidate) return; setLocalError(null); try { setValidation(await validateWorkflowGraph(candidate)); } catch (reason) { setLocalError(readError(reason)); } };
  const save = async () => { const candidate = currentGraph(); if (!candidate) return; setSaveState("saving"); setLocalError(null); try { const result = await validateWorkflowGraph(candidate); setValidation(result); if (!result.valid) { setSaveState("failed"); return; } const saved = await saveCanvasGraph(projectId, canvasId, candidate); setGraph(saved.canvas.graph); setSaveState("saved"); onGraphChange?.(saved.canvas.graph); } catch (reason) { setLocalError(readError(reason)); setSaveState("failed"); } };

  const moveEnd = (_: MouseEvent | TouchEvent | null, viewport: Viewport) => {
    if (!graph) return;
    onUiState({ ...uiState, workflows: { ...uiState.workflows, [graph.id]: { viewport, nodePositions: positionsFromNodes(nodes), collapsedGroups: uiState.workflows[graph.id]?.collapsedGroups ?? [] } } });
  };
  const nodeDragStop = () => {
    if (!graph) return;
    const previous = uiState.workflows[graph.id];
    onUiState({ ...uiState, workflows: { ...uiState.workflows, [graph.id]: { viewport: previous?.viewport ?? { x: 0, y: 0, zoom: 1 }, nodePositions: positionsFromNodes(nodes), collapsedGroups: previous?.collapsedGroups ?? [] } } });
  };
  const selectionChanged = useCallback(({ nodes: selectedNodes, edges: selectedEdges }: OnSelectionChangeParams<CanvasNode, CanvasEdge>) => {
    const node = selectedNodes[0];
    if (node) { onSelection({ kind: "node", node: node.data.model, wallet, onUpdate: updateNode }); return; }
    const edge = selectedEdges[0];
    if (edge && graph) { const model = canvasToGraph(graph, nodes, edges).edges.find((item) => item.id === edge.id); if (model) { onSelection({ kind: "edge", edge: model }); return; } }
    onSelection(null);
  }, [edges, graph, nodes, onSelection, updateNode, wallet]);
  const beforeDelete = async ({ nodes: deletingNodes }: { nodes: CanvasNode[]; edges: CanvasEdge[] }) => {
    if (!deletingNodes.length) return true;
    if (deletingNodes.some((node) => node.data.model.kind === "publisher")) { setLocalError("Canvas 总输出器受保护，不能删除。"); return false; }
    const impacts = await Promise.all(deletingNodes.filter((node) => graph?.nodes.some((item) => item.id === node.id)).map((node) => getCanvasNodeImpact(projectId, canvasId, node.id).catch(() => null)));
    const publishers = [...new Set(impacts.flatMap((impact) => impact?.affected_publishers ?? []))];
    const downstream = [...new Set(impacts.flatMap((impact) => impact?.downstream_nodes ?? []))];
    const detail = [`将删除 ${deletingNodes.length} 个节点。`, publishers.length ? `受影响 Publisher：${publishers.join("、")}` : "没有已发布出口受影响。", downstream.length ? `下游节点：${downstream.join("、")}` : "没有下游节点。"].join("\n");
    const accepted = window.confirm(`${detail}\n\n此操作会形成未保存草稿，是否继续？`);
    if (accepted) { snapshotHistory(); markDirty(); }
    return accepted;
  };

  if (loadingError) return <div><p className="error-message" role="alert">{loadingError}</p><EmptyPage page="工作流无法加载" hint="最后一份有效运行配置没有被修改。" icon={Branch} /></div>;
  if (!graph) return <EmptyPage page="正在加载工作流" hint="工作流状态保存在本地工作区中。" icon={Branch} />;

  return <section className="workflow-editor">
    <div className="workflow-heading"><div><p className="eyebrow">Workflow Graph</p><h2>{graph.id}</h2></div><div className="workflow-heading-badges"><Badge color="info" variant="soft">{nodes.length} 个节点 · {edges.length} 条连接</Badge>{publisherRunning ? <Badge color="warning" variant="soft">运行中 · 编辑为草稿</Badge> : null}</div></div>
    <WorkflowValidationPanel result={validation} localError={localError} />
    <WorkflowToolbar wallet={wallet} hasPublisher={nodes.some((node) => node.data.model.kind === "publisher")} saveState={saveState} canUndo={history.current.length > 0} canRedo={future.current.length > 0} onAddAsset={addAsset} onAddControl={addNode} onApplyTemplate={applyTemplate} onUndo={undo} onRedo={redo} onLayout={autoLayout} onFit={() => void fitView({ padding: 0.18, duration: 250 })} onValidate={() => void validate()} onSave={() => void save()} />
    <div className="workflow-canvas" aria-label="API ARRAY 节点编排画布">
      <ReactFlow nodes={displayedNodes} edges={edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={connect} isValidConnection={(connection) => !connectionError(connection)} onMoveEnd={moveEnd} onNodeDragStop={nodeDragStop} onSelectionChange={selectionChanged} onBeforeDelete={beforeDelete} defaultViewport={uiState.workflows[graph.id]?.viewport} fitView={!uiState.workflows[graph.id]} nodesFocusable edgesFocusable deleteKeyCode={["Backspace", "Delete"]} multiSelectionKeyCode={["Control", "Meta"]} ariaLabelConfig={ariaLabels} minZoom={0.2} maxZoom={2.5} snapToGrid snapGrid={[16, 16]}>
        <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} />
        {!nodes.length ? <Panel position="top-center" className="canvas-empty"><strong>从 API 钱包开始编组</strong><span>新 Canvas 会自动生成 Composer 与 Publisher；钱包资产会作为候选连接到 Composer。</span></Panel> : null}
        <MiniMap pannable zoomable nodeColor={(node) => (node as CanvasNode).data.model.enabled ? "var(--app-port-candidate)" : "var(--color-text-tertiary)"} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  </section>;
}
