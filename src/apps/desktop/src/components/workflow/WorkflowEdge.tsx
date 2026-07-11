import { BaseEdge, EdgeLabelRenderer, getBezierPath, type EdgeProps } from "@xyflow/react";
import type { CanvasEdge } from "./graphModel";

export function WorkflowEdge(props: EdgeProps<CanvasEdge>) {
  const [path, labelX, labelY] = getBezierPath(props);
  return <><BaseEdge id={props.id} path={path} markerEnd={props.markerEnd} style={props.style} /><EdgeLabelRenderer><span className={`workflow-edge-label port-${props.data?.dataType ?? "control"}`} style={{ transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)` }}>{props.data?.dataType ?? "control"}</span></EdgeLabelRenderer></>;
}
