import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { CheckCircle, CircleQuestion, Code, Play, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { getProviderInstances, testPublisherConnection, type DesktopSnapshot, type ProviderInstanceItem, type WorkspaceIntent } from "../../lib/desktop";
import type { AppPage } from "../navigation";
import { readError } from "../shared";
import { buildSetupProgress, intentCopy } from "../setup/setupModel";

export function GuidedHome({ control, intent, onNavigate }: { control: NonNullable<DesktopSnapshot["control"]>; intent: WorkspaceIntent; onNavigate: (page: AppPage) => void }) {
  const [providers, setProviders] = useState<ProviderInstanceItem[]>([]);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  useEffect(() => { void getProviderInstances().then(setProviders).catch(() => undefined); }, [control.providerCount]);
  const progress = buildSetupProgress(control, providers);
  const copy = intentCopy(intent);
  const publishers = control.supervisor.publishers ?? [];
  const firstPublisher = publishers[0];
  const destination = { provider: "assets", inspection: "assets", publisher: "publishers", call: "templates" }[progress.current] as AppPage;
  const test = async () => { if (!firstPublisher) return; setTesting(true); try { const result = await testPublisherConnection(firstPublisher.id); setTestResult(`${result.safeSummary} ${result.latencyMs} ms，${result.modelCount} 个模型。`); } catch (reason) { setTestResult(readError(reason)); } finally { setTesting(false); } };

  if (control.publisherCount > 0) return <div className="home-dashboard"><section className="hero-panel compact"><div><Badge color="success" variant="soft">本地控制平面已就绪</Badge><h1>{control.workspaceName}</h1><p>你的 API 已进入可发布、可审计的日常工作状态。</p></div><Button color="primary" onClick={() => onNavigate("publishers")}><Plugin />管理端点</Button></section><div className="dashboard-grid"><article className="dashboard-card"><span>Provider</span><strong>{control.providerCount}</strong><small>{control.missingSecretCount ? `${control.missingSecretCount} 项 Secret 待绑定` : "Secret 状态正常"}</small></article><article className="dashboard-card"><span>Publisher</span><strong>{control.publisherCount}</strong><small>{publishers.filter((item) => item.status === "running").length} 个正在运行</small></article><article className="dashboard-card"><span>通知</span><strong>{control.notifications.length}</strong><small>{control.notifications.at(-1)?.message ?? "没有新的运行事件"}</small></article></div><section className="endpoint-action"><div><p className="eyebrow">Quick verification</p><h2>{firstPublisher?.id ?? "本地端点"}</h2><p>{testResult ?? "执行无计费的本地 /v1/models 检查，确认桌面端与 Publisher 鉴权链路。"}</p></div><div className="actions"><Button color="secondary" variant="soft" loading={testing} disabled={!firstPublisher} onClick={() => void test()}><Play />本地自检</Button><Button color="primary" variant="soft" onClick={() => onNavigate("templates")}><Code />调用模板</Button></div></section></div>;

  return <div className="guided-home"><section className="hero-panel"><div><p className="eyebrow">开始使用 API ARRAY</p><h1>{copy.title}</h1><p>{copy.detail}</p></div><Button color="primary" onClick={() => onNavigate(destination)}>{progress.steps.find((step) => step.id === progress.current)?.action}</Button></section><div className="setup-progress"><div className="progress-heading"><span>首次发布进度</span><strong>{progress.completed}/4</strong></div><div className="progress-track"><span style={{ width: `${progress.completed * 25}%` }} /></div></div><div className="setup-steps">{progress.steps.map((step, index) => <button key={step.id} className={`setup-step ${step.complete ? "complete" : ""} ${step.id === progress.current ? "current" : ""}`} onClick={() => onNavigate(({ provider: "assets", inspection: "assets", publisher: "publishers", call: "templates" } as const)[step.id])}><span className="step-index">{step.complete ? <CheckCircle /> : index + 1}</span><span><strong>{step.title}</strong><small>{step.description}</small></span>{step.id === progress.current ? <Badge color="info" variant="soft">当前</Badge> : null}</button>)}</div><div className="guided-note"><CircleQuestion /><span>无需理解节点系统。完成第一次调用后，再进入专业画布检查或调整路由。</span></div></div>;
}
