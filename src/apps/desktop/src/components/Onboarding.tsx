import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { SegmentedControl } from "@openai/apps-sdk-ui/components/SegmentedControl";
import { ApiKeys, FileDocument, Key } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import { createWorkspaceAt, openWorkspaceAt, type DesktopSnapshot, type WorkspaceIntent } from "../lib/desktop";
import { WindowTitleBar } from "./shell/WindowTitleBar";
import { readError } from "./shared";

export function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot, intent: WorkspaceIntent) => void }) {
  const [mode, setMode] = useState<"create" | "open">("create");
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [root, setRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
  const browse = async () => { try { const { open } = await import("@tauri-apps/plugin-dialog"); const selected = await open({ directory: true, multiple: false, title: mode === "create" ? "选择新工作区目录" : "选择已有 API ARRAY 工作区" }); if (typeof selected === "string") setRoot(selected); } catch (reason) { setError(readError(reason)); } };
  const submit = async () => { if (!root.trim()) { setError("请先选择工作区目录。"); return; } setBusy(true); setError(null); try { const snapshot = mode === "create" ? await createWorkspaceAt(root, name) : await openWorkspaceAt(root); onCreated(snapshot, "manage_apis"); } catch (reason) { setError(readError(reason)); } finally { setBusy(false); } };
  return <main className="oobe">
    <WindowTitleBar title="欢迎使用 API ARRAY" />
    <section className="oobe-card">
      <div className="brand-mark"><Key /></div><Badge color="info" variant="soft">SQLite 工作区 · Secret 系统保护</Badge>
      <p className="eyebrow">WORKSPACE</p><h1>{mode === "create" ? "创建你的 API 钱包" : "重新连接已有工作区"}</h1>
      <p className="lead">工作区数据库、审计和编组配置保存在你选择的目录；API Key 和本地 Token 仍由 Windows Credential Manager 单独保护。</p>
      <SegmentedControl value={mode} onChange={(value) => setMode(value as "create" | "open")} aria-label="工作区打开方式" block><SegmentedControl.Option value="create">新建工作区</SegmentedControl.Option><SegmentedControl.Option value="open">打开已有工作区</SegmentedControl.Option></SegmentedControl>
      {mode === "create" ? <label className="field-label">工作区名称<Input value={name} maxLength={80} onChange={(event) => setName(event.target.value)} /></label> : null}
      <label className="field-label">工作区目录<div className="path-picker"><Input value={root} onChange={(event) => setRoot(event.target.value)} placeholder={mode === "create" ? "选择一个空目录" : "选择包含 workspace.sqlite3 的目录"} /><Button color="secondary" variant="soft" onClick={() => void browse()}><FileDocument />浏览</Button></div></label>
      <div className="setup-steps"><div className="setup-step current"><span className="step-index">1</span><span><strong>连接 SQLite 工作区</strong><small>数据库损坏时只进入恢复状态，不会自动删除。</small></span><ApiKeys /></div><div className="setup-step"><span className="step-index">2</span><span><strong>添加第一个 API</strong><small>凭据只写入 Windows Credential Manager。</small></span></div><div className="setup-step"><span className="step-index">3</span><span><strong>直出或编组</strong><small>通过统一 LocalGateway 使用并审计。</small></span></div></div>
      {error ? <p className="error-message" role="alert">{error}</p> : null}<Button color="primary" block loading={busy} disabled={!root.trim() || (mode === "create" && !name.trim())} onClick={() => void submit()}>{mode === "create" ? "创建工作区并继续" : "验证并打开工作区"}</Button>
    </section>
  </main>;
}
