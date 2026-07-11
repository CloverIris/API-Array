import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { ApiKeys, Pause, Play, Plugin, Stop } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { changePublisherState, createPublisher, getProviderInstances, type DesktopSnapshot, type ProviderInstanceItem, type PublisherSnapshot } from "../../lib/desktop";
import { EmptyPage, StatusBadge, readError } from "../shared";

export function PublishersPage({ publishers, onSnapshot }: { publishers: PublisherSnapshot[]; onSnapshot: (snapshot: DesktopSnapshot) => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [instances, setInstances] = useState<ProviderInstanceItem[]>([]);
  const [providerInstance, setProviderInstance] = useState("");
  const [id, setId] = useState("local-api");
  const [model, setModel] = useState("smart");
  const [port, setPort] = useState("6188");
  const [token, setToken] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => { void getProviderInstances().then((next) => { setInstances(next); setProviderInstance(next[0]?.id ?? ""); }).catch((reason) => setMessage(readError(reason))); }, []);
  const act = async (action: "start" | "pause" | "stop", publisherId: string) => { setBusy(`${action}:${publisherId}`); try { onSnapshot(await changePublisherState(action, publisherId)); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(null); } };
  const create = async () => { setBusy("create"); setMessage(null); try { onSnapshot(await createPublisher({ id, name: id, providerInstance, publicModel: model, upstreamModel: model, port: Number(port), token })); setToken(""); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(null); } };
  return <div className="publisher-page">{message ? <p className="error-message" role="alert">{message}</p> : null}<section className="form-card"><div className="section-heading"><div><p className="eyebrow">Local Publisher</p><h2>创建本地统一端点</h2></div><Badge color="info" variant="soft">仅回环监听</Badge></div>{instances.length ? <div className="form-grid"><label className="field-label">Provider 实例<Select options={instances.map((instance) => ({ value: instance.id, label: `${instance.id} · ${instance.name}` }))} value={providerInstance} onChange={(option) => setProviderInstance(option.value)} /></label><label className="field-label">Publisher ID<Input value={id} onChange={(event) => setId(event.target.value)} /></label><label className="field-label">公开模型名<Input value={model} onChange={(event) => setModel(event.target.value)} /></label><label className="field-label">端口<Input type="number" value={port} onChange={(event) => setPort(event.target.value)} /></label><label className="field-label">Publisher Token<Input type="password" value={token} onChange={(event) => setToken(event.target.value)} placeholder="只写入本机凭据存储" autoComplete="new-password" /></label></div> : <EmptyPage page="请先配置 Provider" hint="Publisher 必须关联一个已配置的 Provider 实例。" icon={ApiKeys} />}<Button color="primary" loading={busy === "create"} disabled={!instances.length} onClick={() => void create()}><Plugin />创建 Publisher</Button></section>{publishers.length ? <div className="publisher-list">{publishers.map((publisher) => <article className="publisher-card" key={publisher.id}><div><div className="publisher-title"><Plugin className="size-4" /><h2>{publisher.id}</h2><StatusBadge status={publisher.status} /></div><p>{publisher.message ?? "等待桌面操作"}</p></div><div className="actions"><Button color="success" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("start", publisher.id)}><Play />启动</Button><Button color="warning" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("pause", publisher.id)}><Pause />暂停</Button><Button color="danger" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("stop", publisher.id)}><Stop />停止</Button></div></article>)}</div> : <EmptyPage page="尚无 Publisher" hint="创建第一个本机统一端点后，它会出现在这里。" icon={Plugin} />}</div>;
}
