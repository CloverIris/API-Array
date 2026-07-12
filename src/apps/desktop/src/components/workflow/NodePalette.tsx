import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Branch, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import type { WalletCard, WorkflowGraph } from "../../lib/desktop";

type Kind = WorkflowGraph["nodes"][number]["kind"];
export type WorkflowTemplate = "single" | "failover" | "guarded";
const controls: Array<{ kind: Exclude<Kind, "provider" | "publisher">; label: string; hint: string }> = [
  { kind: "composer", label: "Composer 综合器", hint: "候选汇聚、模型映射与故障切换" },
  { kind: "probe", label: "健康探测", hint: "无副作用检测并输出健康信号" },
  { kind: "middleware", label: "策略与规范化", hint: "预算、速率和 Canonical 预设" },
  { kind: "group", label: "节点组", hint: "纯视觉整理，不参与运行" },
];

export function NodePalette({ wallet, hasPublisher, onAddAsset, onAddControl, onApplyTemplate }: { wallet: WalletCard[]; hasPublisher: boolean; onAddAsset: (asset: WalletCard) => void; onAddControl: (kind: Kind) => void; onApplyTemplate: (template: WorkflowTemplate) => void }) {
  const [open, setOpen] = useState(false); const [query, setQuery] = useState("");
  const assets = useMemo(() => wallet.filter((item) => item.source === "asset" && `${item.name} ${item.providerId}`.toLowerCase().includes(query.trim().toLowerCase())), [query, wallet]);
  const filtered = controls.filter((item) => `${item.label} ${item.hint}`.toLowerCase().includes(query.trim().toLowerCase()));
  return <div className="node-palette-anchor"><Button color="primary" variant="soft" size="sm" aria-expanded={open} onClick={() => setOpen((value) => !value)}><Plus />添加节点</Button>{open ? <div className="node-palette" role="dialog" aria-label="添加服务编组节点"><div className="palette-heading"><Branch /><div><strong>Canonical 编组素材</strong><small>Provider 只能来自 API 钱包</small></div></div><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索钱包资产或节点" autoFocus /><div className="palette-section"><div className="palette-section-title"><span>API 钱包 Provider</span><Badge color="secondary" variant="soft">{assets.length}</Badge></div>{assets.map((asset) => <button key={asset.id} className="palette-item" onClick={() => { onAddAsset(asset); setOpen(false); }}><strong>{asset.name}</strong><small>{asset.configured ? asset.enabled ? "可作为 Candidate 加入 Composer" : "资产已停用" : "缺少 Key；仅可保存草稿"}</small></button>)}{!assets.length ? <p className="palette-empty">没有匹配的钱包资产。</p> : null}</div><div className="palette-section"><div className="palette-section-title"><span>编组控制</span></div>{filtered.map((item) => <button key={item.kind} className="palette-item" onClick={() => { onAddControl(item.kind); setOpen(false); }}><strong>{item.label}</strong><small>{item.hint}</small></button>)}</div><div className="palette-section"><div className="palette-section-title"><span>基础模板</span></div><button className="palette-item" onClick={() => { onApplyTemplate("single"); setOpen(false); }}><strong>单 Provider 直通</strong><small>Provider → Composer → Publisher</small></button><button className="palette-item" onClick={() => { onApplyTemplate("failover"); setOpen(false); }}><strong>多模型或主备编组</strong><small>多个 Candidate 汇聚到 Composer</small></button><button className="palette-item" onClick={() => { onApplyTemplate("guarded"); setOpen(false); }}><strong>受控主备路由</strong><small>增加 Probe 与 Middleware</small></button></div><div className="palette-output"><strong>Canvas 总输出器</strong><span>{hasPublisher ? "已存在且受保护" : "缺失；Graph V2 校验会拒绝"}</span></div></div> : null}</div>;
}
