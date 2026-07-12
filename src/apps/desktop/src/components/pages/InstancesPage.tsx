"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { ArrowRotateCcw, CheckCircle, Code, Copy, Grid, History, Pause, Play, Settings as SettingsIcon } from "@openai/apps-sdk-ui/components/Icon";
import { useCallback, useEffect, useMemo, useState } from "react";
import { getInstanceRackSnapshot, startAllInstances, startManagedInstance, stopAllInstances, stopManagedInstance, testManagedInstance, type InstanceBatchResult, type InstanceRackSnapshot, type ManagedInstance, type ManagedInstanceKind, type ManagedInstanceStatus } from "../../lib/desktop";
import { formatTimestamp, readError } from "../shared";

type Props = {
  onNavigate: (page: "wallet" | "direct" | "runs") => void;
  onOpenCanvas: (projectId: string, canvasId: string, tab?: "overview" | "workflow" | "publisher" | "docs") => void;
  onChanged: () => void;
};

const statusMeta: Record<ManagedInstanceStatus, { label: string; color: "success" | "secondary" | "warning" | "danger" | "info" }> = {
  running: { label: "运行中", color: "success" }, stopped: { label: "已停止", color: "secondary" },
  starting: { label: "启动中", color: "info" }, stopping: { label: "停止中", color: "warning" },
  blocked: { label: "配置阻塞", color: "warning" }, failed: { label: "运行异常", color: "danger" }, unpublished: { label: "尚未发布", color: "secondary" },
};

export function InstancesPage({ onNavigate, onOpenCanvas, onChanged }: Props) {
  const [snapshot, setSnapshot] = useState<InstanceRackSnapshot | null>(null);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | ManagedInstanceKind>("all");
  const [status, setStatus] = useState<"all" | ManagedInstanceStatus>("all");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [batch, setBatch] = useState<InstanceBatchResult | null>(null);
  const [testMessage, setTestMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try { setSnapshot(await getInstanceRackSnapshot()); setError(null); }
    catch (reason) { setError(readError(reason)); }
  }, []);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => listen("desktop:instances-changed", () => void load())).then((next) => { dispose = next; }).catch(() => undefined);
    const timer = window.setInterval(() => void load(), 15_000);
    return () => { dispose?.(); window.clearInterval(timer); };
  }, [load]);

  const visible = useMemo(() => (snapshot?.instances ?? []).filter((item) => {
    const text = `${item.name} ${item.ownership} ${item.assetNames.join(" ")} ${item.publicModels.join(" ")}`.toLowerCase();
    return (!query.trim() || text.includes(query.trim().toLowerCase())) && (kind === "all" || item.kind === kind) && (status === "all" || item.status === status);
  }), [snapshot, query, kind, status]);

  const runBatch = async (action: "start-all" | "stop-all") => {
    const count = snapshot?.instances.length ?? 0;
    if (!count) return;
    const wording = action === "start-all" ? "启动所有配置就绪的实例" : "停止全部业务入口";
    if (!window.confirm(`${wording}？配置和钱包资产不会被删除。`)) return;
    setBusy(action); setError(null); setBatch(null);
    try {
      const result = await (action === "start-all" ? startAllInstances() : stopAllInstances());
      setBatch(result); setSnapshot(result.snapshot); onChanged();
    } catch (reason) { setError(readError(reason)); }
    finally { setBusy(null); }
  };
  const toggle = async (item: ManagedInstance) => {
    setBusy(item.id); setError(null);
    try {
      const result = await (item.status === "running" ? stopManagedInstance(item.id) : startManagedInstance(item.id));
      setBatch(result); setSnapshot(result.snapshot); onChanged();
    } catch (reason) { setError(readError(reason)); }
    finally { setBusy(null); }
  };
  const test = async (item: ManagedInstance) => {
    setBusy(`test:${item.id}`); setTestMessage(null); setError(null);
    try { const result = await testManagedInstance(item.id); setTestMessage(`${item.name}：${result.safeSummary} ${result.latencyMs} ms，${result.modelCount} 个模型。`); }
    catch (reason) { setError(readError(reason)); }
    finally { setBusy(null); }
  };
  const openDetails = (item: ManagedInstance, target = item.repairTarget) => {
    if (item.kind === "direct_endpoint") { if (item.directEndpointId) sessionStorage.setItem("apiarray.direct.endpoint", item.directEndpointId); onNavigate(target === "wallet" ? "wallet" : "direct"); return; }
    if (item.projectId && item.canvasId) onOpenCanvas(item.projectId, item.canvasId, target === "canvas:publisher" ? "publisher" : target === "canvas:workflow" ? "workflow" : "overview");
  };
  const openDocs = (item: ManagedInstance) => {
    if (item.kind === "direct_endpoint") { if (item.directEndpointId) sessionStorage.setItem("apiarray.direct.docs", item.directEndpointId); onNavigate("direct"); }
    else if (item.projectId && item.canvasId) onOpenCanvas(item.projectId, item.canvasId, "docs");
  };

  const metrics = [
    ["运行中", snapshot?.runningCount ?? 0, "success"], ["已停止", snapshot?.stoppedCount ?? 0, "secondary"],
    ["阻塞 / 未发布", snapshot?.blockedCount ?? 0, "warning"], ["运行异常", snapshot?.failedCount ?? 0, "danger"],
  ] as const;
  return <section className="instance-rack">
    <header className="instance-rack-header"><div><p className="eyebrow">INSTANCE RACK</p><h1>全部本地 API，一处运行</h1><p>统一管理审计直出与 Canvas 编组实例。配置仍保留在各自所属页面。</p></div><div className="rack-bulk-actions"><Button color="success" loading={busy === "start-all"} disabled={Boolean(busy)} onClick={() => void runBatch("start-all")}><Play />全部启动</Button><Button color="danger" variant="soft" loading={busy === "stop-all"} disabled={Boolean(busy)} onClick={() => void runBatch("stop-all")}><Pause />全部停止</Button><Button color="secondary" variant="ghost" uniform aria-label="刷新实例状态" disabled={Boolean(busy)} onClick={() => void load()}><ArrowRotateCcw /></Button></div></header>
    <div className="rack-metrics">{metrics.map(([label, value, color]) => <article key={label}><Badge color={color} variant="soft">{label}</Badge><strong>{value}</strong></article>)}<article className="gateway-metric"><Badge color={snapshot?.gateway.running ? "success" : "danger"} variant="soft">统一网关</Badge><strong>{snapshot?.gateway.running ? `${snapshot.gateway.entryCount} 个入口` : "未运行"}</strong><small>{snapshot?.gateway.baseUrl}</small></article></div>
    {error ? <p className="error-message" role="alert">{error}</p> : null}{testMessage ? <p className="success-message" role="status">{testMessage}</p> : null}
    <div className="rack-toolbar"><Input aria-label="搜索实例" placeholder="搜索名称、Project、钱包资产或模型" value={query} onChange={(event) => setQuery(event.target.value)} /><Select aria-label="实例类型" value={kind} options={[{ value: "all", label: "全部类型" }, { value: "direct_endpoint", label: "审计直出" }, { value: "canvas", label: "Canvas 编组" }]} onChange={(option) => setKind(option.value as typeof kind)} /><Select aria-label="运行状态" value={status} options={[{ value: "all", label: "全部状态" }, ...Object.entries(statusMeta).map(([value, item]) => ({ value, label: item.label }))]} onChange={(option) => setStatus(option.value as typeof status)} /><Badge color="secondary" variant="soft">{visible.length} / {snapshot?.instances.length ?? 0}</Badge></div>
    {!snapshot ? <div className="rack-empty"><Grid /><strong>正在读取实例状态</strong></div> : !snapshot.instances.length ? <div className="rack-empty"><Grid /><strong>机架还是空的</strong><p>先创建审计直出端点，或在编组模式发布一个 Canvas。</p><div><Button color="primary" onClick={() => onNavigate("direct")}>创建审计直出</Button></div></div> : !visible.length ? <div className="rack-empty"><Grid /><strong>没有符合筛选条件的实例</strong><Button color="secondary" variant="ghost" onClick={() => { setQuery(""); setKind("all"); setStatus("all"); }}>清除筛选</Button></div> : <div className="rack-table" role="table" aria-label="实例机架">
      <div className="rack-row rack-table-head" role="row"><span>实例</span><span>入口与模型</span><span>最近活动</span><span>状态</span><span>操作</span></div>
      {visible.map((item) => { const meta = statusMeta[item.status]; return <article className="rack-row" role="row" key={item.id}>
        <div className="rack-identity"><span className={`rack-kind ${item.kind}`}><Grid /></span><div><strong>{item.name}</strong><small>{item.kind === "canvas" ? `Canvas · ${item.ownership}` : `直出 · ${item.assetNames.join("、") || "未关联资产"}`}</small></div></div>
        <div className="rack-endpoint"><code>{item.baseUrl ?? "尚未创建本地出口"}</code><small>{item.publicModels.join("、") || "尚无公开模型"}</small></div>
        <div className="rack-activity"><strong>{item.requestCount} 次调用</strong><small>{item.lastCallAtMs ? `${formatTimestamp(item.lastCallAtMs)} · ${item.lastLatencyMs ?? 0} ms` : "暂无调用记录"}</small></div>
        <div className="rack-state"><Badge color={meta.color} variant="soft">{meta.label}</Badge>{item.hasUnappliedChanges ? <small>有未应用草稿</small> : null}{item.blockingReasons[0] ? <button onClick={() => openDetails(item)}>{item.blockingReasons[0]}</button> : item.lastError ? <small>{item.lastError}</small> : null}</div>
        <div className="rack-actions"><Button color={item.status === "running" ? "warning" : "success"} variant="soft" size="sm" loading={busy === item.id} disabled={Boolean(busy) || item.status === "unpublished"} onClick={() => void toggle(item)}>{item.status === "running" ? <Pause /> : <Play />}{item.status === "running" ? "停止" : "启动"}</Button><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`自检 ${item.name}`} disabled={item.status !== "running" || Boolean(busy)} loading={busy === `test:${item.id}`} onClick={() => void test(item)}><CheckCircle /></Button><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`复制 ${item.name} URL`} disabled={!item.baseUrl} onClick={() => item.baseUrl && void navigator.clipboard.writeText(item.baseUrl)}><Copy /></Button><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`打开 ${item.name} 活文档`} disabled={!item.baseUrl} onClick={() => openDocs(item)}><Code /></Button><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`查看 ${item.name} 审计`} onClick={() => { if (item.auditPublisherId) sessionStorage.setItem("apiarray.audit.publisher", item.auditPublisherId); onNavigate("runs"); }}><History /></Button><Button color="secondary" variant="ghost" size="sm" uniform aria-label={`编辑 ${item.name}`} onClick={() => openDetails(item)}><SettingsIcon /></Button></div>
      </article>; })}
    </div>}
    {batch ? <aside className="rack-result" aria-label="批量操作结果"><div className="section-heading"><div><p className="eyebrow">OPERATION RESULT</p><h2>运行计划已处理</h2><p>{batch.succeeded} 成功 · {batch.failed} 失败 · {batch.skipped} 无需变更</p></div><Button color="secondary" variant="ghost" onClick={() => setBatch(null)}>关闭</Button></div><div className="rack-result-list">{batch.results.map((result) => <article key={result.instanceId}><Badge color={result.success ? "success" : "danger"} variant="soft">{result.success ? (result.skipped ? "跳过" : "完成") : "失败"}</Badge><code>{result.instanceId}</code><span>{result.message}</span></article>)}</div></aside> : null}
  </section>;
}
