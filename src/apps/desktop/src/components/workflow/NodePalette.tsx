import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Branch, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import type { GraphTemplateKind, WalletCard, WorkflowGraph } from "../../lib/desktop";

type Kind = WorkflowGraph["nodes"][number]["kind"];
export type WorkflowTemplate = GraphTemplateKind;
const controls: Array<{ kind: Exclude<Kind, "provider" | "publisher">; label: string; hint: string }> = [
  { kind: "composer", label: "Composer 综合器", hint: "候选汇聚、模型映射与真实路由策略" },
  { kind: "probe", label: "健康探测", hint: "无副作用检测、失败与恢复滞回" },
  { kind: "middleware", label: "策略中间件", hint: "默认值、模型策略、限流或预算监控" },
  { kind: "group", label: "节点组", hint: "纯视觉组织，不参与 Runtime" },
];
const templates: Array<{ id: WorkflowTemplate; label: string; hint: string; assets: number }> = [
  { id: "single_provider", label: "单 Provider 直通", hint: "Provider → Composer → Publisher", assets: 1 },
  { id: "priority_failover", label: "同模型主备", hint: "按健康与优先级故障切换", assets: 2 },
  { id: "multi_model", label: "多模型集合", hint: "多个公开模型共享一个出口", assets: 2 },
  { id: "weighted_round_robin", label: "加权负载均衡", hint: "平滑加权轮询，默认 70 / 30", assets: 2 },
  { id: "lowest_latency", label: "最低延迟路由", hint: "Probe + EWMA 延迟与滞回", assets: 2 },
  { id: "failover_with_probe", label: "主备 + Probe", hint: "主动健康探测驱动主备切换", assets: 2 },
  { id: "guarded_failover", label: "受控高可用", hint: "主备 + Probe + 限流 + 预算预警", assets: 2 },
];

export function NodePalette({ wallet, hasPublisher, onAddAsset, onAddControl, onApplyTemplate }: { wallet: WalletCard[]; hasPublisher: boolean; onAddAsset: (asset: WalletCard) => void; onAddControl: (kind: Kind) => void; onApplyTemplate: (template: WorkflowTemplate) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const assets = useMemo(() => wallet.filter((item) => item.source === "asset" && `${item.name} ${item.providerId}`.toLowerCase().includes(query.trim().toLowerCase())), [query, wallet]);
  const filtered = controls.filter((item) => `${item.label} ${item.hint}`.toLowerCase().includes(query.trim().toLowerCase()));
  return <div className="node-palette-anchor"><Button color="primary" variant="soft" size="sm" aria-expanded={open} onClick={() => setOpen((value) => !value)}><Plus />添加节点</Button>{open ? <div className="node-palette" role="dialog" aria-label="添加服务编组节点">
    <div className="palette-heading"><Branch /><div><strong>Graph V3 编组素材</strong><small>Provider 只能来自 API 钱包</small></div></div>
    <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索钱包资产或节点" autoFocus />
    <div className="palette-scroll">
      <div className="palette-section"><div className="palette-section-title"><span>钱包资产</span><Badge color="secondary" variant="soft">{assets.length}</Badge></div>{assets.map((asset) => <button key={asset.id} className="palette-item" onClick={() => { onAddAsset(asset); setOpen(false); }}><strong>{asset.name}</strong><small>{asset.configured ? asset.enabled ? "作为候选加入 Composer" : "资产已停用" : "缺少 Key；仅可保存草稿"}</small></button>)}{!assets.length ? <p className="palette-empty">没有匹配的钱包资产。</p> : null}</div>
      <div className="palette-section"><div className="palette-section-title"><span>综合与策略</span></div>{filtered.map((item) => <button key={item.kind} className="palette-item" onClick={() => { onAddControl(item.kind); setOpen(false); }}><strong>{item.label}</strong><small>{item.hint}</small></button>)}</div>
      <div className="palette-section"><div className="palette-section-title"><span>Core 模板</span></div>{templates.map((template) => <button key={template.id} className="palette-item" disabled={assets.length < template.assets} onClick={() => { onApplyTemplate(template.id); setOpen(false); }}><strong>{template.label}</strong><small>{template.hint} · 需要 {template.assets} 个资产</small></button>)}</div>
    </div>
    <div className="palette-output"><strong>编组方案总输出</strong><span>{hasPublisher ? "已存在且受保护" : "缺失；Graph V3 校验将阻止保存"}</span></div>
  </div> : null}</div>;
}
