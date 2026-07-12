import { BaseEdge, EdgeLabelRenderer, getBezierPath, type EdgeProps } from "@xyflow/react";
import type { CanvasEdge } from "./graphModel";

export function WorkflowEdge(props: EdgeProps<CanvasEdge>) {
  const [path, labelX, labelY] = getBezierPath(props);
  const type = props.data?.dataType ?? "service_plan";
  const label = ({ candidate: "候选", service_plan: "服务计划", health_signal: "健康信号" } as Record<string, string>)[type] ?? type;
  return <><BaseEdge id={props.id} path={path} markerEnd={props.markerEnd} style={props.style} /><EdgeLabelRenderer><span className={`workflow-edge-label port-${type}`} style={{ transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)` }}>{label}</span></EdgeLabelRenderer></>;
}
