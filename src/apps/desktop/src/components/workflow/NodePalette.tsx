import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Branch, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import type { WalletCard, WorkflowGraph } from "../../lib/desktop";

type Kind = WorkflowGraph["nodes"][number]["kind"];
export type WorkflowTemplate = "single" | "failover" | "guarded";

const controls: Array<{ kind: Exclude<Kind, "adapter" | "publisher">; label: string; hint: string }> = [
  { kind: "router", label: "路由器", hint: "主备顺序、重试与故障切换" },
  { kind: "probe", label: "健康探测", hint: "执行无副作用的可用性检测" },
  { kind: "guard", label: "安全规则", hint: "预算预警、模型与速率边界" },
  { kind: "transform", label: "协议转换", hint: "使用内置可审计转换预设" },
  { kind: "group", label: "节点组", hint: "整理复杂编组，不改变运行语义" },
];

export function NodePalette({ wallet, hasPublisher, onAddAsset, onAddControl, onApplyTemplate }: { wallet: WalletCard[]; hasPublisher: boolean; onAddAsset: (asset: WalletCard) => void; onAddControl: (kind: Kind) => void; onApplyTemplate: (template: WorkflowTemplate) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const assets = useMemo(() => wallet.filter((item) => item.source === "asset" && `${item.name} ${item.providerId}`.toLowerCase().includes(query.trim().toLowerCase())), [query, wallet]);
  const filteredControls = controls.filter((item) => `${item.label} ${item.hint}`.toLowerCase().includes(query.trim().toLowerCase()));
  return <div className="node-palette-anchor"><Button color="primary" variant="soft" size="sm" aria-expanded={open} onClick={() => setOpen((value) => !value)}><Plus />添加节点</Button>{open ? <div className="node-palette" role="dialog" aria-label="添加编组节点"><div className="palette-heading"><Branch /><div><strong>编组素材</strong><small>节点只能引用 API 钱包中的资产</small></div></div><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索钱包资产或节点" aria-label="搜索编组素材" autoFocus /><div className="palette-section"><div className="palette-section-title"><span>API 钱包</span><Badge color="secondary" variant="soft">{assets.length}</Badge></div>{assets.map((asset) => <button key={asset.id} className="palette-item" onClick={() => { onAddAsset(asset); setOpen(false); }}><strong>{asset.name}</strong><small>{asset.configured ? asset.enabled ? "已配置，可加入编组" : "资产已停用" : "缺少 Key；可加入草稿但不能运行"}</small></button>)}{!assets.length ? <p className="palette-empty">钱包里没有匹配的已添加资产。</p> : null}</div><div className="palette-section"><div className="palette-section-title"><span>路由与控制</span></div>{filteredControls.map((item) => <button key={item.kind} className="palette-item" onClick={() => { onAddControl(item.kind); setOpen(false); }}><strong>{item.label}</strong><small>{item.hint}</small></button>)}</div><div className="palette-section"><div className="palette-section-title"><span>基础模板</span></div><button className="palette-item" onClick={() => { onApplyTemplate("single"); setOpen(false); }}><strong>单 API 直通</strong><small>一个钱包资产连接到总输出器</small></button><button className="palette-item" onClick={() => { onApplyTemplate("failover"); setOpen(false); }}><strong>主备故障切换</strong><small>两个资产通过 Router 汇聚</small></button><button className="palette-item" onClick={() => { onApplyTemplate("guarded"); setOpen(false); }}><strong>受控主备路由</strong><small>增加 Probe 与预算 Guard</small></button></div><div className="palette-output"><strong>Canvas 总输出器</strong><span>{hasPublisher ? "已存在且受保护" : "缺失；保存校验会拒绝当前图"}</span></div></div> : null}</div>;
}
