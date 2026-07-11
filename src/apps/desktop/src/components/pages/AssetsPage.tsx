import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Textarea } from "@openai/apps-sdk-ui/components/Textarea";
import { ApiKeys, CheckCircle, FileDocument, Key } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { getProviderCatalog, getProviderInstances, removeProviderInstance, runProviderProbe, storeSecret, upsertCustomProviderYaml, upsertProviderInstance, validateProviderYaml, type InspectionReport, type ProviderCatalogItem, type ProviderInstanceItem } from "../../lib/desktop";
import { EmptyPage, readError } from "../shared";

export function AssetsPage() {
  const [catalog, setCatalog] = useState<ProviderCatalogItem[]>([]);
  const [instances, setInstances] = useState<ProviderInstanceItem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [instanceId, setInstanceId] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [secret, setSecret] = useState("");
  const [yaml, setYaml] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [report, setReport] = useState<InspectionReport | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { void Promise.all([getProviderCatalog(), getProviderInstances()]).then(([nextCatalog, nextInstances]) => { setCatalog(nextCatalog); setInstances(nextInstances); setSelectedId(nextCatalog[0]?.id ?? ""); }).catch((reason) => setMessage(readError(reason))); }, []);
  const selected = catalog.find((item) => item.id === selectedId);
  useEffect(() => { if (selected) { setEndpoint(selected.defaultBaseUrl); setYaml(""); } }, [selected]);
  const secretReference = () => `secret://workspace/${instanceId.trim()}/api_key`;
  const saveSecret = async () => { if (secret) { await storeSecret(secretReference(), secret); setSecret(""); } };
  const save = async () => { if (!selected || !instanceId.trim()) { setMessage("请填写 Provider 实例 ID。"); return; } setBusy(true); setMessage(null); try { await saveSecret(); setInstances(await upsertProviderInstance({ providerId: selected.id, instanceId: instanceId.trim(), endpointOverride: endpoint, enabled: true, secretFields: { api_key: secretReference() } })); setMessage("Provider 实例已保存；Secret 不会回显。"); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  const validate = async () => { setBusy(true); try { const result = await validateProviderYaml(yaml); setMessage(result.valid ? `YAML 有效：${result.name ?? result.providerId}` : result.error ?? "YAML 校验失败"); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  const saveYaml = async () => { if (!yaml.trim() || !instanceId.trim()) { setMessage("请先填写实例 ID 和 YAML。"); return; } setBusy(true); try { await saveSecret(); setInstances(await upsertCustomProviderYaml({ yaml, instanceId: instanceId.trim(), endpointOverride: endpoint, secretFields: { api_key: secretReference() } })); setMessage("自定义 Provider 已保存；Secret 不会回显。"); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  const inspect = async (id: string) => { setBusy(true); setMessage(null); try { setReport(await runProviderProbe(id)); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(false); } };
  return <div className="assets-view"><div className="section-heading"><div><p className="eyebrow">Provider Catalog</p><h2>API 资产</h2></div><Badge color="info" variant="soft">{catalog.length} 个内置 Provider</Badge></div>{message ? <p className="error-message" role="alert">{message}</p> : null}<section className="form-card"><div className="form-grid"><label className="field-label">Provider<Select options={catalog.map((item) => ({ value: item.id, label: `${item.name} · ${item.adapter}` }))} value={selectedId} onChange={(option) => setSelectedId(option.value)} placeholder="选择 Provider" /></label><label className="field-label">实例 ID<Input value={instanceId} onChange={(event) => setInstanceId(event.target.value)} placeholder="例如 openai-main" /></label><label className="field-label">Base URL<Input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} disabled={!selected?.editableEndpoint} /></label><label className="field-label">API Key<Input type="password" value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="仅提交到本机凭据存储" autoComplete="new-password" /></label></div><div className="form-actions"><Button color="primary" loading={busy} onClick={() => void save()}><Key />保存实例</Button><Button color="secondary" variant="ghost" onClick={() => setAdvanced((value) => !value)}><FileDocument />{advanced ? "收起 YAML" : "YAML 高级编辑"}</Button></div>{advanced ? <div className="advanced-editor"><Textarea value={yaml} onChange={(event) => setYaml(event.target.value)} rows={10} placeholder="粘贴 Provider YAML，校验后再保存。" /><div className="form-actions"><Button color="secondary" variant="soft" disabled={!yaml.trim()} loading={busy} onClick={() => void validate()}>校验 YAML</Button><Button color="primary" variant="soft" disabled={!yaml.trim() || !instanceId.trim()} loading={busy} onClick={() => void saveYaml()}>保存自定义 Provider</Button></div></div> : null}</section><section><div className="section-heading"><h2>已配置实例</h2><span className="muted">{instances.length} 个</span></div>{instances.length ? <div className="asset-list">{instances.map((instance) => <article className="asset-card" key={instance.id}><div><div className="publisher-title"><ApiKeys className="size-4" /><h3>{instance.id}</h3><Badge color={instance.enabled ? "success" : "secondary"} variant="soft">{instance.enabled ? "已启用" : "已停用"}</Badge></div><p>{instance.name} · {instance.endpointOverride ?? "使用默认端点"}</p></div><div className="actions"><Button color="info" variant="soft" size="sm" loading={busy} onClick={() => void inspect(instance.id)}><CheckCircle />体检</Button><Button color="danger" variant="ghost" size="sm" onClick={() => void removeProviderInstance(instance.id).then(setInstances).catch((reason) => setMessage(readError(reason)))}>删除</Button></div></article>)}</div> : <EmptyPage page="尚无 Provider 实例" hint="从上方选择一个内置 Provider，输入端点和 API Key 后保存。" icon={ApiKeys} />}</section>{report ? <InspectionCard report={report} /> : null}</div>;
}

function InspectionCard({ report }: { report: InspectionReport }) {
  const color = report.overall === "healthy" ? "success" : report.overall === "degraded" ? "warning" : report.overall === "unavailable" ? "danger" : "secondary";
  return <section className="inspection-card"><div className="section-heading"><div><p className="eyebrow">Inspection Report</p><h2>{report.provider_id}</h2></div><Badge color={color} variant="soft" pill>{report.overall}</Badge></div><div className="finding-grid">{Object.values(report.findings).map((finding) => <article className="finding" key={finding.dimension}><div><strong>{finding.dimension}</strong><Badge color={finding.status === "passed" ? "success" : finding.status === "failed" ? "danger" : finding.status === "authorization_required" ? "warning" : "secondary"} variant="soft">{finding.status}</Badge></div><p>{finding.safe_summary ?? "无附加说明"}</p>{finding.latency_ms !== undefined ? <small>{finding.latency_ms} ms</small> : null}</article>)}</div></section>;
}
