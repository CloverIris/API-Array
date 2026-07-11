import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Branch, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import type { WorkflowGraph } from "../../lib/desktop";

const nodeKinds: Array<{ kind: WorkflowGraph["nodes"][number]["kind"]; label: string; hint: string }> = [
  { kind: "adapter", label: "Adapter", hint: "上游协议适配" }, { kind: "probe", label: "Probe", hint: "健康与能力检测" },
  { kind: "transform", label: "Transform", hint: "规范化请求" }, { kind: "router", label: "Router", hint: "主备与路由" },
  { kind: "guard", label: "Guard", hint: "规则与安全边界" }, { kind: "publisher", label: "Publisher", hint: "本地发布出口" },
  { kind: "group", label: "Group", hint: "折叠节点组" },
];

export function NodePalette({ onAdd }: { onAdd: (kind: WorkflowGraph["nodes"][number]["kind"]) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const filtered = nodeKinds.filter((item) => `${item.label} ${item.hint}`.toLowerCase().includes(query.trim().toLowerCase()));
  return <div className="node-palette-anchor"><Button color="primary" variant="soft" size="sm" aria-expanded={open} onClick={() => setOpen((value) => !value)}><Plus />添加节点</Button>{open ? <div className="node-palette" role="dialog" aria-label="添加工作流节点"><div className="palette-heading"><Branch /><div><strong>节点类型</strong><small>添加到当前视口</small></div></div><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索节点" aria-label="搜索节点类型" autoFocus />{filtered.map((item) => <button key={item.kind} className="palette-item" onClick={() => { onAdd(item.kind); setOpen(false); }}><strong>{item.label}</strong><small>{item.hint}</small></button>)}{!filtered.length ? <p className="palette-empty">没有匹配的节点类型</p> : null}</div> : null}</div>;
}
