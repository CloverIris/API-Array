import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import type { CandidateBinding, PublicModelRoute, WalletCard, WorkflowGraph, WorkflowNodeConfig } from "../../lib/desktop";
import { StateLine } from "../shared";

type Node = WorkflowGraph["nodes"][number];
type Edge = WorkflowGraph["edges"][number];
export type SelectedWorkflowItem =
  | { kind: "node"; node: Node; nodes: Node[]; wallet: WalletCard[]; onUpdate: (id: string, patch: Partial<Node>) => void }
  | { kind: "edge"; edge: Edge; onUpdate: (id: string, patch: Partial<Edge>) => void }
  | null;

export function WorkflowInspector({ selected }: { selected: SelectedWorkflowItem }) {
  if (!selected) return <>
    <StateLine label="选择" value="未选择节点" />
    <div className="next-step"><strong>服务计划画布</strong><p>Provider 候选进入 Composer；服务计划再按拓扑顺序经过中间件，最终到达 Publisher。</p></div>
  </>;
  if (selected.kind === "edge") return <EdgeInspector {...selected} />;
  return <NodeInspector {...selected} />;
}

function NodeInspector({ node, nodes, wallet, onUpdate }: Extract<NonNullable<SelectedWorkflowItem>, { kind: "node" }>) {
  const setConfig = (config: WorkflowNodeConfig) => onUpdate(node.id, { config });
  return <div className="workflow-inspector">
    <div className="inspector-title-row"><Badge color={node.enabled ? kindColor(node.kind) : "secondary"} variant="soft">{kindLabel(node.kind)}</Badge><Badge color={node.enabled ? "success" : "secondary"} variant="soft">{node.enabled ? "已启用" : "已停用"}</Badge></div>
    <section className="inspector-section"><h3>基础</h3>
      <label className="field-label">节点名称<Input value={node.name} onChange={(event) => onUpdate(node.id, { name: event.target.value })} /></label>
      <StateLine label="节点 ID" value={node.id} /><StateLine label="端口" value={`${node.inputs.length} 入 / ${node.outputs.length} 出`} />
      {node.kind !== "publisher" ? <Switch checked={node.enabled} label={node.enabled ? "已启用" : "已停用"} onCheckedChange={(enabled) => onUpdate(node.id, { enabled })} /> : null}
    </section>
    {node.config.type === "provider" ? <ProviderFields wallet={wallet} config={node.config} onChange={setConfig} /> : null}
    {node.config.type === "composer" ? <ComposerFields nodes={nodes} config={node.config} onChange={setConfig} /> : null}
    {node.config.type === "probe" ? <ProbeFields nodes={nodes} config={node.config} onChange={setConfig} /> : null}
    {node.config.type === "middleware" ? <MiddlewareFields config={node.config} onChange={setConfig} /> : null}
    {node.config.type === "group" ? <GroupFields config={node.config} onChange={setConfig} /> : null}
    {node.config.type === "publisher" ? <section className="inspector-section"><h3>出口</h3><div className="next-step"><strong>OpenAI-compatible 本地出口</strong><p>总输出器不可停用或删除。地址、Token 与运行状态在“出口”页配置；模型和上游完全由 Graph V3 编译。</p></div></section> : null}
    <details className="inspector-section inspector-diagnostics"><summary>诊断与安全边界</summary><p>节点只保存钱包资产 ID、模型映射和策略。上游 Key、本地 Token 和请求正文不会进入 Graph。</p></details>
  </div>;
}

function ProviderFields({ wallet, config, onChange }: { wallet: WalletCard[]; config: Extract<WorkflowNodeConfig, { type: "provider" }>; onChange: (config: WorkflowNodeConfig) => void }) {
  const assets = wallet.filter((item) => item.source === "asset");
  const selected = assets.find((item) => item.id === config.asset_id);
  return <section className="inspector-section"><h3>Provider</h3>
    <label className="field-label">钱包资产<Select value={config.asset_id} options={assets.map((asset) => ({ value: asset.id, label: asset.name }))} onChange={(option) => onChange({ ...config, asset_id: option.value })} /></label>
    <StringListField label="选中的上游模型" values={config.selected_models} placeholder="default" onChange={(selected_models) => onChange({ ...config, selected_models })} />
    <Badge color={selected?.configured && selected.enabled ? "success" : "warning"} variant="soft">{selected?.configured ? selected.enabled ? "资产与 Secret 已就绪" : "钱包资产已停用" : "缺少 Key"}</Badge>
    <p className="field-help">协议、Base URL 与能力来自钱包资产；Graph 只保存稳定资产引用。</p>
  </section>;
}

function ComposerFields({ nodes, config, onChange }: { nodes: Node[]; config: Extract<WorkflowNodeConfig, { type: "composer" }>; onChange: (config: WorkflowNodeConfig) => void }) {
  const providers = nodes.filter((node) => node.kind === "provider");
  const updateRoute = (index: number, route: PublicModelRoute) => onChange({ ...config, routes: config.routes.map((item, itemIndex) => itemIndex === index ? route : item) });
  const removeRoute = (index: number) => onChange({ ...config, routes: config.routes.filter((_, itemIndex) => itemIndex !== index) });
  return <>
    <section className="inspector-section"><h3>策略</h3>
      <label className="field-label">候选选择<Select value={config.strategy} options={[
        { value: "priority_failover", label: "优先级故障切换" },
        { value: "weighted_round_robin", label: "平滑加权轮询" },
        { value: "lowest_latency", label: "最低延迟" },
      ]} onChange={(option) => onChange({ ...config, strategy: option.value as typeof config.strategy })} /></label>
      <div className="field-grid"><NumberField label="超时（毫秒）" value={config.timeout_ms} min={1_000} max={300_000} onChange={(timeout_ms) => onChange({ ...config, timeout_ms })} /><NumberField label="最大重试" value={config.max_retries} min={0} max={10} onChange={(max_retries) => onChange({ ...config, max_retries })} /></div>
      {config.strategy === "lowest_latency" ? <NumberField label="延迟滞回（毫秒）" value={config.latency_hysteresis_ms} min={1} max={5_000} onChange={(latency_hysteresis_ms) => onChange({ ...config, latency_hysteresis_ms })} /> : null}
      <Switch checked={config.allow_capability_degradation} label="允许显式能力降级" onCheckedChange={(allow_capability_degradation) => onChange({ ...config, allow_capability_degradation })} />
    </section>
    <section className="inspector-section"><div className="section-heading"><h3>公开模型路由</h3><Button size="sm" color="secondary" variant="soft" onClick={() => onChange({ ...config, routes: [...config.routes, { public_model: `model-${config.routes.length + 1}`, candidates: [] }] })}>添加模型</Button></div>
      {!config.routes.length ? <p className="field-help">尚未显式配置路由。编译器会按 Provider 的已选模型生成直通映射。</p> : null}
      {config.routes.map((route, index) => <RouteEditor key={`${route.public_model}:${index}`} route={route} providers={providers} onChange={(next) => updateRoute(index, next)} onRemove={() => removeRoute(index)} />)}
    </section>
  </>;
}

function RouteEditor({ route, providers, onChange, onRemove }: { route: PublicModelRoute; providers: Node[]; onChange: (route: PublicModelRoute) => void; onRemove: () => void }) {
  const addCandidate = () => {
    const provider = providers.find((item) => !route.candidates.some((candidate) => candidate.provider_node_id === item.id)) ?? providers[0];
    if (!provider) return;
    onChange({ ...route, candidates: [...route.candidates, { provider_node_id: provider.id, upstream_model: route.public_model || "default", priority: route.candidates.length, weight: 1 }] });
  };
  const updateCandidate = (index: number, candidate: CandidateBinding) => onChange({ ...route, candidates: route.candidates.map((item, itemIndex) => itemIndex === index ? candidate : item) });
  return <div className="route-editor">
    <div className="route-editor-heading"><Input value={route.public_model} aria-label="公开模型名" onChange={(event) => onChange({ ...route, public_model: event.target.value })} /><Button size="sm" color="danger" variant="ghost" onClick={onRemove}>删除</Button></div>
    {route.candidates.map((candidate, index) => <div className="candidate-editor" key={`${candidate.provider_node_id}:${index}`}>
      <label className="field-label">Provider<Select value={candidate.provider_node_id} options={providers.map((provider) => ({ value: provider.id, label: provider.name }))} onChange={(option) => updateCandidate(index, { ...candidate, provider_node_id: option.value })} /></label>
      <label className="field-label">上游模型<Input value={candidate.upstream_model} onChange={(event) => updateCandidate(index, { ...candidate, upstream_model: event.target.value })} /></label>
      <div className="field-grid"><NumberField label="优先级" value={candidate.priority} min={0} max={10_000} onChange={(priority) => updateCandidate(index, { ...candidate, priority })} /><NumberField label="权重" value={candidate.weight} min={1} max={100} onChange={(weight) => updateCandidate(index, { ...candidate, weight })} /></div>
      <Button size="sm" color="secondary" variant="ghost" onClick={() => onChange({ ...route, candidates: route.candidates.filter((_, itemIndex) => itemIndex !== index) })}>移除候选</Button>
    </div>)}
    <Button size="sm" color="secondary" variant="soft" disabled={!providers.length} onClick={addCandidate}>添加候选</Button>
  </div>;
}

function ProbeFields({ nodes, config, onChange }: { nodes: Node[]; config: Extract<WorkflowNodeConfig, { type: "probe" }>; onChange: (config: WorkflowNodeConfig) => void }) {
  const providers = nodes.filter((node) => node.kind === "provider");
  return <section className="inspector-section"><h3>健康探测</h3>
    <label className="field-label">绑定 Provider<Select value={config.provider_node_id} options={providers.map((provider) => ({ value: provider.id, label: provider.name }))} onChange={(option) => onChange({ ...config, provider_node_id: option.value })} /></label>
    <Switch checked={config.safe_only} label="仅执行无副作用探测" onCheckedChange={(safe_only) => onChange({ ...config, safe_only })} />
    <div className="field-grid"><NumberField label="间隔（秒）" value={config.interval_seconds} min={30} max={86_400} onChange={(interval_seconds) => onChange({ ...config, interval_seconds })} /><NumberField label="超时（毫秒）" value={config.timeout_ms} min={1_000} max={60_000} onChange={(timeout_ms) => onChange({ ...config, timeout_ms })} /></div>
    <div className="field-grid"><NumberField label="失败阈值" value={config.failure_threshold} min={1} max={20} onChange={(failure_threshold) => onChange({ ...config, failure_threshold })} /><NumberField label="恢复阈值" value={config.recovery_threshold} min={1} max={20} onChange={(recovery_threshold) => onChange({ ...config, recovery_threshold })} /></div>
  </section>;
}

function MiddlewareFields({ config, onChange }: { config: Extract<WorkflowNodeConfig, { type: "middleware" }>; onChange: (config: WorkflowNodeConfig) => void }) {
  const kind = config.middleware.kind;
  const setKind = (next: string) => {
    const middleware = next === "request_defaults" ? { kind: "request_defaults" as const, temperature: null, top_p: null, max_output_tokens: null }
      : next === "model_policy" ? { kind: "model_policy" as const, allowed_models: [], max_output_tokens: null }
      : next === "rate_limit" ? { kind: "rate_limit" as const, requests_per_minute: 60, max_concurrent: 8 }
      : { kind: "budget_monitor" as const, warning_thresholds: [50, 80, 100] };
    onChange({ ...config, middleware });
  };
  return <section className="inspector-section"><h3>安全策略</h3>
    <label className="field-label">中间件类型<Select value={kind} options={[
      { value: "request_defaults", label: "请求默认值" }, { value: "model_policy", label: "模型策略" }, { value: "rate_limit", label: "限流" }, { value: "budget_monitor", label: "预算监控" },
    ]} onChange={(option) => setKind(option.value)} /></label>
    {config.middleware.kind === "request_defaults" ? <RequestDefaultsFields value={config.middleware} onChange={(middleware) => onChange({ ...config, middleware })} /> : null}
    {config.middleware.kind === "model_policy" ? <ModelPolicyFields value={config.middleware} onChange={(middleware) => onChange({ ...config, middleware })} /> : null}
    {config.middleware.kind === "rate_limit" ? <RateLimitFields value={config.middleware} onChange={(middleware) => onChange({ ...config, middleware })} /> : null}
    {config.middleware.kind === "budget_monitor" ? <BudgetMonitorFields value={config.middleware} onChange={(middleware) => onChange({ ...config, middleware })} /> : null}
  </section>;
}

function RequestDefaultsFields({ value, onChange }: { value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "request_defaults" }>; onChange: (value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "request_defaults" }>) => void }) { return <><OptionalNumber label="默认温度" value={value.temperature} min={0} max={2} onChange={(temperature) => onChange({ ...value, temperature })} /><OptionalNumber label="默认 Top P" value={value.top_p} min={0} max={1} onChange={(top_p) => onChange({ ...value, top_p })} /><OptionalNumber label="默认最大输出 Token" value={value.max_output_tokens} min={1} onChange={(max_output_tokens) => onChange({ ...value, max_output_tokens })} /></>; }
function ModelPolicyFields({ value, onChange }: { value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "model_policy" }>; onChange: (value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "model_policy" }>) => void }) { return <><StringListField label="允许模型" values={value.allowed_models} placeholder="model-a, model-b" onChange={(allowed_models) => onChange({ ...value, allowed_models })} /><OptionalNumber label="每模型最大输出 Token" value={value.max_output_tokens} min={1} onChange={(max_output_tokens) => onChange({ ...value, max_output_tokens })} /></>; }
function RateLimitFields({ value, onChange }: { value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "rate_limit" }>; onChange: (value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "rate_limit" }>) => void }) { return <div className="field-grid"><NumberField label="每分钟请求" value={value.requests_per_minute} min={1} max={1_000_000} onChange={(requests_per_minute) => onChange({ ...value, requests_per_minute })} /><NumberField label="最大并发" value={value.max_concurrent} min={1} max={10_000} onChange={(max_concurrent) => onChange({ ...value, max_concurrent })} /></div>; }
function BudgetMonitorFields({ value, onChange }: { value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "budget_monitor" }>; onChange: (value: Extract<import("../../lib/desktop").MiddlewareKind, { kind: "budget_monitor" }>) => void }) { return <StringListField label="预算预警阈值（%）" values={value.warning_thresholds.map(String)} placeholder="50, 80, 100" onChange={(values) => onChange({ ...value, warning_thresholds: values.map(Number).filter((item) => item > 0 && item <= 100) })} />; }

function GroupFields({ config, onChange }: { config: Extract<WorkflowNodeConfig, { type: "group" }>; onChange: (config: WorkflowNodeConfig) => void }) {
  return <section className="inspector-section"><h3>组织</h3><Switch checked={config.collapsed} label="折叠节点组" onCheckedChange={(collapsed) => onChange({ ...config, collapsed })} /><StateLine label="成员数" value={String(config.member_ids.length)} /><p className="field-help">节点组只影响画布组织，不进入 Runtime 服务计划。</p></section>;
}

function EdgeInspector({ edge, onUpdate }: Extract<NonNullable<SelectedWorkflowItem>, { kind: "edge" }>) {
  return <div className="workflow-inspector"><Badge color={edge.enabled ? "info" : "secondary"} variant="soft">服务计划连接</Badge><StateLine label="ID" value={edge.id} /><StateLine label="来源" value={`${edge.from.node}.${edge.from.port}`} /><StateLine label="目标" value={`${edge.to.node}.${edge.to.port}`} /><label className="field-label">安全标签<Input value={edge.label ?? ""} placeholder="可选审计标签" onChange={(event) => onUpdate(edge.id, { label: event.target.value || null })} /></label><Switch checked={edge.enabled} label={edge.enabled ? "连接已启用" : "连接已停用"} onCheckedChange={(enabled) => onUpdate(edge.id, { enabled })} /></div>;
}

function NumberField({ label, value, min, max, onChange }: { label: string; value: number; min?: number; max?: number; onChange: (value: number) => void }) {
  return <label className="field-label">{label}<Input type="number" value={String(value)} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}
function OptionalNumber({ label, value, min, max, onChange }: { label: string; value?: number | null; min?: number; max?: number; onChange: (value: number | null) => void }) {
  return <label className="field-label">{label}<Input type="number" value={value == null ? "" : String(value)} min={min} max={max} placeholder="沿用请求值" onChange={(event) => onChange(event.target.value === "" ? null : Number(event.target.value))} /></label>;
}
function StringListField({ label, values, placeholder, onChange }: { label: string; values: string[]; placeholder: string; onChange: (values: string[]) => void }) {
  return <label className="field-label">{label}<Input value={values.join(", ")} placeholder={placeholder} onChange={(event) => onChange(event.target.value.split(",").map((value) => value.trim()).filter(Boolean))} /></label>;
}
function kindLabel(kind: Node["kind"]) { return ({ provider: "钱包 Provider", composer: "Composer 综合器", middleware: "策略中间件", probe: "健康探测", publisher: "总输出器", group: "节点组" })[kind]; }
function kindColor(kind: Node["kind"]): "info" | "success" | "warning" | "secondary" { if (kind === "publisher") return "success"; if (kind === "composer" || kind === "middleware") return "warning"; if (kind === "group") return "secondary"; return "info"; }
