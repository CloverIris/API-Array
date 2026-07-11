"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { SegmentedControl } from "@openai/apps-sdk-ui/components/SegmentedControl";
import { Textarea } from "@openai/apps-sdk-ui/components/Textarea";
import { ArrowRotateCcw, Branch, Code, History, Pause, Play, Plugin, Stop } from "@openai/apps-sdk-ui/components/Icon";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createCanvasPublisher, getAuditRecords, getCanvasSnapshot, getPublisherTemplates, getWalletGallery, pauseCanvas, refreshCanvas, runCanvas, stopCanvas,
  type CanvasSnapshot, type CanvasTab, type CodeTemplate, type DesktopSnapshot, type ExecutionTrace, type WalletCard, type WorkspaceUiState,
} from "../../lib/desktop";
import { readError } from "../shared";
import { WorkflowCanvas } from "../workflow/WorkflowCanvas";
import type { SelectedWorkflowItem } from "../workflow/WorkflowInspector";

const tabs: Array<{ id: CanvasTab; label: string }> = [
  { id: "overview", label: "概览" }, { id: "workflow", label: "编排" }, { id: "routes", label: "API 与路由" }, { id: "publisher", label: "Publisher" }, { id: "docs", label: "活文档" }, { id: "runs", label: "运行记录" },
];

export function CanvasWorkspace({ projectId, canvasId, tab, uiState, onTab, onUiState, onDesktopSnapshot, onWorkflowSelection, onChanged }: { projectId: string; canvasId: string; tab: CanvasTab; uiState: WorkspaceUiState; onTab: (tab: CanvasTab) => void; onUiState: (state: WorkspaceUiState) => void; onDesktopSnapshot: (snapshot: DesktopSnapshot) => void; onWorkflowSelection: (selection: SelectedWorkflowItem) => void; onChanged: () => void }) {
  const [snapshot, setSnapshot] = useState<CanvasSnapshot | null>(null);
  const [wallet, setWallet] = useState<WalletCard[]>([]);
  const [templates, setTemplates] = useState<CodeTemplate[]>([]);
  const [records, setRecords] = useState<ExecutionTrace[]>([]);
  const [language, setLanguage] = useState("");
  const [dialog, setDialog] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [provider, setProvider] = useState("");
  const [publisherId, setPublisherId] = useState(`${canvasId}-api`);
  const [model, setModel] = useState("default");
  const [port, setPort] = useState("6188");
  const [token, setToken] = useState("");

  const load = useCallback(async () => {
    try {
      const [next, assets, audits] = await Promise.all([getCanvasSnapshot(projectId, canvasId), getWalletGallery(), getAuditRecords(500)]);
      setSnapshot(next); setWallet(assets); setRecords(next.publisher ? audits.filter((record) => record.publisher_id === next.publisher?.id) : []);
      if (next.publisher) { const docs = await getPublisherTemplates(next.publisher.id); setTemplates(docs); setLanguage((current) => current || docs[0]?.language || ""); } else setTemplates([]);
    } catch (reason) { setError(readError(reason)); }
  }, [canvasId, projectId]);
  useEffect(() => { void load(); }, [load]);

  const placed = useMemo(() => wallet.filter((asset) => asset.providerInstanceId && snapshot?.providerInstanceIds.includes(asset.providerInstanceId)), [snapshot, wallet]);
  const ready = placed.filter((asset) => asset.configured && asset.enabled && asset.providerInstanceId);
  const lifecycle = snapshot?.publisher?.status ?? "draft";
  const dirty = Boolean(snapshot && snapshot.canvas.draftRevision !== snapshot.canvas.appliedRevision);
  const execute = async (key: string, action: () => Promise<DesktopSnapshot | unknown>) => { setBusy(key); setError(null); try { const result = await action(); if (isDesktopSnapshot(result)) onDesktopSnapshot(result); await load(); onChanged(); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };
  const start = () => { if (!snapshot?.publisher) { setProvider(ready[0]?.providerInstanceId ?? ""); setDialog(true); return; } void execute("run", () => runCanvas(projectId, canvasId)); };
  const publishAndRun = async () => { if (!provider || !token.trim()) return; setBusy("publish"); setError(null); try { const next = await createCanvasPublisher({ projectId, canvasId, id: publisherId, name: publisherId, providerInstance: provider, publicModel: model, upstreamModel: model, port: Number(port), token }); onDesktopSnapshot(next); setToken(""); setDialog(false); await runCanvas(projectId, canvasId).then(onDesktopSnapshot); await load(); onChanged(); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };

  if (!snapshot) return <section className="canvas-workspace"><p className="muted">正在打开 Canvas…</p>{error ? <p className="error-message" role="alert">{error}</p> : null}</section>;
  return <section className="canvas-workspace">
    <header className="canvas-workspace-header"><div><p className="eyebrow">{snapshot.projectName} / Canvas</p><div className="canvas-title-line"><h1>{snapshot.canvas.name}</h1><Badge color={lifecycle === "running" ? "success" : lifecycle === "paused" ? "warning" : lifecycle === "failed" ? "danger" : "secondary"} variant="soft">{lifecycle}</Badge>{dirty ? <Badge color="info" variant="soft">未应用草稿 r{snapshot.canvas.draftRevision}</Badge> : <Badge color="secondary" variant="soft">已应用 r{snapshot.canvas.appliedRevision}</Badge>}</div></div><div className="canvas-runtime-actions"><Button color="success" size="sm" loading={busy === "run"} onClick={start}><Play />运行</Button><Button color="warning" variant="soft" size="sm" disabled={!snapshot.publisher || busy !== null} onClick={() => void execute("pause", () => pauseCanvas(projectId, canvasId))}><Pause />暂停</Button><Button color="danger" variant="soft" size="sm" disabled={!snapshot.publisher || busy !== null} onClick={() => void execute("stop", () => stopCanvas(projectId, canvasId))}><Stop />停止</Button><Button color="secondary" variant="ghost" size="sm" disabled={busy !== null} onClick={() => void execute("refresh", () => refreshCanvas(projectId, canvasId))}><ArrowRotateCcw />刷新</Button></div></header>
    {error ? <p className="error-message" role="alert">{error}</p> : null}
    <SegmentedControl className="canvas-tabs" value={tab} onChange={(value) => onTab(value as CanvasTab)} size="sm" aria-label="Canvas 工作台页面">{tabs.map((item) => <SegmentedControl.Option key={item.id} value={item.id}>{item.label}</SegmentedControl.Option>)}</SegmentedControl>
    <div className={`canvas-tab-content tab-${tab}`}>
      {tab === "overview" ? <CanvasOverview snapshot={snapshot} placed={placed} dirty={dirty} onNavigate={onTab} /> : null}
      {tab === "workflow" ? <WorkflowCanvas key={`${projectId}:${canvasId}`} projectId={projectId} canvasId={canvasId} uiState={uiState} onUiState={onUiState} publisherRunning={lifecycle === "running"} onSelection={onWorkflowSelection} onGraphChange={() => void load()} /> : null}
      {tab === "routes" ? <RoutesView assets={placed} /> : null}
      {tab === "publisher" ? <PublisherView snapshot={snapshot} onCreate={() => { setProvider(ready[0]?.providerInstanceId ?? ""); setDialog(true); }} /> : null}
      {tab === "docs" ? <DocsView templates={templates} language={language} onLanguage={setLanguage} /> : null}
      {tab === "runs" ? <RunsView records={records} /> : null}
    </div>
    {dialog ? <div className="wallet-dialog" role="dialog" aria-modal="true" aria-label="配置 Canvas Publisher"><div><Button className="dialog-close" color="secondary" variant="ghost" uniform aria-label="关闭" onClick={() => setDialog(false)}>×</Button><p className="eyebrow">First Run</p><h2>发布并运行 {snapshot.canvas.name}</h2><p>端点固定监听本机回环地址，Token 只写入 Windows Credential Manager。</p>{ready.length ? <><label className="field-label">Provider<Select value={provider} onChange={(option) => setProvider(option.value)} options={ready.map((asset) => ({ value: asset.providerInstanceId!, label: asset.name }))} /></label><label className="field-label">Publisher ID<Input value={publisherId} onChange={(event) => setPublisherId(event.target.value)} /></label><label className="field-label">公开模型名<Input value={model} onChange={(event) => setModel(event.target.value)} /></label><label className="field-label">端口<Input type="number" min="1024" max="65535" value={port} onChange={(event) => setPort(event.target.value)} /></label><label className="field-label">Publisher Token<Input type="password" value={token} onChange={(event) => setToken(event.target.value)} autoComplete="new-password" /></label><Button color="primary" block loading={busy === "publish"} disabled={!provider || !token.trim()} onClick={() => void publishAndRun()}>发布并运行</Button></> : <div className="next-step"><strong>还没有可运行的 Provider</strong><p>先在“API 与路由”中放入一个已配置密钥的 API 资产。</p></div>}</div></div> : null}
  </section>;
}

function CanvasOverview({ snapshot, placed, dirty, onNavigate }: { snapshot: CanvasSnapshot; placed: WalletCard[]; dirty: boolean; onNavigate: (tab: CanvasTab) => void }) { return <div className="canvas-overview-grid"><article><Branch /><span>编排</span><strong>{snapshot.canvas.graph.nodes.length} 个节点</strong><small>{dirty ? "有待应用修改" : "与运行版本一致"}</small><Button color="secondary" variant="ghost" size="sm" onClick={() => onNavigate("workflow")}>打开编排</Button></article><article><Plugin /><span>Publisher</span><strong>{snapshot.publisher?.baseUrl ?? "尚未发布"}</strong><small>{snapshot.publisher?.publicModels.join("、") || "首次运行时配置"}</small><Button color="secondary" variant="ghost" size="sm" onClick={() => onNavigate("publisher")}>查看出口</Button></article><article><Code /><span>API 与路由</span><strong>{placed.length} 个 Provider</strong><small>{snapshot.missingSecretCount ? `${snapshot.missingSecretCount} 个 Secret 缺失` : "凭据状态正常"}</small><Button color="secondary" variant="ghost" size="sm" onClick={() => onNavigate("routes")}>查看路由</Button></article></div>; }
function RoutesView({ assets }: { assets: WalletCard[] }) { return <div className="canvas-list-page"><div className="section-heading"><div><p className="eyebrow">Canvas Scope</p><h2>API 与路由</h2></div><Badge color="info" variant="soft">{assets.length} 个上游</Badge></div>{assets.length ? assets.map((asset, index) => <article className="route-asset" key={asset.id}><span>{index === 0 ? "主" : `备 ${index}`}</span><div><strong>{asset.name}</strong><small>{asset.configured ? asset.enabled ? "已配置并启用" : "已停用" : "缺少 Key"}</small></div></article>) : <p className="empty-state">当前 Canvas 尚未放置 API 资产。请在编排页添加 Provider Group。</p>}</div>; }
function PublisherView({ snapshot, onCreate }: { snapshot: CanvasSnapshot; onCreate: () => void }) { return <div className="canvas-list-page"><div className="section-heading"><div><p className="eyebrow">Canvas Output</p><h2>Publisher</h2></div>{snapshot.publisher ? <Badge color="success" variant="soft">已绑定</Badge> : <Button color="primary" onClick={onCreate}>配置 Publisher</Button>}</div>{snapshot.publisher ? <article className="canvas-publisher-card"><Plugin /><div><strong>{snapshot.publisher.id}</strong><p>{snapshot.publisher.baseUrl}</p><small>{snapshot.publisher.publicModels.join("、")}</small></div></article> : <p className="empty-state">此 Canvas 尚未发布。点击“运行”也会打开同一个发布向导。</p>}</div>; }
function DocsView({ templates, language, onLanguage }: { templates: CodeTemplate[]; language: string; onLanguage: (language: string) => void }) { const current = templates.find((template) => template.language === language) ?? templates[0]; return <div className="canvas-list-page"><div className="section-heading"><div><p className="eyebrow">Live Documentation</p><h2>活文档</h2></div>{templates.length ? <Select value={current?.language ?? ""} onChange={(option) => onLanguage(option.value)} options={templates.map((template) => ({ value: template.language, label: template.title }))} /> : null}</div>{current ? <Textarea className="code-template" value={current.code} readOnly rows={20} /> : <p className="empty-state">发布当前 Canvas 后自动生成八种语言的调用模板。</p>}</div>; }
function RunsView({ records }: { records: ExecutionTrace[] }) { return <div className="canvas-list-page"><div className="section-heading"><div><p className="eyebrow">Canvas Audit</p><h2>运行记录</h2></div><Badge color="secondary" variant="soft">{records.length} 条</Badge></div>{records.length ? records.map((record) => <article className="canvas-run-card" key={record.correlation_id}><History /><div><strong>{record.public_model}</strong><small>{record.total_latency_ms} ms · {record.streaming ? "流式" : "非流式"} · {record.result}</small></div></article>) : <p className="empty-state">此 Canvas 尚无调用记录。</p>}</div>; }
function isDesktopSnapshot(value: unknown): value is DesktopSnapshot { return Boolean(value && typeof value === "object" && "initialized" in value); }
