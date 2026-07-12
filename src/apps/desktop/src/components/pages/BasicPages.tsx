import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { SegmentedControl } from "@openai/apps-sdk-ui/components/SegmentedControl";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import { Textarea } from "@openai/apps-sdk-ui/components/Textarea";
import { ApiKeys, ArrowRotateCcw, Bell, CheckCircle, Code, FileDocument, History, Key, Plugin, Settings as SettingsIcon } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { exportWorkspace, getAuditRecords, getPublisherTemplates, importWorkspace, type CodeTemplate, type DesktopSnapshot, type ExecutionTrace, type PublisherSnapshot, type WorkspaceUiState } from "../../lib/desktop";
import { EmptyPage, Metric, formatTimestamp, readError } from "../shared";

export function OverviewPage({ control }: { control: NonNullable<DesktopSnapshot["control"]> }) {
  const publishers = control.supervisor.publishers ?? [];
  return <><p className="lead">本地 ControlPlane 的只读快照。未配置 Provider 前，不会产生任何上游请求。</p><div className="metric-grid"><Metric label="API 资产" value={`${control.providerCount}`} detail="已配置的 Provider 实例" icon={ApiKeys} /><Metric label="Publisher" value={`${publishers.length}`} detail="可由桌面端管理" icon={Plugin} /><Metric label="缺失 Secret" value={`${control.missingSecretCount}`} detail="密钥值永不发送到前端" icon={Key} /><Metric label="通知" value={`${control.notifications.length}`} detail="静默聚合的事件" icon={Bell} /></div><EmptyPage page="控制平面已就绪" hint="从 API 资产接入 Provider，完成体检后创建 Publisher，或进入工作流编排专业路由。" icon={CheckCircle} /></>;
}

export function NotificationsPage({ notifications }: { notifications: NonNullable<DesktopSnapshot["control"]>["notifications"] }) {
  if (!notifications.length) return <EmptyPage page="暂无通知" hint="重复错误会被聚合，避免在系统通知栏循环刷屏。" icon={Bell} />;
  return <div className="notification-list">{notifications.map((notice, index) => <article key={notice.id ?? index}><div className="notification-heading"><Bell className="size-4" /><Badge color="info" variant="soft">本地事件</Badge></div><strong>{notice.message}</strong><small>{formatTimestamp(notice.createdAt)}</small></article>)}</div>;
}

export function RunsPage() {
  const [records, setRecords] = useState<ExecutionTrace[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const refresh = async () => { setLoading(true); try { setRecords(await getAuditRecords()); setMessage(null); } catch (reason) { setMessage(readError(reason)); } finally { setLoading(false); } };
  useEffect(() => { void refresh(); }, []);
  if (loading) return <EmptyPage page="正在读取运行记录" hint="审计只保存元数据、延迟与路由结果，不保存请求或响应正文。" icon={History} />;
  return <section className="runs-view"><div className="section-heading"><div><p className="eyebrow">Local Audit</p><h2>运行记录</h2></div><Button color="secondary" variant="soft" size="sm" onClick={() => void refresh()}><ArrowRotateCcw />刷新</Button></div>{message ? <p className="error-message" role="alert">{message}</p> : null}{records.length ? <div className="run-list">{records.map((record) => <article className="run-card" key={record.correlation_id}><div className="run-heading"><div><strong>{record.publisher_id}</strong><small>{record.public_model} · {formatTimestamp(record.started_at_unix_ms)}</small></div><Badge color={record.result === "success" ? "success" : "danger"} variant="soft">{record.result}</Badge></div><div className="run-metrics"><span>{record.total_latency_ms} ms</span><span>{record.retry_count} 次重试</span><span>{record.failover_count} 次切换</span><span>{record.streaming ? "流式" : "非流式"}</span></div>{record.final_error ? <p>{record.final_error}</p> : null}</article>)}</div> : <EmptyPage page="暂无运行记录" hint="启动 Publisher 并接收本地请求后，安全审计记录会出现在这里。" icon={History} />}</section>;
}

export function TemplatesPage({ publishers }: { publishers: PublisherSnapshot[] }) {
  const [selected, setSelected] = useState("");
  const [templates, setTemplates] = useState<CodeTemplate[]>([]);
  const [language, setLanguage] = useState("");
  const [copied, setCopied] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => { setSelected(publishers[0]?.id ?? ""); }, [publishers]);
  useEffect(() => { if (!selected) { setTemplates([]); return; } void getPublisherTemplates(selected).then((next) => { setTemplates(next); setLanguage(next[0]?.language ?? ""); }).catch((reason) => setMessage(readError(reason))); }, [selected]);
  const current = templates.find((template) => template.language === language) ?? templates[0];
  const copy = async () => { if (!current) return; await navigator.clipboard.writeText(current.code); setCopied(true); window.setTimeout(() => setCopied(false), 1600); };
  if (!publishers.length) return <EmptyPage page="暂无可用模板" hint="创建 Publisher 后，系统会根据当前端点生成八类语言模板。" icon={Code} />;
  return <section className="templates-view"><div className="section-heading"><div><p className="eyebrow">Live Documentation</p><h2>活文档与调用模板</h2></div><Badge color="success" variant="soft">由当前 Publisher 生成</Badge></div>{message ? <p className="error-message" role="alert">{message}</p> : null}<label className="field-label">Publisher<Select options={publishers.map((publisher) => ({ value: publisher.id, label: publisher.id }))} value={selected} onChange={(option) => setSelected(option.value)} /></label>{templates.length ? <><div className="template-toolbar"><Select options={templates.map((template) => ({ value: template.language, label: template.title }))} value={language} onChange={(option) => setLanguage(option.value)} /><Button color="secondary" variant="soft" onClick={() => void copy()}><Code />{copied ? "已复制" : "复制代码"}</Button></div><Textarea className="code-template" value={current?.code ?? ""} readOnly rows={18} /></> : <EmptyPage page="正在生成模板" hint="模板从当前 Publisher 的地址、模型和安全占位符实时生成。" icon={Code} />}</section>;
}

export function SettingsPage({ onSnapshot, uiState, onUiState }: { onSnapshot: (snapshot: DesktopSnapshot) => void; uiState: WorkspaceUiState; onUiState: (state: WorkspaceUiState) => void }) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [workspaceJson, setWorkspaceJson] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => { void import("@tauri-apps/plugin-autostart").then(({ isEnabled }) => isEnabled().then(setEnabled)).catch(() => setEnabled(false)); }, []);
  const toggle = async (next: boolean) => { setBusy(true); try { const plugin = await import("@tauri-apps/plugin-autostart"); if (next) await plugin.enable(); else await plugin.disable(); setEnabled(next); } finally { setBusy(false); } };
  const exportCurrent = async () => { setBusy(true); try { const json = await exportWorkspace(); setWorkspaceJson(json); await navigator.clipboard.writeText(json); setMessage("工作区 JSON 已复制；文件中不包含 Secret 明文。"); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  const importCurrent = async () => { if (!workspaceJson.trim()) return; setBusy(true); try { onSnapshot(await importWorkspace(workspaceJson)); setMessage("工作区已导入。缺失的 Secret 会保持未绑定状态。"); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  return <div className="settings-view"><section className="appearance-card"><div><p className="eyebrow">Appearance</p><h2>外观</h2><p>默认跟随 Windows，也可以为 API ARRAY 单独设置主题。</p></div><label>主题<SegmentedControl value={uiState.themePreference} onChange={(themePreference) => onUiState({ ...uiState, themePreference })} aria-label="主题偏好" block><SegmentedControl.Option value="system">系统</SegmentedControl.Option><SegmentedControl.Option value="light">亮色</SegmentedControl.Option><SegmentedControl.Option value="dark">暗色</SegmentedControl.Option></SegmentedControl></label></section><section className="setting-card"><div><div className="setting-title"><SettingsIcon className="size-5" /><h2>开机启动</h2></div><p>仅保存为此设备的桌面偏好，不写入业务工作区。</p></div><Switch checked={enabled ?? false} disabled={busy || enabled === null} label={enabled ? "已开启" : "已关闭"} onCheckedChange={(next) => void toggle(next)} /></section><section className="workspace-transfer"><div className="section-heading"><div><p className="eyebrow">Workspace Transfer</p><h2>导入与导出</h2></div><Badge color="info" variant="soft">Secret 仅保留引用</Badge></div>{message ? <p className="error-message" role="alert">{message}</p> : null}<Textarea value={workspaceJson} onChange={(event) => setWorkspaceJson(event.target.value)} rows={12} placeholder="导出工作区后显示 JSON；也可以在此粘贴另一台设备导出的工作区。" /><div className="form-actions"><Button color="secondary" variant="soft" loading={busy} onClick={() => void exportCurrent()}><FileDocument />导出并复制</Button><Button color="primary" loading={busy} disabled={!workspaceJson.trim()} onClick={() => void importCurrent()}><CheckCircle />导入工作区</Button></div></section></div>;
}
