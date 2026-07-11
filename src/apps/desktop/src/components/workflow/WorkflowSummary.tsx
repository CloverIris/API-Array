import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Branch, CheckCircle, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { getWorkflowGraph, type DesktopSnapshot, type WorkflowGraph } from "../../lib/desktop";
import { EmptyPage, readError } from "../shared";

const kindLabels: Record<string, string> = { adapter: "Provider 入口", probe: "健康检测", transform: "请求转换", router: "路由与回退", guard: "安全规则", publisher: "本地 Publisher", group: "节点组" };

export function WorkflowSummary({ control, onProfessional }: { control: NonNullable<DesktopSnapshot["control"]>; onProfessional: () => void }) {
  const [graph, setGraph] = useState<WorkflowGraph | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { void getWorkflowGraph().then(setGraph).catch((reason) => setError(readError(reason))); }, []);
  if (error) return <p className="error-message" role="alert">{error}</p>;
  if (!graph) return <EmptyPage page="正在整理工作流" hint="系统正在从当前运行模型生成简洁投影。" icon={Branch} />;
  if (!graph.nodes.length) return <section className="summary-empty"><EmptyPage page="还没有可解释的请求路径" hint="创建 Publisher 后，系统会自动生成 Provider、探测、路由和发布节点。" icon={Branch} /><Button color="primary" onClick={onProfessional}>打开专业画布</Button></section>;
  const ordered = topologicalNodes(graph);
  const disabled = graph.nodes.filter((node) => !node.enabled);
  const complex = graph.nodes.some((node) => node.kind === "transform" || node.kind === "guard" || node.kind === "group");
  return <div className="workflow-summary"><section className="summary-heading"><div><p className="eyebrow">Simple projection</p><h1>当前请求如何到达上游</h1><p>这是专业工作流的简洁投影，所有修改仍使用同一份运行模型。</p></div><Button color="primary" variant="soft" onClick={onProfessional}><Branch />在专业画布中编辑</Button></section>{complex ? <div className="summary-warning"><Badge color="warning" variant="soft">包含高级配置</Badge><span>当前图包含转换、规则或节点组；简洁视图仅展示安全摘要。</span></div> : null}<div className="request-path">{ordered.map((node, index) => <article key={node.id} className={`path-stage ${node.enabled ? "" : "disabled"}`}><span className="path-stage-index">{index + 1}</span><div><small>{kindLabels[node.kind] ?? node.kind}</small><strong>{node.name}</strong><span>{node.enabled ? "已启用" : "已停用"}</span></div>{index < ordered.length - 1 ? <i aria-hidden="true">→</i> : null}</article>)}</div><div className="summary-grid"><article><CheckCircle /><div><span>结构状态</span><strong>{disabled.length ? `${disabled.length} 个节点停用` : "全部关键节点启用"}</strong></div></article><article><Plugin /><div><span>Publisher</span><strong>{control.publisherCount} 个本地出口</strong></div></article><article><Branch /><div><span>连接</span><strong>{graph.edges.length} 条类型化路径</strong></div></article></div></div>;
}

function topologicalNodes(graph: WorkflowGraph) {
  const indegree = new Map(graph.nodes.map((node) => [node.id, 0]));
  const outgoing = new Map(graph.nodes.map((node) => [node.id, [] as string[]]));
  for (const edge of graph.edges) { indegree.set(edge.to.node, (indegree.get(edge.to.node) ?? 0) + 1); outgoing.get(edge.from.node)?.push(edge.to.node); }
  const queue = graph.nodes.filter((node) => indegree.get(node.id) === 0).map((node) => node.id);
  const result: string[] = [];
  while (queue.length) { const id = queue.shift()!; result.push(id); for (const next of outgoing.get(id) ?? []) { indegree.set(next, (indegree.get(next) ?? 1) - 1); if (indegree.get(next) === 0) queue.push(next); } }
  const byId = new Map(graph.nodes.map((node) => [node.id, node]));
  return (result.length === graph.nodes.length ? result : graph.nodes.map((node) => node.id)).map((id) => byId.get(id)!).filter(Boolean);
}
