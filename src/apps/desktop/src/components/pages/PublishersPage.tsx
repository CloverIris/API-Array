import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Pause, Play, Plugin, Stop } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import { changePublisherState, type DesktopSnapshot, type ProjectTree, type PublisherSnapshot } from "../../lib/desktop";
import { EmptyPage, StatusBadge, readError } from "../shared";

export function PublishersPage({ tree, publishers, onSnapshot, onOpenCanvas }: { tree: ProjectTree; publishers: PublisherSnapshot[]; onSnapshot: (snapshot: DesktopSnapshot) => void; onOpenCanvas: (projectId: string, canvasId: string) => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const owners = new Map<string, { projectId: string; canvasId: string; canvasName: string }>();
  for (const project of Object.values(tree.projects)) for (const canvas of Object.values(project.canvases)) if (canvas.publisherId) owners.set(canvas.publisherId, { projectId: project.id, canvasId: canvas.id, canvasName: canvas.name });
  const act = async (action: "start" | "pause" | "stop", publisherId: string) => { setBusy(`${action}:${publisherId}`); setMessage(null); try { onSnapshot(await changePublisherState(action, publisherId)); } catch (reason) { setMessage(readError(reason)); } finally { setBusy(null); } };
  return <div className="reading-page-inner publisher-page"><header className="page-heading"><div><p className="eyebrow">GLOBAL PUBLISHERS</p><h1>Publisher 总面板</h1><p>这里用于监控、诊断和控制已有实例。新 Publisher 只能在所属 Canvas 中创建。</p></div><Badge color="secondary" variant="soft">{publishers.length} 个实例</Badge></header>{message ? <p className="error-message" role="alert">{message}</p> : null}{publishers.length ? <div className="publisher-list">{publishers.map((publisher) => { const owner = owners.get(publisher.id); return <article className="publisher-card" key={publisher.id}><div><div className="publisher-title"><Plugin className="size-4" /><h2>{publisher.id}</h2><StatusBadge status={publisher.status} /></div><p>{publisher.message ?? "等待运行操作"}</p><small>{owner ? `归属：${owner.canvasName}` : "待整理：未绑定 Canvas"}</small></div><div className="actions"><Button color="success" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("start", publisher.id)}><Play />启动</Button><Button color="warning" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("pause", publisher.id)}><Pause />暂停</Button><Button color="danger" variant="soft" size="sm" disabled={busy !== null} onClick={() => void act("stop", publisher.id)}><Stop />停止</Button>{owner ? <Button color="secondary" size="sm" onClick={() => onOpenCanvas(owner.projectId, owner.canvasId)}>打开 Canvas</Button> : null}</div></article>; })}</div> : <EmptyPage page="尚无 Publisher" hint="打开一个 Canvas 并点击运行，完成该 Canvas 的首次发布配置。" icon={Plugin} />}</div>;
}
