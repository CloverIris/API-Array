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
  type OnConnectEnd,
  type OnSelectionChangeParams,
  type Viewport,
} from "@xyflow/react";
import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Branch } from "@openai/apps-sdk-ui/components/Icon";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  applyCanvasTemplate,
  getCanvasGraph,
  getCanvasRuntimeSnapshot,
  getCanvasNodeImpact,
  saveCanvasGraph,
  runCanvasProbe,
  setCanvasProbeSchedule,
  simulateCanvasRoute,
  validateWorkflowGraph,
  type WorkflowGraph,
  type CanvasRuntimeSnapshot,
  type WalletCard,
  type WorkflowValidationResult,
  type WorkspaceUiState,
} from "../../lib/desktop";
import { EmptyPage, readError } from "../shared";
import { askAppDialog } from "../dialogs/AppDialog";
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
type ClipboardGraph = { nodes: WorkflowNode[]; edges: WorkflowGraph["edges"] };
type QuickConnectState = {
  clientX: number;
  clientY: number;
  position: { x: number; y: number };
  sourceNodeId: string;
  sourceHandleId: string;
  dataType: "candidate" | "service_plan" | "health_signal";
  kinds: Array<WorkflowNode["kind"]>;
};
type CanvasContextMenuState = {
  clientX: number;
  clientY: number;
  nodeId?: string;
  edgeId?: string;
};

export function WorkflowCanvas({ projectId, canvasId, expectedDraftRevision, wallet, uiState, publisherRunning, onUiState, onSelection, onGraphChange }: { projectId: string; canvasId: string; expectedDraftRevision: number; wallet: WalletCard[]; uiState: WorkspaceUiState; publisherRunning: boolean; onUiState: (next: WorkspaceUiState) => void; onSelection: (selected: SelectedWorkflowItem) => void; onGraphChange?: (graph: WorkflowGraph) => void }) {
  return <ReactFlowProvider><WorkflowCanvasInner projectId={projectId} canvasId={canvasId} expectedDraftRevision={expectedDraftRevision} wallet={wallet} uiState={uiState} publisherRunning={publisherRunning} onUiState={onUiState} onSelection={onSelection} onGraphChange={onGraphChange} /></ReactFlowProvider>;
}

function WorkflowCanvasInner({ projectId, canvasId, expectedDraftRevision, wallet, uiState, publisherRunning, onUiState, onSelection, onGraphChange }: { projectId: string; canvasId: string; expectedDraftRevision: number; wallet: WalletCard[]; uiState: WorkspaceUiState; publisherRunning: boolean; onUiState: (next: WorkspaceUiState) => void; onSelection: (selected: SelectedWorkflowItem) => void; onGraphChange?: (graph: WorkflowGraph) => void }) {
  const [graph, setGraph] = useState<WorkflowGraph | null>(null);
  const [nodes, setNodes] = useState<CanvasNode[]>([]);
  const [edges, setEdges] = useState<CanvasEdge[]>([]);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [validation, setValidation] = useState<WorkflowValidationResult | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [loadingError, setLoadingError] = useState<string | null>(null);
  const [draftRevision, setDraftRevision] = useState(expectedDraftRevision);
  const [runtimeSnapshot, setRuntimeSnapshot] = useState<CanvasRuntimeSnapshot | null>(null);
  const [commandPalette, setCommandPalette] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [simulation, setSimulation] = useState<string | null>(null);
  const [scheduledProbeIds, setScheduledProbeIds] = useState<Set<string>>(() => new Set());
  const [quickConnect, setQuickConnect] = useState<QuickConnectState | null>(null);
  const [contextMenu, setContextMenu] = useState<CanvasContextMenuState | null>(null);
  const history = useRef<HistoryEntry[]>([]);
  const future = useRef<HistoryEntry[]>([]);
  const clipboard = useRef<ClipboardGraph | null>(null);
  const { deleteElements, fitView, screenToFlowPosition } = useReactFlow<CanvasNode, CanvasEdge>();

  useEffect(() => setDraftRevision(expectedDraftRevision), [expectedDraftRevision]);

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

  useEffect(() => {
    let disposed = false;
    const refresh = () => void getCanvasRuntimeSnapshot(projectId, canvasId).then((snapshot) => { if (!disposed) setRuntimeSnapshot(snapshot); }).catch(() => { if (!disposed) setRuntimeSnapshot(null); });
    refresh();
    let unlisten: undefined | (() => void);
    void import("@tauri-apps/api/event").then(({ listen }) => Promise.all([
      listen<string>("desktop:canvas-runtime-changed", (event) => { if (event.payload === canvasId) refresh(); }),
      listen<string>("desktop:provider-health-changed", (event) => { if (event.payload === canvasId) refresh(); }),
    ])).then((handlers) => { unlisten = () => handlers.forEach((handler) => handler()); }).catch(() => undefined);
    return () => { disposed = true; unlisten?.(); };
  }, [canvasId, projectId]);

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
    if (current.data.model.kind === "publisher") { setLocalError("编组方案总输出器不能停用。"); return; }
    if (current.data.model.enabled && graph?.nodes.some((item) => item.id === id)) {
      const impact = await getCanvasNodeImpact(projectId, canvasId, id).catch(() => null);
      const detail = impact
        ? [`下游节点：${impact.downstream_nodes.join("、") || "无"}`, `受影响 Publisher：${impact.affected_publishers.join("、") || "无"}`].join("\n")
        : "暂时无法读取完整影响范围。";
      if (!await askAppDialog({ title: `停用 ${current.data.model.name}？`, description: `这可能中断请求路径。\n${detail}`, confirmLabel: "停用节点", danger: true })) return;
    }
    updateNode(id, { enabled: !current.data.model.enabled });
  }, [canvasId, graph, nodes, projectId, updateNode]);

  const displayedNodes = useMemo(() => {
    const collapsedGroups = new Set(nodes.filter((node) => node.data.model.config.type === "group" && node.data.model.config.collapsed).map((node) => node.id));
    return nodes.map((node) => ({ ...node, hidden: Boolean(node.parentId && collapsedGroups.has(node.parentId)), data: { ...node.data, runtime: runtimeSnapshot?.providers.find((provider) => provider.providerNodeId === node.id), onToggle: (id: string) => void toggleNode(id) } }));
  }, [nodes, runtimeSnapshot, toggleNode]);

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
    const enabledEdges = edges.filter((edge) => edge.data?.enabled !== false);
    if (enabledEdges.some((edge) => edge.source === connection.source && edge.sourceHandle === connection.sourceHandle && edge.target === connection.target && edge.targetHandle === connection.targetHandle)) return "这两个端口已经连接。";
    if (output.data_type === "service_plan" && enabledEdges.some((edge) => edge.source === connection.source && edge.sourceHandle === connection.sourceHandle)) return "首版 ServicePlan 不允许分叉；该输出已经连接。";
    if (input.data_type === "service_plan" && enabledEdges.some((edge) => edge.target === connection.target && edge.targetHandle === connection.targetHandle)) return "每个 ServicePlan 输入只能接收一条启用连接。";
    if ((source.kind === "provider" || source.kind === "probe") && enabledEdges.some((edge) => edge.source === connection.source && edge.sourceHandle === connection.sourceHandle)) return "Provider 候选或 Probe 健康信号只能连接一次。";
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
    setEdges((current) => addEdge({ ...connection, id: `edge-${Date.now().toString(36)}`, type: "workflow", data: { dataType, enabled: true, label: null }, markerEnd: { type: MarkerType.ArrowClosed } }, current));
    setLocalError(null);
    markDirty();
  }, [connectionError, markDirty, nodes, snapshotHistory]);

  const connectEnd = useCallback<OnConnectEnd>((event, state) => {
    if (state.isValid || state.toNode || !state.fromHandle || !state.fromNode || state.fromHandle.type !== "source" || !state.fromHandle.id) {
      return;
    }
    const source = nodes.find((node) => node.id === state.fromNode.id)?.data.model;
    const output = source?.outputs.find((port) => port.id === state.fromHandle.id);
    if (!source || !output) return;
    const enabledEdges = edges.filter((edge) => edge.data?.enabled !== false);
    if (enabledEdges.some((edge) => edge.source === source.id && edge.sourceHandle === output.id)) {
      setLocalError("该输出端口已经连接；Graph V3 的候选、健康信号和服务计划均不允许隐式分叉。");
      return;
    }
    let kinds: Array<WorkflowNode["kind"]> = [];
    if (output.data_type === "candidate" || output.data_type === "health_signal") {
      if (!nodes.some((node) => node.data.model.kind === "composer")) kinds = ["composer"];
    } else {
      kinds = ["middleware"];
      if (!nodes.some((node) => node.data.model.kind === "publisher")) kinds.push("publisher");
    }
    if (!kinds.length) {
      setLocalError("当前画布已经存在唯一综合器或总输出器，请连接到现有兼容节点。");
      return;
    }
    const point = "changedTouches" in event ? event.changedTouches[0] : event;
    setQuickConnect({
      clientX: Math.max(8, Math.min(point.clientX, window.innerWidth - 230)),
      clientY: Math.max(8, Math.min(point.clientY, window.innerHeight - 220)),
      position: screenToFlowPosition({ x: point.clientX, y: point.clientY }),
      sourceNodeId: source.id,
      sourceHandleId: output.id,
      dataType: output.data_type,
      kinds,
    });
  }, [edges, nodes, screenToFlowPosition]);

  const createConnectedNode = useCallback((kind: WorkflowNode["kind"]) => {
    if (!quickConnect) return;
    const targetHandle = quickConnect.dataType === "candidate"
      ? "candidate_in"
      : quickConnect.dataType === "health_signal"
        ? "health_in"
        : "service_plan_in";
    snapshotHistory();
    const model = newWorkflowNode(kind, nodes.length);
    const node: CanvasNode = {
      id: model.id,
      type: "apiArray",
      position: quickConnect.position,
      data: { model },
      selected: true,
      ariaLabel: `${model.name} 节点`,
    };
    const edge: CanvasEdge = {
      id: `edge-${Date.now().toString(36)}`,
      type: "workflow",
      source: quickConnect.sourceNodeId,
      sourceHandle: quickConnect.sourceHandleId,
      target: model.id,
      targetHandle,
      data: { dataType: quickConnect.dataType, enabled: true, label: null },
      markerEnd: { type: MarkerType.ArrowClosed },
    };
    setNodes((current) => [...current.map((item) => ({ ...item, selected: false })), node]);
    setEdges((current) => [...current, edge]);
    setQuickConnect(null);
    setLocalError(null);
    markDirty();
  }, [markDirty, nodes.length, quickConnect, snapshotHistory]);

  const duplicateSingleNode = useCallback((id: string) => {
    const source = nodes.find((node) => node.id === id);
    if (!source || source.data.model.kind === "publisher" || source.data.model.kind === "composer") {
      setLocalError("Composer 与总输出器是编组方案单例，不能复制。");
      return;
    }
    snapshotHistory();
    const nextId = `${source.data.model.kind}-${Date.now().toString(36)}-copy`;
    const model = remapNode(source.data.model, new Map([[source.id, nextId]]));
    setNodes((current) => [...current.map((item) => ({ ...item, selected: false })), {
      ...source,
      id: nextId,
      parentId: undefined,
      extent: undefined,
      position: { x: source.position.x + 36, y: source.position.y + 36 },
      data: { model },
      selected: true,
    }]);
    markDirty();
    setContextMenu(null);
  }, [markDirty, nodes, snapshotHistory]);

  const openNodeContextMenu = useCallback((event: React.MouseEvent, node: CanvasNode) => {
    event.preventDefault();
    setNodes((current) => current.map((item) => ({ ...item, selected: item.id === node.id })));
    setEdges((current) => current.map((item) => ({ ...item, selected: false })));
    setContextMenu({
      clientX: Math.max(8, Math.min(event.clientX, window.innerWidth - 210)),
      clientY: Math.max(8, Math.min(event.clientY, window.innerHeight - 190)),
      nodeId: node.id,
    });
  }, []);

  const openEdgeContextMenu = useCallback((event: React.MouseEvent, edge: CanvasEdge) => {
    event.preventDefault();
    setNodes((current) => current.map((item) => ({ ...item, selected: false })));
    setEdges((current) => current.map((item) => ({ ...item, selected: item.id === edge.id })));
    setContextMenu({
      clientX: Math.max(8, Math.min(event.clientX, window.innerWidth - 210)),
      clientY: Math.max(8, Math.min(event.clientY, window.innerHeight - 150)),
      edgeId: edge.id,
    });
  }, []);

  const addNode = (kind: WorkflowNode["kind"]) => {
    if (kind === "publisher" && nodes.some((node) => node.data.model.kind === "publisher")) { setLocalError("每个编组方案只能有一个总输出器。"); return; }
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
    model.config = { type: "provider", asset_id: asset.id, selected_models: ["default"] };
    const position = screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
    const composer = nodes.find((node) => node.data.model.kind === "composer");
    setNodes((current) => [...current, { id: model.id, type: "apiArray", position, data: { model }, ariaLabel: `钱包 Provider：${asset.name}` }]);
    if (composer) setEdges((current) => [...current, { id: `edge-${model.id}-composer`, type: "workflow", source: model.id, sourceHandle: "candidate_out", target: composer.id, targetHandle: "candidate_in", data: { dataType: "candidate", enabled: true, label: null }, markerEnd: { type: MarkerType.ArrowClosed } }]);
    setLocalError(asset.configured ? null : `${asset.name} 尚未配置 Key；可以保存草稿，但运行会被阻止。`);
    markDirty();
  };

  const applyTemplate = async (template: WorkflowTemplate) => {
    const usable = wallet.filter((asset) => asset.source === "asset" && asset.providerInstanceId);
    const required = template === "single_provider" ? 1 : 2;
    if (usable.length < required) { setLocalError(`该模板至少需要 ${required} 个钱包资产。`); return; }
    const accepted = await askAppDialog({ title: "应用 Core 编组模板？", description: `模板会原子替换当前草稿，并使用 ${usable.slice(0, required).map((asset) => asset.name).join("、")}。运行快照不会改变，直到你再次运行该方案。`, confirmLabel: "替换草稿", danger: true });
    if (!accepted) return;
    setSaveState("saving");
    try {
      const saved = await applyCanvasTemplate(projectId, canvasId, draftRevision, template, usable.slice(0, required).map((asset) => asset.id));
      const generated = graphToCanvas(saved.canvas.graph, uiState);
      setGraph(saved.canvas.graph); setNodes(generated.nodes); setEdges(generated.edges); setDraftRevision(saved.canvas.draftRevision); setSaveState("saved"); setValidation(null); setLocalError("模板已由 Core 生成并保存为新草稿；运行快照尚未改变。"); onGraphChange?.(saved.canvas.graph);
    } catch (reason) { setSaveState("failed"); setLocalError(readError(reason)); }
  };

  const undo = () => { const previous = history.current.pop(); if (!previous) return; future.current.push({ nodes, edges }); setNodes(previous.nodes); setEdges(previous.edges); markDirty(); };
  const redo = () => { const next = future.current.pop(); if (!next) return; history.current.push({ nodes, edges }); setNodes(next.nodes); setEdges(next.edges); markDirty(); };
  const autoLayout = () => { if (!graph) return; snapshotHistory(); const currentGraph = canvasToGraph(graph, nodes, edges); const positions = layeredPositions(currentGraph); setNodes((current) => current.map((node) => ({ ...node, position: positions[node.id] ?? node.position }))); markDirty(); window.setTimeout(() => void fitView({ padding: 0.18, duration: 250 }), 0); };
  const currentGraph = useCallback(() => graph ? canvasToGraph(graph, nodes, edges) : null, [edges, graph, nodes]);

  const copySelection = useCallback(() => {
    if (!graph) return;
    const selectedIds = new Set(nodes.filter((node) => node.selected && node.data.model.kind !== "publisher" && node.data.model.kind !== "composer").map((node) => node.id));
    if (!selectedIds.size) { setLocalError("请先选择可复制的 Provider、Probe、Middleware 或 Group。"); return; }
    const source = canvasToGraph(graph, nodes, edges);
    clipboard.current = { nodes: source.nodes.filter((node) => selectedIds.has(node.id)), edges: source.edges.filter((edge) => selectedIds.has(edge.from.node) && selectedIds.has(edge.to.node)) };
    setLocalError(`已复制 ${selectedIds.size} 个节点；Secret 与 Publisher Token 不会被复制。`);
  }, [edges, graph, nodes]);

  const pasteSelection = useCallback(() => {
    const payload = clipboard.current;
    if (!payload) { setLocalError("内部画布剪贴板为空。"); return; }
    snapshotHistory();
    const serial = Date.now().toString(36);
    const ids = new Map(payload.nodes.map((node, index) => [node.id, `${node.kind}-${serial}-${index}`]));
    const clonedModels = payload.nodes.map((node) => remapNode(node, ids));
    const sourcePositions = new Map(nodes.map((node) => [node.id, node.position]));
    const clonedNodes = clonedModels.map((model, index) => ({ id: model.id, type: "apiArray" as const, position: { x: (sourcePositions.get(payload.nodes[index].id)?.x ?? 100) + 36, y: (sourcePositions.get(payload.nodes[index].id)?.y ?? 100) + 36 }, data: { model }, selected: true, ariaLabel: `${model.kind} 节点：${model.name}` }));
    const clonedEdges = payload.edges.map((edge, index) => ({ id: `edge-${serial}-${index}`, type: "workflow" as const, source: ids.get(edge.from.node)!, sourceHandle: edge.from.port, target: ids.get(edge.to.node)!, targetHandle: edge.to.port, data: { dataType: edgeType(payload.nodes, edge), enabled: edge.enabled, label: edge.label }, markerEnd: { type: MarkerType.ArrowClosed } }));
    setNodes((current) => [...current.map((node) => ({ ...node, selected: false })), ...clonedNodes]);
    setEdges((current) => [...current, ...clonedEdges]);
    markDirty();
  }, [markDirty, nodes, snapshotHistory]);

  const deleteSelection = useCallback(() => {
    const selected = new Set(nodes.filter((node) => node.selected && !["publisher", "composer"].includes(node.data.model.kind)).map((node) => node.id));
    if (!selected.size) return;
    snapshotHistory(); setNodes((current) => current.filter((node) => !selected.has(node.id))); setEdges((current) => current.filter((edge) => !selected.has(edge.source) && !selected.has(edge.target))); markDirty();
  }, [markDirty, nodes, snapshotHistory]);

  const cutSelection = useCallback(() => { copySelection(); deleteSelection(); }, [copySelection, deleteSelection]);
  const alignSelection = useCallback((axis: "left" | "top") => {
    const selected = nodes.filter((node) => node.selected);
    if (selected.length < 2) return;
    snapshotHistory(); const value = Math.min(...selected.map((node) => axis === "left" ? node.position.x : node.position.y)); const ids = new Set(selected.map((node) => node.id));
    setNodes((current) => current.map((node) => ids.has(node.id) ? { ...node, position: { ...node.position, [axis === "left" ? "x" : "y"]: value } } : node)); markDirty();
  }, [markDirty, nodes, snapshotHistory]);
  const distributeSelection = useCallback((axis: "horizontal" | "vertical") => {
    const selected = nodes.filter((node) => node.selected).sort((a, b) => axis === "horizontal" ? a.position.x - b.position.x : a.position.y - b.position.y);
    if (selected.length < 3) return;
    const key = axis === "horizontal" ? "x" : "y"; const first = selected[0].position[key]; const last = selected[selected.length - 1].position[key]; const step = (last - first) / (selected.length - 1); const positions = new Map(selected.map((node, index) => [node.id, first + step * index]));
    snapshotHistory(); setNodes((current) => current.map((node) => positions.has(node.id) ? { ...node, position: { ...node.position, [key]: positions.get(node.id)! } } : node)); markDirty();
  }, [markDirty, nodes, snapshotHistory]);
  const groupSelection = useCallback(() => {
    const selected = nodes.filter((node) => node.selected && node.data.model.kind !== "group"); if (!selected.length) return;
    snapshotHistory(); const minX = Math.min(...selected.map((node) => node.position.x)) - 36; const minY = Math.min(...selected.map((node) => node.position.y)) - 56; const groupModel = newWorkflowNode("group", nodes.length); if (groupModel.config.type === "group") groupModel.config.member_ids = selected.map((node) => node.id).sort();
    const groupNode: CanvasNode = { id: groupModel.id, type: "apiArray", position: { x: minX, y: minY }, data: { model: groupModel }, style: { width: 420, height: Math.max(280, Math.max(...selected.map((node) => node.position.y)) - minY + 220) }, selected: true };
    const ids = new Set(selected.map((node) => node.id)); setNodes((current) => [groupNode, ...current.map((node) => ids.has(node.id) ? { ...node, parentId: groupModel.id, extent: "parent" as const, position: { x: node.position.x - minX, y: node.position.y - minY }, selected: false } : node)]); markDirty();
  }, [markDirty, nodes, snapshotHistory]);
  const ungroupSelection = useCallback(() => {
    const group = nodes.find((node) => node.selected && node.data.model.kind === "group"); if (!group) return;
    snapshotHistory(); setNodes((current) => current.filter((node) => node.id !== group.id).map((node) => node.parentId === group.id ? { ...node, parentId: undefined, extent: undefined, position: { x: node.position.x + group.position.x, y: node.position.y + group.position.y } } : node)); markDirty();
  }, [markDirty, nodes, snapshotHistory]);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null; if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "p") { event.preventDefault(); setCommandPalette(true); return; }
      if (!event.ctrlKey && !event.metaKey) return;
      if (event.key.toLowerCase() === "c") { event.preventDefault(); copySelection(); }
      if (event.key.toLowerCase() === "x") { event.preventDefault(); cutSelection(); }
      if (event.key.toLowerCase() === "v") { event.preventDefault(); pasteSelection(); }
      if (event.key.toLowerCase() === "d") { event.preventDefault(); copySelection(); pasteSelection(); }
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [copySelection, cutSelection, pasteSelection]);
  const validate = async () => { const candidate = currentGraph(); if (!candidate) return; setLocalError(null); try { setValidation(await validateWorkflowGraph(candidate)); } catch (reason) { setLocalError(readError(reason)); } };
  const save = async () => { const candidate = currentGraph(); if (!candidate) return; setSaveState("saving"); setLocalError(null); try { const result = await validateWorkflowGraph(candidate); setValidation(result); if (!result.valid) { setSaveState("failed"); return; } const saved = await saveCanvasGraph(projectId, canvasId, draftRevision, candidate); setGraph(saved.canvas.graph); setDraftRevision(saved.canvas.draftRevision); setSaveState("saved"); onGraphChange?.(saved.canvas.graph); } catch (reason) { setLocalError(readError(reason)); setSaveState("failed"); } };
  const simulateRoute = async () => {
    const composer = nodes.find((node) => node.data.model.config.type === "composer")?.data.model.config;
    const publicModel = composer?.type === "composer" ? composer.routes[0]?.public_model : undefined;
    if (!publicModel) { setLocalError("请先在 Composer 中配置至少一个公开模型。 "); return; }
    try {
      const result = await simulateCanvasRoute(projectId, canvasId, publicModel);
      setSimulation(result.selectedProviderNodeId
        ? `${result.publicModel} → ${result.selectedProviderNodeId} / ${result.selectedUpstreamModel ?? "默认模型"}。${result.explanation}`
        : `${result.publicModel} 当前没有可选候选。${result.explanation}`);
    } catch (reason) { setLocalError(readError(reason)); }
  };
  const runSelectedProbe = async () => {
    const probe = nodes.find((node) => node.selected && node.data.model.kind === "probe");
    if (!probe) { setLocalError("请先选择一个健康探测节点。 "); return; }
    try {
      const report = await runCanvasProbe(projectId, canvasId, probe.id);
      setSimulation(`探测 ${probe.data.model.name}：${report.overall}，连续失败 ${report.consecutive_failures} 次。`);
    } catch (reason) { setLocalError(readError(reason)); }
  };
  const toggleSelectedProbeSchedule = async () => {
    const probe = nodes.find((node) => node.selected && node.data.model.kind === "probe");
    if (!probe) { setLocalError("请先选择一个健康探测节点。 "); return; }
    const enabled = !scheduledProbeIds.has(probe.id);
    try {
      await setCanvasProbeSchedule(projectId, canvasId, probe.id, enabled);
      setScheduledProbeIds((current) => { const next = new Set(current); if (enabled) next.add(probe.id); else next.delete(probe.id); return next; });
      setSimulation(`${probe.data.model.name} 的周期探测已${enabled ? "启用" : "停用"}。`);
    } catch (reason) { setLocalError(readError(reason)); }
  };

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
    if (node) { onSelection({ kind: "node", node: node.data.model, nodes: nodes.map((item) => item.data.model), wallet, onUpdate: updateNode }); return; }
    const edge = selectedEdges[0];
    if (edge && graph) { const model = canvasToGraph(graph, nodes, edges).edges.find((item) => item.id === edge.id); if (model) { onSelection({ kind: "edge", edge: model, onUpdate: (id, patch) => { snapshotHistory(); setEdges((current) => current.map((item) => item.id === id ? { ...item, data: { dataType: item.data?.dataType ?? "service_plan", enabled: patch.enabled ?? item.data?.enabled !== false, label: patch.label === undefined ? item.data?.label : patch.label } } : item)); markDirty(); } }); return; } }
    onSelection(null);
  }, [edges, graph, markDirty, nodes, onSelection, snapshotHistory, updateNode, wallet]);
  const beforeDelete = async ({ nodes: deletingNodes }: { nodes: CanvasNode[]; edges: CanvasEdge[] }) => {
    if (!deletingNodes.length) return true;
    if (deletingNodes.some((node) => node.data.model.kind === "publisher" || node.data.model.kind === "composer")) { setLocalError("编组方案的 Composer 与总输出器是受保护的唯一节点，不能删除。"); return false; }
    const impacts = await Promise.all(deletingNodes.filter((node) => graph?.nodes.some((item) => item.id === node.id)).map((node) => getCanvasNodeImpact(projectId, canvasId, node.id).catch(() => null)));
    const publishers = [...new Set(impacts.flatMap((impact) => impact?.affected_publishers ?? []))];
    const downstream = [...new Set(impacts.flatMap((impact) => impact?.downstream_nodes ?? []))];
    const detail = [`将删除 ${deletingNodes.length} 个节点。`, publishers.length ? `受影响 Publisher：${publishers.join("、")}` : "没有已发布出口受影响。", downstream.length ? `下游节点：${downstream.join("、")}` : "没有下游节点。"].join("\n");
    const accepted = await askAppDialog({ title: "删除所选编排对象？", description: `${detail}\n此操作会形成未保存草稿。`, confirmLabel: "删除", danger: true });
    if (accepted) { snapshotHistory(); markDirty(); }
    return Boolean(accepted);
  };
  const commands = [
    { id: "copy", label: "复制所选节点", hint: "Ctrl+C", run: copySelection },
    { id: "paste", label: "粘贴并生成新 ID", hint: "Ctrl+V", run: pasteSelection },
    { id: "align-left", label: "左对齐", hint: "多选", run: () => alignSelection("left") },
    { id: "align-top", label: "顶部对齐", hint: "多选", run: () => alignSelection("top") },
    { id: "distribute-x", label: "水平等距分布", hint: "至少 3 个节点", run: () => distributeSelection("horizontal") },
    { id: "distribute-y", label: "垂直等距分布", hint: "至少 3 个节点", run: () => distributeSelection("vertical") },
    { id: "group", label: "创建节点组", hint: "纯视觉组织", run: groupSelection },
    { id: "ungroup", label: "解散所选节点组", hint: "保留成员", run: ungroupSelection },
    { id: "layout", label: "确定性自动布局", hint: "不改变运行语义", run: autoLayout },
    { id: "validate", label: "校验 Graph V3", hint: "结构与端口", run: () => void validate() },
    { id: "simulate", label: "模拟首个公开模型路由", hint: "不发起网络请求", run: () => void simulateRoute() },
    { id: "probe-now", label: "立即运行所选健康探测", hint: "仅执行无副作用探测", run: () => void runSelectedProbe() },
    { id: "probe-schedule", label: "切换所选健康探测周期", hint: "重启后自动恢复", run: () => void toggleSelectedProbeSchedule() },
  ].filter((command) => command.label.toLowerCase().includes(commandQuery.trim().toLowerCase()));

  if (loadingError) return <div><p className="error-message" role="alert">{loadingError}</p><EmptyPage page="工作流无法加载" hint="最后一份有效运行配置没有被修改。" icon={Branch} /></div>;
  if (!graph) return <EmptyPage page="正在加载工作流" hint="工作流状态保存在本地工作区中。" icon={Branch} />;

  return <section className="workflow-editor">
    <div className="workflow-heading"><div><p className="eyebrow">GRAPH V3 SERVICE PLAN</p><h2>{graph.id}</h2></div><div className="workflow-heading-badges"><Badge color="info" variant="soft">{nodes.length} 个节点 · {edges.length} 条连接</Badge>{runtimeSnapshot ? <Badge color={runtimeSnapshot.staleRuntime ? "warning" : "success"} variant="soft">{runtimeSnapshot.staleRuntime ? "运行快照已过期" : strategyLabel(runtimeSnapshot.strategy)}</Badge> : null}{publisherRunning ? <Badge color="warning" variant="soft">运行中 · 编辑为草稿</Badge> : null}</div></div>
    <WorkflowValidationPanel result={validation} localError={localError} />
    {simulation ? <div className="workflow-simulation" role="status"><strong>路由模拟</strong><span>{simulation}</span><button type="button" onClick={() => setSimulation(null)} aria-label="关闭路由模拟结果">×</button></div> : null}
    <WorkflowToolbar wallet={wallet} hasPublisher={nodes.some((node) => node.data.model.kind === "publisher")} saveState={saveState} canUndo={history.current.length > 0} canRedo={future.current.length > 0} onAddAsset={addAsset} onAddControl={addNode} onApplyTemplate={(template) => void applyTemplate(template)} onUndo={undo} onRedo={redo} onCommands={() => setCommandPalette(true)} onLayout={autoLayout} onFit={() => void fitView({ padding: 0.18, duration: 250 })} onValidate={() => void validate()} onSave={() => void save()} />
    <div className="workflow-canvas" aria-label="API ARRAY 节点编排画布">
      <ReactFlow nodes={displayedNodes} edges={edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={connect} onConnectEnd={connectEnd} isValidConnection={(connection) => !connectionError(connection)} onMoveEnd={moveEnd} onNodeDragStop={nodeDragStop} onSelectionChange={selectionChanged} onNodeContextMenu={openNodeContextMenu} onEdgeContextMenu={openEdgeContextMenu} onPaneClick={() => { setContextMenu(null); setQuickConnect(null); }} onBeforeDelete={beforeDelete} defaultViewport={uiState.workflows[graph.id]?.viewport} fitView={!uiState.workflows[graph.id]} nodesFocusable edgesFocusable deleteKeyCode={["Backspace", "Delete"]} multiSelectionKeyCode={["Control", "Meta"]} ariaLabelConfig={ariaLabels} minZoom={0.2} maxZoom={2.5} snapToGrid snapGrid={[16, 16]}>
        <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} />
        {!nodes.length ? <Panel position="top-center" className="canvas-empty"><strong>从 API 钱包开始编组</strong><span>新编组方案会自动生成 Composer 与 Publisher；钱包资产会作为候选连接到 Composer。</span></Panel> : null}
        {nodes.filter((node) => node.selected).length > 1 ? <Panel position="bottom-center" className="canvas-selection-toolbar"><span>{nodes.filter((node) => node.selected).length} 个节点</span><Button size="sm" color="secondary" variant="ghost" onClick={() => alignSelection("left")}>左对齐</Button><Button size="sm" color="secondary" variant="ghost" onClick={() => alignSelection("top")}>顶对齐</Button><Button size="sm" color="secondary" variant="ghost" onClick={groupSelection}>创建组</Button></Panel> : null}
        <MiniMap pannable zoomable nodeColor={(node) => (node as CanvasNode).data.model.enabled ? "var(--app-port-candidate)" : "var(--color-text-tertiary)"} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
    {quickConnect ? <div className="quick-connect-menu" role="dialog" aria-label="创建兼容节点" style={{ left: quickConnect.clientX, top: quickConnect.clientY }}>
      <strong>创建并连接</strong>
      <span>{quickConnect.dataType === "candidate" ? "候选" : quickConnect.dataType === "health_signal" ? "健康信号" : "服务计划"}</span>
      {quickConnect.kinds.map((kind) => <button key={kind} type="button" onClick={() => createConnectedNode(kind)}>{kind === "composer" ? "Composer 综合器" : kind === "middleware" ? "策略中间件" : "总输出器"}</button>)}
      <button type="button" className="quick-connect-cancel" onClick={() => setQuickConnect(null)}>取消</button>
    </div> : null}
    {contextMenu ? <div className="canvas-context-menu" role="menu" aria-label="画布对象操作" style={{ left: contextMenu.clientX, top: contextMenu.clientY }}>
      {contextMenu.nodeId ? <>
        <button type="button" role="menuitem" onClick={() => { const node = nodes.find((item) => item.id === contextMenu.nodeId); if (node) void toggleNode(node.id); setContextMenu(null); }}>{nodes.find((item) => item.id === contextMenu.nodeId)?.data.model.enabled ? "停用节点" : "启用节点"}</button>
        <button type="button" role="menuitem" onClick={() => duplicateSingleNode(contextMenu.nodeId!)}>复制节点</button>
        <button type="button" role="menuitem" className="danger" onClick={() => { void deleteElements({ nodes: [{ id: contextMenu.nodeId! }] }); setContextMenu(null); }}>删除节点</button>
      </> : null}
      {contextMenu.edgeId ? <>
        <button type="button" role="menuitem" onClick={() => { snapshotHistory(); setEdges((current) => current.map((edge) => edge.id === contextMenu.edgeId ? { ...edge, data: { dataType: edge.data?.dataType ?? "service_plan", enabled: edge.data?.enabled === false, label: edge.data?.label } } : edge)); markDirty(); setContextMenu(null); }}>{edges.find((edge) => edge.id === contextMenu.edgeId)?.data?.enabled === false ? "启用连接" : "停用连接"}</button>
        <button type="button" role="menuitem" className="danger" onClick={() => { void deleteElements({ edges: [{ id: contextMenu.edgeId! }] }); setContextMenu(null); }}>删除连接</button>
      </> : null}
    </div> : null}
    {commandPalette ? <div className="canvas-command-backdrop" role="presentation" onMouseDown={(event) => { if (event.currentTarget === event.target) setCommandPalette(false); }}><div className="canvas-command-palette" role="dialog" aria-modal="true" aria-label="画布命令"><Input value={commandQuery} onChange={(event) => setCommandQuery(event.target.value)} placeholder="搜索画布命令" autoFocus /> <div>{commands.map((command) => <button key={command.id} onClick={() => { command.run(); setCommandPalette(false); setCommandQuery(""); }}><span><strong>{command.label}</strong><small>{command.hint}</small></span></button>)}</div></div></div> : null}
  </section>;
}

function strategyLabel(value: CanvasRuntimeSnapshot["strategy"]) { return ({ priority_failover: "优先级故障切换", weighted_round_robin: "平滑加权轮询", lowest_latency: "最低延迟" })[value]; }

function remapNode(node: WorkflowNode, ids: Map<string, string>): WorkflowNode {
  const config = structuredClone(node.config);
  if (config.type === "probe") config.provider_node_id = ids.get(config.provider_node_id) ?? config.provider_node_id;
  if (config.type === "composer") config.routes = config.routes.map((route) => ({ ...route, candidates: route.candidates.map((candidate) => ({ ...candidate, provider_node_id: ids.get(candidate.provider_node_id) ?? candidate.provider_node_id })) }));
  if (config.type === "group") config.member_ids = config.member_ids.map((id) => ids.get(id)).filter((id): id is string => Boolean(id));
  return { ...node, id: ids.get(node.id)!, name: `${node.name} 副本`, config };
}
function edgeType(nodes: WorkflowNode[], edge: WorkflowGraph["edges"][number]): "candidate" | "service_plan" | "health_signal" { return nodes.find((node) => node.id === edge.from.node)?.outputs.find((port) => port.id === edge.from.port)?.data_type ?? "service_plan"; }
