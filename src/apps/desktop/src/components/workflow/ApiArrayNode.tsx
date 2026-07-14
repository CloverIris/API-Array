import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import { Branch, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import type { NodeProps } from "@xyflow/react";
import { memo } from "react";
import type { WorkflowNodeConfig } from "../../lib/desktop";
import type { CanvasNode } from "./graphModel";
import { TypedHandle } from "./TypedHandle";

export const ApiArrayNode = memo(function ApiArrayNode({ data, selected }: NodeProps<CanvasNode>) {
  const node = data.model;
  const summary = nodeSummary(node, data.runtime);
  return <article className={`apiarray-node apiarray-node--${node.kind} ${selected ? "selected" : ""} ${node.enabled ? "" : "disabled"}`}>
    <header className="node-heading"><span className="node-kind-icon">{node.kind === "composer" || node.kind === "provider" ? <Branch /> : <Plugin />}</span><div className="node-title"><div className="node-badges"><Badge color={node.enabled ? kindColor(node.kind) : "secondary"} variant="soft">{kindLabel(node.kind)}</Badge>{summary.status ? <Badge color={summary.statusColor} variant="soft">{summary.status}</Badge> : null}</div><h3>{node.name}</h3></div></header>
    <div className="node-summary"><strong>{summary.primary}</strong><span>{summary.secondary}</span></div>
    {node.kind !== "publisher" ? <div className="node-status nodrag nowheel"><Switch checked={node.enabled} label={node.enabled ? "已启用" : "已停用"} onCheckedChange={() => data.onToggle?.(node.id)} /></div> : <div className="node-protected">总输出 · 受保护</div>}
    {node.inputs.map((port, index) => <TypedHandle key={`in:${port.id}`} id={port.id} dataType={port.data_type} direction="input" index={index} count={node.inputs.length} />)}
    {node.outputs.map((port, index) => <TypedHandle key={`out:${port.id}`} id={port.id} dataType={port.data_type} direction="output" index={index} count={node.outputs.length} />)}
  </article>;
});

function nodeSummary(node: CanvasNode["data"]["model"], runtime?: CanvasNode["data"]["runtime"]): { primary: string; secondary: string; status?: string; statusColor: "success" | "warning" | "danger" | "secondary" } {
  const config = node.config;
  if (config.type === "provider") return { primary: config.asset_id || "未绑定钱包资产", secondary: runtime ? `${runtime.latencyEwmaMs == null ? "尚无延迟样本" : `${runtime.latencyEwmaMs} ms EWMA`} · ${config.selected_models.length} 个模型` : `${config.selected_models.length} 个上游模型`, status: runtime ? healthLabel(runtime.status) : config.asset_id ? "已绑定" : "阻塞", statusColor: runtime ? healthColor(runtime.status) : config.asset_id ? "success" : "warning" };
  if (config.type === "composer") return { primary: strategyLabel(config.strategy), secondary: `${config.routes.length || "自动"} 个公开模型路由`, status: `${config.max_retries} 次重试`, statusColor: "secondary" };
  if (config.type === "middleware") return { primary: middlewareLabel(config.middleware.kind), secondary: middlewareSummary(config), statusColor: "secondary" };
  if (config.type === "probe") return { primary: config.provider_node_id || "未绑定 Provider", secondary: `${config.interval_seconds} 秒周期 · ${config.failure_threshold}/${config.recovery_threshold} 滞回`, status: config.provider_node_id ? "已绑定" : "阻塞", statusColor: config.provider_node_id ? "success" : "warning" };
  if (config.type === "publisher") return { primary: "OpenAI-compatible", secondary: config.publisher_id ? `入口 ${config.publisher_id}` : "尚未配置本地入口", statusColor: "secondary" };
  return { primary: `${config.member_ids.length} 个成员`, secondary: config.collapsed ? "已折叠" : "已展开", statusColor: "secondary" };
}

function middlewareSummary(config: Extract<WorkflowNodeConfig, { type: "middleware" }>) {
  const middleware = config.middleware;
  if (middleware.kind === "rate_limit") return `${middleware.requests_per_minute} RPM · ${middleware.max_concurrent} 并发`;
  if (middleware.kind === "model_policy") return `${middleware.allowed_models.length} 个允许模型`;
  if (middleware.kind === "budget_monitor") return `阈值 ${middleware.warning_thresholds.join(" / ")}%`;
  return "仅填充请求未提供的参数";
}
function strategyLabel(strategy: string) { return ({ priority_failover: "优先级故障切换", weighted_round_robin: "平滑加权轮询", lowest_latency: "最低延迟" } as Record<string, string>)[strategy] ?? strategy; }
function middlewareLabel(kind: string) { return ({ request_defaults: "请求默认值", model_policy: "模型策略", rate_limit: "限流", budget_monitor: "预算监控" } as Record<string, string>)[kind] ?? kind; }
function kindLabel(kind: string) { return ({ provider: "Provider", composer: "Composer", middleware: "Middleware", probe: "Probe", publisher: "Publisher", group: "Group" } as Record<string, string>)[kind] ?? kind; }
function kindColor(kind: string): "info" | "success" | "warning" | "secondary" { if (kind === "publisher") return "success"; if (kind === "composer" || kind === "middleware") return "warning"; if (kind === "group") return "secondary"; return "info"; }
function healthLabel(status: string) { return ({ healthy: "健康", degraded: "降级", unhealthy: "不可用", unknown: "未知", paused: "暂停" } as Record<string, string>)[status] ?? status; }
function healthColor(status: string): "success" | "warning" | "danger" | "secondary" { if (status === "healthy") return "success"; if (status === "degraded") return "warning"; if (status === "unhealthy") return "danger"; return "secondary"; }
