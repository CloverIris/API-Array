import { describe, expect, it } from "vitest";
import type { WorkflowGraph, WorkspaceUiState } from "../../lib/desktop";
import { canvasToGraph, connectionCreatesCycle, graphToCanvas, layeredPositions, newWorkflowNode } from "./graphModel";

const graph: WorkflowGraph = {
  schema_version: 1,
  id: "main",
  nodes: [
    { id: "source", name: "Source", kind: "transform", enabled: true, inputs: [], outputs: [{ id: "request", data_type: "request" }], config: {} },
    { id: "target", name: "Target", kind: "router", enabled: true, inputs: [{ id: "request", data_type: "request" }], outputs: [], config: {} },
  ],
  edges: [{ id: "source-target", from: { node: "source", port: "request" }, to: { node: "target", port: "request" } }],
};

const uiState: WorkspaceUiState = {
  schemaVersion: 2,
  themePreference: "system",
  viewMode: "simple",
  lastPage: "overview",
  workspaceIntent: "manage_apis",
  shell: { leftSidebarCollapsed: false, rightInspectorOpen: true, rightInspectorPinned: true, leftWidth: 230, rightWidth: 310 },
  workflows: { main: { viewport: { x: 10, y: 20, zoom: 1.2 }, nodePositions: { source: { x: 444, y: 222 } }, collapsedGroups: [] } },
};

describe("workflow graph canvas adapter", () => {
  it("round-trips runtime semantics without placing UI positions in WorkflowGraph", () => {
    const canvas = graphToCanvas(graph, uiState);
    expect(canvas.nodes[0].position).toEqual({ x: 444, y: 222 });
    const restored = canvasToGraph(graph, canvas.nodes, canvas.edges);
    expect(restored).toEqual(graph);
    expect(JSON.stringify(restored)).not.toContain("position");
    expect(JSON.stringify(restored)).not.toContain("viewport");
  });

  it("detects direct and transitive cycles", () => {
    const { edges } = graphToCanvas(graph);
    expect(connectionCreatesCycle(edges, "target", "source")).toBe(true);
    expect(connectionCreatesCycle(edges, "source", "source")).toBe(true);
    expect(connectionCreatesCycle(edges, "target", "third")).toBe(false);
  });

  it("produces deterministic left-to-right DAG positions", () => {
    const first = layeredPositions(graph);
    const second = layeredPositions(graph);
    expect(first).toEqual(second);
    expect(first.target.x).toBeGreaterThan(first.source.x);
  });

  it("creates typed, script-free nodes for every supported kind", () => {
    for (const kind of ["adapter", "probe", "transform", "router", "guard", "publisher", "group"] as const) {
      const node = newWorkflowNode(kind, 0);
      expect(node.kind).toBe(kind);
      expect(node.config).toEqual({});
      expect(node.inputs.length + node.outputs.length).toBeGreaterThan(0);
    }
  });
});
