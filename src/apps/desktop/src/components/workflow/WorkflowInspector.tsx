import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import type { WalletCard, WorkflowGraph } from "../../lib/desktop";
import { StateLine } from "../shared";

type Node = WorkflowGraph["nodes"][number];
export type SelectedWorkflowItem = { kind: "node"; node: Node; wallet: WalletCard[]; onUpdate: (id: string, patch: Partial<Node>) => void } | { kind: "edge"; edge: WorkflowGraph["edges"][number] } | null;

export function WorkflowInspector({ selected }: { selected: SelectedWorkflowItem }) {
  if (!selected) return <><StateLine label="选择" value="未选择节点" /><div className="next-step"><strong>服务编组画布</strong><p>Provider 候选只能连接 Composer；服务计划从 Composer 流向 Middleware 和 Publisher。</p></div></>;
  if (selected.kind === "edge") return <><Badge color="info" variant="soft">服务计划连接</Badge><StateLine label="ID" value={selected.edge.id} /><StateLine label="来源" value={`${selected.edge.from.node}.${selected.edge.from.port}`} /><StateLine label="目标" value={`${selected.edge.to.node}.${selected.edge.to.port}`} /></>;
  const { node, wallet, onUpdate } = selected;
  const config = node.config ?? {};
  const patchConfig = (patch: Record<string, unknown>) => onUpdate(node.id, { config: { ...config, ...patch } });
  return <div className="workflow-inspector">
    <Badge color={node.enabled ? "success" : "secondary"} variant="soft">{kindLabel(node.kind)}</Badge>
    <label className="field-label">节点名称<Input value={node.name} onChange={(event) => onUpdate(node.id, { name: event.target.value })} /></label>
    <StateLine label="节点 ID" value={node.id} /><StateLine label="端口" value={`${node.inputs.length} 入 / ${node.outputs.length} 出`} />
    {node.kind === "provider" ? <ProviderFields wallet={wallet} config={config} onChange={patchConfig} /> : null}
    {node.kind === "composer" ? <><NumberField label="超时（毫秒）" value={num(config.timeout_ms, 30000)} min={1000} onChange={(timeout_ms) => patchConfig({ timeout_ms })} /><NumberField label="最大重试" value={num(config.max_retries, 2)} min={0} max={10} onChange={(max_retries) => patchConfig({ max_retries })} /><Switch checked={Boolean(config.allow_capability_degradation)} label="允许显式能力降级" onCheckedChange={(allow_capability_degradation) => patchConfig({ allow_capability_degradation })} /><StateLine label="Canonical 策略" value="按优先级故障切换" /></> : null}
    {node.kind === "probe" ? <><Switch checked={config.safe_only !== false} label="仅无副作用探测" onCheckedChange={(safe_only) => patchConfig({ safe_only })} /><NumberField label="探测间隔（秒）" value={num(config.interval_seconds, 300)} min={30} onChange={(interval_seconds) => patchConfig({ interval_seconds })} /></> : null}
    {node.kind === "middleware" ? <><label className="field-label">受控预设<Select value={str(config.preset, "budget_and_rate")} options={[{ value: "budget_and_rate", label: "预算与速率限制" }, { value: "model_alias", label: "模型别名" }, { value: "canonical_defaults", label: "Canonical 默认参数" }]} onChange={(option) => patchConfig({ preset: option.value })} /></label><NumberField label="预算预警（%）" value={num(config.budget_warning_percent, 80)} min={1} max={100} onChange={(budget_warning_percent) => patchConfig({ budget_warning_percent })} /></> : null}
    {node.kind === "group" ? <Switch checked={Boolean(config.collapsed)} label="折叠节点组" onCheckedChange={(collapsed) => patchConfig({ collapsed })} /> : null}
    {node.kind === "publisher" ? <div className="next-step"><strong>OpenAI-compatible 出口</strong><p>总输出器不可停用或删除。地址与 Token 在“出口”页配置，路由完全由 Composer 编译。</p></div> : <Switch checked={node.enabled} label={node.enabled ? "已启用" : "已停用"} onCheckedChange={(enabled) => onUpdate(node.id, { enabled })} />}
    <div className="next-step"><strong>Canonical 安全边界</strong><p>节点只保存钱包资产 ID、模型映射和策略；密钥与请求正文不会进入 Graph。</p></div>
  </div>;
}

function ProviderFields({ wallet, config, onChange }: { wallet: WalletCard[]; config: Record<string, unknown>; onChange: (patch: Record<string, unknown>) => void }) { const assets = wallet.filter((item) => item.source === "asset"); const assetId = str(config.asset_id, ""); const selected = assets.find((item) => item.id === assetId); return <><label className="field-label">钱包资产<Select value={assetId} options={assets.map((asset) => ({ value: asset.id, label: asset.name }))} onChange={(option) => onChange({ asset_id: option.value })} /></label><label className="field-label">公开模型<Input value={str(config.public_model, "default")} onChange={(event) => onChange({ public_model: event.target.value })} /></label><label className="field-label">上游模型<Input value={str(config.upstream_model, "default")} onChange={(event) => onChange({ upstream_model: event.target.value })} /></label><NumberField label="候选优先级" value={num(config.priority, 0)} min={0} onChange={(priority) => onChange({ priority })} /><Badge color={selected?.configured && selected.enabled ? "success" : "warning"} variant="soft">{selected?.configured ? selected.enabled ? "资产已就绪" : "资产已停用" : "缺少 Key"}</Badge></>; }
function NumberField({ label, value, min, max, onChange }: { label: string; value: number; min?: number; max?: number; onChange: (value: number) => void }) { return <label className="field-label">{label}<Input type="number" value={String(value)} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))} /></label>; }
function num(value: unknown, fallback: number) { return typeof value === "number" && Number.isFinite(value) ? value : fallback; }
function str(value: unknown, fallback: string) { return typeof value === "string" ? value : fallback; }
function kindLabel(kind: Node["kind"]) { return ({ provider: "钱包 Provider", composer: "Composer 综合器", middleware: "策略与规范化", probe: "健康探测", publisher: "总输出器", group: "节点组" })[kind]; }
