import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import type { WorkflowGraph } from "../../lib/desktop";
import { StateLine } from "../shared";

export type SelectedWorkflowItem = { kind: "node"; node: WorkflowGraph["nodes"][number] } | { kind: "edge"; edge: WorkflowGraph["edges"][number] } | null;

export function WorkflowInspector({ selected, onUpdateNode }: { selected: SelectedWorkflowItem; onUpdateNode?: (id: string, patch: Partial<WorkflowGraph["nodes"][number]>) => void }) {
  if (!selected) return <><StateLine label="选择" value="未选择节点" /><div className="next-step"><strong>画布操作</strong><p>选择节点或连接后在此查看属性。拖动端口可创建类型安全的连接。</p></div></>;
  if (selected.kind === "edge") return <><Badge color="info" variant="soft">连接</Badge><StateLine label="ID" value={selected.edge.id} /><StateLine label="来源" value={`${selected.edge.from.node}.${selected.edge.from.port}`} /><StateLine label="目标" value={`${selected.edge.to.node}.${selected.edge.to.port}`} /></>;
  const node = selected.node;
  return <div className="workflow-inspector"><Badge color={node.enabled ? "success" : "secondary"} variant="soft">{node.kind}</Badge><label className="field-label">节点名称<Input value={node.name} onChange={(event) => onUpdateNode?.(node.id, { name: event.target.value })} /></label><StateLine label="节点 ID" value={node.id} /><StateLine label="输入端口" value={`${node.inputs.length} 个`} /><StateLine label="输出端口" value={`${node.outputs.length} 个`} /><Switch checked={node.enabled} label={node.enabled ? "已启用" : "已停用"} onCheckedChange={(enabled) => onUpdateNode?.(node.id, { enabled })} /><div className="next-step"><strong>Secret 安全边界</strong><p>节点配置只保存 Secret 引用。密钥值不会出现在画布数据或属性栏。</p></div></div>;
}
