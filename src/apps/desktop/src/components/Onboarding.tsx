import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { ApiKeys, Key } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import { initializeWorkspace, type DesktopSnapshot, type WorkspaceIntent } from "../lib/desktop";
import { WindowTitleBar } from "./shell/WindowTitleBar";
import { readError } from "./shared";

export function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot, intent: WorkspaceIntent) => void }) {
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
  const create = async () => { setBusy(true); setError(null); try { onCreated(await initializeWorkspace(name), "manage_apis"); } catch (reason) { setError(readError(reason)); } finally { setBusy(false); } };
  return <main className="oobe"><WindowTitleBar title="欢迎使用 API ARRAY" /><section className="oobe-card"><div className="brand-mark"><Key /></div><Badge color="info" variant="soft">本地 · 安全 · 可审计</Badge><p className="eyebrow">API WALLET</p><h1>先把第一个 API 放进钱包</h1><p className="lead">下一步只需要选择 Provider、填写端点与 Key。API ARRAY 会为它创建独立本地入口和审计记录；节点编组可以稍后再做。</p><div className="setup-steps"><div className="setup-step current"><span className="step-index">1</span><span><strong>创建本地工作区</strong><small>所有配置与审计只保存在此电脑。</small></span><ApiKeys /></div><div className="setup-step"><span className="step-index">2</span><span><strong>添加第一个 API</strong><small>保存端点、上游 Key 与独立本地 Token。</small></span></div><div className="setup-step"><span className="step-index">3</span><span><strong>复制本地调用模板</strong><small>通过 API ARRAY 网关直接使用并审计。</small></span></div></div><label className="field-label">工作区名称<Input value={name} maxLength={80} onChange={(event) => setName(event.target.value)} /></label>{error ? <p className="error-message" role="alert">{error}</p> : null}<Button color="primary" block loading={busy} disabled={!name.trim()} onClick={() => void create()}>创建钱包并继续</Button></section></main>;
}
