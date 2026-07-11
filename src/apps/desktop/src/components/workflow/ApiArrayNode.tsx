import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import { Plugin } from "@openai/apps-sdk-ui/components/Icon";
import type { NodeProps } from "@xyflow/react";
import type { CanvasNode } from "./graphModel";
import { TypedHandle } from "./TypedHandle";

export function ApiArrayNode({ data, selected }: NodeProps<CanvasNode>) {
  const node = data.model;
  return <article className={`apiarray-node ${selected ? "selected" : ""} ${node.enabled ? "" : "disabled"}`}><div className="node-heading"><span className="node-kind-icon"><Plugin /></span><div><Badge color={node.enabled ? kindColor(node.kind) : "secondary"} variant="soft">{node.kind}</Badge><h3>{node.name}</h3></div></div><p className="node-id">{node.id}</p><div className="node-status nodrag nowheel"><Switch checked={node.enabled} label={node.enabled ? "已启用" : "已停用"} onCheckedChange={() => data.onToggle?.(node.id)} /></div>{node.inputs.map((port, index) => <TypedHandle key={`in:${port.id}`} id={port.id} dataType={port.data_type} direction="input" index={index} count={node.inputs.length} />)}{node.outputs.map((port, index) => <TypedHandle key={`out:${port.id}`} id={port.id} dataType={port.data_type} direction="output" index={index} count={node.outputs.length} />)}</article>;
}

function kindColor(kind: string): "info" | "success" | "warning" | "secondary" {
  if (kind === "publisher") return "success";
  if (kind === "guard" || kind === "router") return "warning";
  if (kind === "group") return "secondary";
  return "info";
}
