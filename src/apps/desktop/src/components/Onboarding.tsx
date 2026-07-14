"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import { ApiKeys, FileDocument, Key } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import {
  createDefaultWorkspace,
  createWorkspaceAt,
  openWorkspaceAt,
  resetWorkspaceForGraphV3,
  type DesktopSnapshot,
  type WorkspaceIntent,
} from "../lib/desktop";
import { WindowTitleBar } from "./shell/WindowTitleBar";
import { readError } from "./shared";

type Mode = "default" | "custom" | "open";

export function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot, intent: WorkspaceIntent) => void }) {
  const requiresGraphV3Reset = Boolean(initialError && /Schema|schema|Graph|工作区.*版本/.test(initialError));
  const [mode, setMode] = useState<Mode>("default");
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [root, setRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
  const [resetConfirmation, setResetConfirmation] = useState("");
  const [backupFirst, setBackupFirst] = useState(true);
  const browse = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false, title: mode === "open" ? "选择已有 API ARRAY 工作区" : "选择新工作区目录" });
      if (typeof selected === "string") setRoot(selected);
    } catch (reason) { setError(readError(reason)); }
  };
  const submit = async () => {
    if (mode !== "default" && !root.trim()) { setError("请先选择工作区目录。"); return; }
    setBusy(true);
    setError(null);
    try {
      const snapshot = mode === "default"
        ? await createDefaultWorkspace(name)
        : mode === "custom"
          ? await createWorkspaceAt(root, name)
          : await openWorkspaceAt(root);
      onCreated(snapshot, "manage_apis");
    } catch (reason) { setError(readError(reason)); }
    finally { setBusy(false); }
  };

  const resetForGraphV3 = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await resetWorkspaceForGraphV3(resetConfirmation, backupFirst);
      onCreated(next, "manage_apis");
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };

  if (requiresGraphV3Reset) {
    return (
      <main className="oobe">
        <WindowTitleBar title="API ARRAY Graph V3" />
        <section className="oobe-card oobe-compact graph-reset-card" aria-labelledby="graph-reset-title">
          <div className="brand-mark"><Key /></div>
          <Badge color="danger" variant="soft">需要明确确认</Badge>
          <p className="eyebrow">GRAPH V3 WORKSPACE RESET</p>
          <h1 id="graph-reset-title">旧工作区无法直接升级</h1>
          <p className="lead">Graph V3 使用新的强类型节点和运行语义。为避免把旧配置错误解释为可运行服务，本版本不迁移旧 Graph。</p>
          <div className="graph-reset-impact" role="note">
            <strong>确认后将清理当前工作区中的：</strong>
            <ul>
              <li>API 钱包与对应 Windows Credential Manager 凭据</li>
              <li>审计直出端点、编组方案和 Publisher Token</li>
              <li>审计记录、体检报告与通知</li>
            </ul>
            <p>备份不包含任何 Secret 明文。取消或关闭应用不会修改旧工作区。</p>
          </div>
          <label className="switch-line">
            <Switch checked={backupFirst} onCheckedChange={setBackupFirst} />
            <span>重置前创建 SQLite 备份（推荐）</span>
          </label>
          <label className="field-label">输入“重置为 Graph V3”以确认
            <Input value={resetConfirmation} autoComplete="off" onChange={(event) => setResetConfirmation(event.target.value)} />
          </label>
          {error ? <p className="error-message" role="alert">{error}</p> : null}
          <Button color="danger" block loading={busy} disabled={resetConfirmation.trim() !== "重置为 Graph V3"} onClick={() => void resetForGraphV3()}>
            重置并创建 Graph V3 工作区
          </Button>
        </section>
      </main>
    );
  }

  return (
    <main className="oobe">
      <WindowTitleBar title="欢迎使用 API ARRAY" />
      <section className="oobe-card oobe-compact">
        <div className="brand-mark"><Key /></div>
        <Badge color="info" variant="soft">本地 SQLite · Secret 系统保护</Badge>
        <p className="eyebrow">GET STARTED</p>
        <h1>{mode === "open" ? "打开已有工作区" : "创建你的 API 钱包"}</h1>
        <p className="lead">
          {mode === "default"
            ? "直接继续即可。数据库会保存在 API ARRAY 程序目录的 workspace 文件夹中，之后可在设置里迁移。"
            : mode === "custom"
              ? "选择数据库、审计记录和编组配置的存储目录。Key 与本地 Token 不会写入数据库。"
              : "选择包含 workspace.sqlite3 的目录，验证后重新连接。"}
        </p>

        {mode !== "open" ? <label className="field-label">工作区名称<Input value={name} maxLength={80} onChange={(event) => setName(event.target.value)} /></label> : null}
        {mode !== "default" ? (
          <label className="field-label">工作区目录<div className="path-picker"><Input value={root} onChange={(event) => setRoot(event.target.value)} placeholder={mode === "custom" ? "选择一个空目录" : "选择包含 workspace.sqlite3 的目录"} /><Button color="secondary" variant="soft" onClick={() => void browse()}><FileDocument />浏览</Button></div></label>
        ) : (
          <div className="oobe-default-location"><FileDocument /><span><strong>使用默认位置</strong><small>程序目录 / workspace</small></span></div>
        )}

        {error ? <p className="error-message" role="alert">{error}</p> : null}
        <Button color="primary" block loading={busy} disabled={(mode !== "open" && !name.trim()) || (mode !== "default" && !root.trim())} onClick={() => void submit()}>
          <ApiKeys />{mode === "open" ? "验证并打开" : "创建工作区并添加 API"}
        </Button>
        <div className="oobe-secondary-actions">
          {mode !== "default" ? <Button color="secondary" variant="ghost" onClick={() => { setMode("default"); setRoot(""); }}>使用默认位置</Button> : null}
          {mode !== "custom" ? <Button color="secondary" variant="ghost" onClick={() => { setMode("custom"); setRoot(""); }}>自定义存储位置</Button> : null}
          {mode !== "open" ? <Button color="secondary" variant="ghost" onClick={() => { setMode("open"); setRoot(""); }}>打开已有工作区</Button> : null}
        </div>
      </section>
    </main>
  );
}
