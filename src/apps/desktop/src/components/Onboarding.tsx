import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Textarea } from "@openai/apps-sdk-ui/components/Textarea";
import { ApiKeys, ArrowLeft, Branch, FileDocument, Key, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import { importWorkspace, initializeWorkspace, type DesktopSnapshot, type WorkspaceIntent } from "../lib/desktop";
import { WindowTitleBar } from "./shell/WindowTitleBar";
import { readError } from "./shared";

const intents: Array<{ id: WorkspaceIntent; title: string; text: string; icon: typeof ApiKeys }> = [
  { id: "manage_apis", title: "管理我的 API", text: "添加并检查已有的供应商和密钥。", icon: ApiKeys },
  { id: "unified_endpoint", title: "创建统一 AI 端点", text: "把多个模型发布为一个本地地址。", icon: Plugin },
  { id: "reliability", title: "保证 API 不掉线", text: "配置主线路、备用线路和自动切换。", icon: Branch },
  { id: "import", title: "导入现有工作区", text: "打开已分享的工作区或模板。", icon: FileDocument },
];

export function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot, intent: WorkspaceIntent) => void }) {
  const [intent, setIntent] = useState<WorkspaceIntent | null>(null);
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [workspaceJson, setWorkspaceJson] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
  const finish = async () => { if (!intent) return; setBusy(true); setError(null); try { const snapshot = intent === "import" ? await importWorkspace(workspaceJson) : await initializeWorkspace(name); onCreated(snapshot, intent); } catch (reason) { setError(readError(reason)); } finally { setBusy(false); } };
  return <main className="oobe"><WindowTitleBar title="欢迎使用 API ARRAY" /><section className="oobe-card">{intent ? <><Button className="oobe-back" color="secondary" variant="ghost" size="sm" onClick={() => setIntent(null)}><ArrowLeft />返回</Button><div className="brand-mark"><Key /></div><Badge color="info" variant="soft">本地、安全、可审计</Badge><h1>{intents.find((item) => item.id === intent)?.title}</h1><p className="lead">{intent === "import" ? "粘贴另一个 API ARRAY 工作区导出的 JSON。Secret 明文不会包含在导出文件中。" : "创建工作区后，我们会按清晰步骤引导你完成第一次本地调用。"}</p>{intent === "import" ? <label className="field-label">工作区 JSON<Textarea value={workspaceJson} onChange={(event) => setWorkspaceJson(event.target.value)} rows={12} /></label> : <label className="field-label">工作区名称<Input value={name} maxLength={80} onChange={(event) => setName(event.target.value)} /></label>}{error ? <p className="error-message" role="alert">{error}</p> : null}<Button color="primary" block loading={busy} disabled={intent === "import" ? !workspaceJson.trim() : !name.trim()} onClick={() => void finish()}>{intent === "import" ? "导入并打开" : "创建并开始"}</Button></> : <><div className="brand-mark"><Key /></div><p className="eyebrow">本地 API 控制台</p><h1>你想先做什么？</h1><p className="lead">选择目标即可。你不需要先理解节点、端口或路由规则。</p><div className="intent-grid">{intents.map((item) => { const Icon = item.icon; return <button key={item.id} className="intent-card" onClick={() => setIntent(item.id)}><Icon /><span><strong>{item.title}</strong><small>{item.text}</small></span></button>; })}</div>{error ? <p className="error-message" role="alert">{error}</p> : null}</>}</section></main>;
}
