"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { ApiKeys, FileDocument, Key } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import {
  createDefaultWorkspace,
  createWorkspaceAt,
  openWorkspaceAt,
  type DesktopSnapshot,
  type WorkspaceIntent,
} from "../lib/desktop";
import { WindowTitleBar } from "./shell/WindowTitleBar";
import { readError } from "./shared";

type Mode = "default" | "custom" | "open";

export function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot, intent: WorkspaceIntent) => void }) {
  const [mode, setMode] = useState<Mode>("default");
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [root, setRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
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
