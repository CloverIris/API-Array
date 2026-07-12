"use client";

import { Alert } from "@openai/apps-sdk-ui/components/Alert";
import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { EmptyMessage } from "@openai/apps-sdk-ui/components/EmptyMessage";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Menu } from "@openai/apps-sdk-ui/components/Menu";
import { Popover } from "@openai/apps-sdk-ui/components/Popover";
import { SegmentedControl } from "@openai/apps-sdk-ui/components/SegmentedControl";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import { ApiKeys, ArrowRotateCcw, Branch, CheckCircle, Code, Copy, Document, DotsHorizontal, Grid, History, Home, Pause, Play, Plugin, Search, Settings as SettingsIcon, Warning } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import { startAllInstances, startManagedInstance, stopAllInstances, stopManagedInstance, testManagedInstance, type ControlCenterSnapshot, type InstanceBatchResult, type ManagedInstance, type ManagedInstanceKind, type ManagedInstanceStatus, type ProjectTree, type WalletCard } from "../../lib/desktop";
import { copyText, formatTimestamp, readError } from "../shared";
import type { AppPage } from "../navigation";

type Props = {
  workspaceName: string;
  controlCenter: ControlCenterSnapshot | null;
  wallet: WalletCard[];
  projectTree: ProjectTree;
  onControlCenter: (snapshot: ControlCenterSnapshot) => void;
  onRefresh: () => void;
  onNavigate: (page: AppPage) => void;
  onOpenCanvas: (projectId: string, canvasId: string, tab?: "overview" | "workflow" | "publisher" | "docs") => void;
};

const statusMeta: Record<ManagedInstanceStatus, { label: string; color: "success" | "secondary" | "warning" | "danger" | "info" }> = {
  running: { label: "运行中", color: "success" }, stopped: { label: "已停止", color: "secondary" }, starting: { label: "启动中", color: "info" }, stopping: { label: "停止中", color: "warning" }, blocked: { label: "需要处理", color: "warning" }, failed: { label: "运行异常", color: "danger" }, unpublished: { label: "尚未发布", color: "secondary" },
};

export function HomeDashboard({ workspaceName, controlCenter, wallet, projectTree, onControlCenter, onRefresh, onNavigate, onOpenCanvas }: Props) {
  const [view, setView] = useState<"overview" | "instances">("overview");
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | ManagedInstanceKind>("all");
  const [status, setStatus] = useState<"all" | ManagedInstanceStatus>("all");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [batch, setBatch] = useState<InstanceBatchResult | null>(null);
  const [stopConfirmOpen, setStopConfirmOpen] = useState(false);

  const instances = controlCenter?.instances ?? [];
  const canvases = Object.values(projectTree.projects).flatMap((project) => Object.values(project.canvases));
  const visible = useMemo(() => instances.filter((item) => {
    const haystack = `${item.name} ${item.ownership} ${item.assetNames.join(" ")} ${item.publicModels.join(" ")}`.toLowerCase();
    return (!query.trim() || haystack.includes(query.trim().toLowerCase())) && (kind === "all" || item.kind === kind) && (status === "all" || item.status === status);
  }), [instances, query, kind, status]);
  const attention = instances.filter((item) => ["blocked", "failed", "unpublished"].includes(item.status) || item.hasUnappliedChanges);
  const recent = [...instances].sort((a, b) => (b.lastCallAtMs ?? 0) - (a.lastCallAtMs ?? 0) || Number(b.status === "running") - Number(a.status === "running")).slice(0, 6);

  const applyResult = (result: InstanceBatchResult) => { setBatch(result); onControlCenter(result.snapshot); onRefresh(); };
  const startAvailable = async () => { setBusy("start-all"); setError(null); try { applyResult(await startAllInstances()); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };
  const stopAll = async () => { setBusy("stop-all"); setError(null); try { applyResult(await stopAllInstances()); setStopConfirmOpen(false); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };
  const toggle = async (item: ManagedInstance) => { setBusy(item.id); setError(null); try { applyResult(await (item.status === "running" ? stopManagedInstance(item.id) : startManagedInstance(item.id))); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };
  const test = async (item: ManagedInstance) => { setBusy(`test:${item.id}`); setError(null); try { const result = await testManagedInstance(item.id); setNotice(`${item.name}：${result.safeSummary}，${result.latencyMs} ms，${result.modelCount} 个模型。`); } catch (reason) { setError(readError(reason)); } finally { setBusy(null); } };
  const openDetails = (item: ManagedInstance, target = item.repairTarget) => { if (item.kind === "direct_endpoint") { if (item.directEndpointId) sessionStorage.setItem("apiarray.direct.endpoint", item.directEndpointId); onNavigate(target === "wallet" ? "wallet" : "direct"); return; } if (item.projectId && item.canvasId) onOpenCanvas(item.projectId, item.canvasId, target === "canvas:publisher" ? "publisher" : target === "canvas:workflow" ? "workflow" : "overview"); };
  const openDocs = (item: ManagedInstance) => { if (item.kind === "direct_endpoint") { if (item.directEndpointId) sessionStorage.setItem("apiarray.direct.docs", item.directEndpointId); onNavigate("direct"); } else if (item.projectId && item.canvasId) onOpenCanvas(item.projectId, item.canvasId, "docs"); };

  if (!controlCenter) return <EmptyMessage fill="absolute"><EmptyMessage.Icon><Home /></EmptyMessage.Icon><EmptyMessage.Title>正在打开主控台</EmptyMessage.Title><EmptyMessage.Description>正在读取本地网关与实例状态。</EmptyMessage.Description></EmptyMessage>;
  const quickCards = [
    { id: "wallet", title: "API 钱包", description: `${wallet.length} 个上游资产`, icon: ApiKeys, action: () => onNavigate("wallet") },
    { id: "direct", title: "审计直出", description: `${instances.filter((item) => item.kind === "direct_endpoint").length} 个本地端点`, icon: Plugin, action: () => onNavigate("direct") },
    { id: "compositions", title: "编组模式", description: `${canvases.length} 个 Canvas`, icon: Branch, action: () => onNavigate("compositions") },
    { id: "gateway", title: "统一网关", description: controlCenter.gateway.running ? `${controlCenter.gateway.entryCount} 个入口已挂载` : "需要检查", icon: Grid, action: () => setView("instances") },
    { id: "templates", title: "文档", description: "接口说明与八种语言", icon: Document, action: () => onNavigate("templates") },
  ];

  return <section className="mx-auto grid w-full max-w-[1320px] gap-5 pb-12">
    <div className="overflow-hidden rounded-3xl border border-default bg-surface shadow-sm">
      <div className="control-center-hero relative overflow-hidden px-6 py-7 md:px-8 md:py-9">
        <div className="relative z-10"><p className="mb-2 text-xs font-semibold tracking-[.12em] text-info uppercase">API ARRAY CONTROL CENTER</p><h1 className="text-3xl font-semibold tracking-tight">主控台</h1><p className="mt-2 max-w-2xl text-sm leading-6 text-secondary">{workspaceName} 的本地 API、审计入口和编组服务都在这里。</p><div className="mt-5 flex flex-wrap items-center gap-2"><Button color="primary" loading={busy === "start-all"} disabled={Boolean(busy)} onClick={() => void startAvailable()}><Play />启动可用实例</Button><Popover open={stopConfirmOpen} onOpenChange={setStopConfirmOpen}><Popover.Trigger><Button color="secondary" variant="soft" disabled={Boolean(busy)}><Pause />停止全部</Button></Popover.Trigger><Popover.Content align="start" side="bottom" minWidth={340}><Alert color="warning" title="停止全部业务入口？" description="统一网关仍会运行，钱包、配置、Secret 和审计记录不会被删除。" actions={<div className="flex gap-2"><Button color="danger" size="sm" loading={busy === "stop-all"} onClick={() => void stopAll()}>确认停止</Button><Button color="secondary" variant="ghost" size="sm" onClick={() => setStopConfirmOpen(false)}>取消</Button></div>} actionsPlacement="bottom" /></Popover.Content></Popover><Badge color={controlCenter.gateway.running ? "success" : "danger"} variant="soft">{controlCenter.gateway.running ? `网关在线 · ${controlCenter.runningCount} 运行中` : "网关不可用"}</Badge></div></div>
        <div className="control-center-orbit orbit-large" /><div className="control-center-orbit orbit-small" />
      </div>
      <div className="grid grid-cols-2 gap-px bg-surface-secondary md:grid-cols-5">{quickCards.map((card) => { const Icon = card.icon; return <button type="button" key={card.id} className="control-center-shortcut" onClick={card.action}><div className="shortcut-icon"><Icon className="size-5" /></div><span><strong>{card.title}</strong><small>{card.description}</small></span></button>; })}</div>
    </div>

    <div className="dashboard-toolbar"><div><strong>运行概览</strong><small>查看状态，或集中管理全部本地实例。</small></div><div className="flex items-center gap-2"><SegmentedControl value={view} onChange={setView} aria-label="主控台视图" size="sm"><SegmentedControl.Option value="overview">概览</SegmentedControl.Option><SegmentedControl.Option value="instances">全部实例</SegmentedControl.Option></SegmentedControl><Tooltip content="刷新主控台"><Button color="secondary" variant="ghost" uniform aria-label="刷新主控台" onClick={onRefresh}><ArrowRotateCcw /></Button></Tooltip></div></div>
    {error ? <Alert color="danger" title="操作未完成" description={error} actions={<Button color="secondary" variant="soft" size="sm" onClick={() => void copyText(error)}>复制错误摘要</Button>} /> : null}
    {notice ? <Alert color="success" title="本地自检通过" description={notice} actions={<Button color="secondary" variant="ghost" size="sm" onClick={() => setNotice(null)}>关闭</Button>} /> : null}
    {batch ? <Alert color={batch.failed ? "warning" : "success"} title="运行计划已处理" description={`${batch.succeeded} 项完成，${batch.failed} 项失败，${batch.skipped} 项无需变更。`} actions={<Button color="secondary" variant="ghost" size="sm" onClick={() => setBatch(null)}>关闭</Button>} /> : null}

    {view === "overview" ? <div className="grid gap-5">
      <section><div className="mb-3 flex items-center justify-between"><div><h2 className="text-lg font-semibold">需要处理</h2><p className="mt-1 text-sm text-secondary">优先修复会阻止本地调用的问题。</p></div><Badge color={attention.length ? "warning" : "success"} variant="soft">{attention.length ? `${attention.length} 项` : "状态正常"}</Badge></div>{attention.length ? <div className="grid gap-2 md:grid-cols-2">{attention.slice(0, 6).map((item) => <Alert key={item.id} color={item.status === "failed" ? "danger" : "warning"} variant="soft" title={item.name} description={item.blockingReasons[0] ?? (item.hasUnappliedChanges ? "存在尚未应用的草稿。" : statusMeta[item.status].label)} actions={<Button color="secondary" variant="ghost" size="sm" onClick={() => openDetails(item)}>前往处理</Button>} />)}</div> : <div className="rounded-2xl border border-default bg-surface"><EmptyMessage><EmptyMessage.Icon color="secondary"><CheckCircle /></EmptyMessage.Icon><EmptyMessage.Title>没有待处理问题</EmptyMessage.Title><EmptyMessage.Description>已发布实例的配置和本地入口状态正常。</EmptyMessage.Description></EmptyMessage></div>}</section>
      <section><div className="mb-3 flex items-center justify-between"><div><h2 className="text-lg font-semibold">最近活动</h2><p className="mt-1 text-sm text-secondary">最近调用或当前正在运行的实例。</p></div><Button color="secondary" variant="ghost" size="sm" onClick={() => setView("instances")}>查看全部</Button></div>{recent.length ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{recent.map((item) => <InstanceCard key={item.id} item={item} busy={busy} onToggle={toggle} onTest={test} onDocs={openDocs} onAudit={() => onNavigate("runs")} onDetails={openDetails} />)}</div> : <div className="rounded-2xl border border-default bg-surface"><EmptyMessage><EmptyMessage.Icon><History /></EmptyMessage.Icon><EmptyMessage.Title>暂无实例活动</EmptyMessage.Title><EmptyMessage.Description>创建审计直出或发布 Canvas 后会显示在这里。</EmptyMessage.Description><EmptyMessage.ActionRow><Button color="primary" onClick={() => onNavigate("direct")}>创建审计直出</Button></EmptyMessage.ActionRow></EmptyMessage></div>}</section>
    </div> : <section className="grid gap-4"><div className="grid gap-2 md:grid-cols-[minmax(240px,1fr)_180px_180px_auto]"><Input aria-label="筛选实例" placeholder="筛选名称、Project、资产或模型" value={query} onChange={(event) => setQuery(event.target.value)} /><Select aria-label="实例类型" value={kind} options={[{ value: "all", label: "全部类型" }, { value: "direct_endpoint", label: "审计直出" }, { value: "canvas", label: "Canvas 编组" }]} onChange={(option) => setKind(option.value as typeof kind)} /><Select aria-label="实例状态" value={status} options={[{ value: "all", label: "全部状态" }, ...Object.entries(statusMeta).map(([value, item]) => ({ value, label: item.label }))]} onChange={(option) => setStatus(option.value as typeof status)} /><Badge color="secondary" variant="soft">{visible.length} / {instances.length}</Badge></div>{visible.length ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{visible.map((item) => <InstanceCard key={item.id} item={item} busy={busy} onToggle={toggle} onTest={test} onDocs={openDocs} onAudit={() => onNavigate("runs")} onDetails={openDetails} />)}</div> : <div className="rounded-2xl border border-default bg-surface"><EmptyMessage><EmptyMessage.Icon><Search /></EmptyMessage.Icon><EmptyMessage.Title>没有符合条件的实例</EmptyMessage.Title><EmptyMessage.Description>调整搜索词或筛选条件后重试。</EmptyMessage.Description><EmptyMessage.ActionRow><Button color="secondary" variant="ghost" onClick={() => { setQuery(""); setKind("all"); setStatus("all"); }}>清除筛选</Button></EmptyMessage.ActionRow></EmptyMessage></div>}</section>}
  </section>;
}

function InstanceCard({ item, busy, onToggle, onTest, onDocs, onAudit, onDetails }: { item: ManagedInstance; busy: string | null; onToggle: (item: ManagedInstance) => Promise<void>; onTest: (item: ManagedInstance) => Promise<void>; onDocs: (item: ManagedInstance) => void; onAudit: () => void; onDetails: (item: ManagedInstance, target?: string | null) => void }) {
  const meta = statusMeta[item.status];
  return <article className="flex min-h-52 flex-col rounded-2xl border border-default bg-surface p-4 shadow-sm"><div className="flex items-start justify-between gap-3"><div className="flex min-w-0 items-center gap-3"><span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-secondary-soft text-secondary">{item.kind === "canvas" ? <Branch className="size-5" /> : <Plugin className="size-5" />}</span><div className="min-w-0"><h3 className="truncate font-semibold">{item.name}</h3><p className="truncate text-xs text-secondary">{item.kind === "canvas" ? item.ownership : item.assetNames.join("、") || "未关联钱包资产"}</p></div></div><Menu><Menu.Trigger><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`${item.name} 更多操作`}><DotsHorizontal /></Button></Menu.Trigger><Menu.Content align="end" minWidth={190}><Menu.Item disabled={item.status !== "running"} onSelect={() => void onTest(item)}><CheckCircle />本地自检</Menu.Item><Menu.Item disabled={!item.baseUrl} onSelect={() => item.baseUrl && void navigator.clipboard.writeText(item.baseUrl)}><Copy />复制 URL</Menu.Item><Menu.Item disabled={!item.baseUrl} onSelect={() => onDocs(item)}><Code />打开文档</Menu.Item><Menu.Item onSelect={onAudit}><History />查看审计</Menu.Item><Menu.Separator /><Menu.Item onSelect={() => onDetails(item)}><SettingsIcon />编辑配置</Menu.Item></Menu.Content></Menu></div><div className="mt-4 flex items-center gap-2"><Badge color={meta.color} variant="soft">{meta.label}</Badge>{item.hasUnappliedChanges ? <Badge color="info" variant="soft">有新草稿</Badge> : null}</div><div className="mt-4 grid gap-1 text-xs text-secondary"><code className="truncate text-info">{item.baseUrl ?? "尚未创建本地出口"}</code><span>{item.publicModels.join("、") || "尚无公开模型"}</span><span>{item.requestCount} 次调用{item.lastCallAtMs ? ` · ${formatTimestamp(item.lastCallAtMs)}` : ""}</span></div>{item.blockingReasons[0] ? <button className="mt-3 flex items-start gap-2 rounded-lg bg-warning-soft p-2 text-left text-xs text-warning" onClick={() => onDetails(item)}><Warning className="mt-0.5 size-4 shrink-0" />{item.blockingReasons[0]}</button> : null}<Button className="mt-auto pt-4" color={item.status === "running" ? "warning" : "primary"} variant={item.status === "running" ? "soft" : "solid"} block loading={busy === item.id} disabled={Boolean(busy) || item.status === "unpublished"} onClick={() => void onToggle(item)}>{item.status === "running" ? <Pause /> : <Play />}{item.status === "running" ? "停止" : "启动"}</Button></article>;
}
