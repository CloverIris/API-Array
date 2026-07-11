import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Branch, Play, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import type { ProjectTree, PublisherSnapshot } from "../../lib/desktop";

export function GlobalOverview({ tree, publishers, onOpenCanvas }: { tree: ProjectTree; publishers: PublisherSnapshot[]; onOpenCanvas: (projectId: string, canvasId: string) => void }) {
  const canvases = Object.values(tree.projects).flatMap((project) => Object.values(project.canvases).map((canvas) => ({ project, canvas })));
  const running = publishers.filter((item) => item.status === "running");
  return <div className="global-dashboard reading-page-inner">
    <header className="page-heading"><div><p className="eyebrow">GLOBAL RUNTIME</p><h1>运行总览</h1><p>这里汇总所有 Canvas；具体配置与发布仍在各自 Canvas 工作台中完成。</p></div></header>
    <div className="metric-grid"><article><Branch /><span>Canvas</span><strong>{canvases.length}</strong></article><article><Plugin /><span>Publisher</span><strong>{publishers.length}</strong></article><article><Play /><span>正在运行</span><strong>{running.length}</strong></article></div>
    <section className="dashboard-section"><div className="section-heading"><div><p className="eyebrow">CANVAS INSTANCES</p><h2>全部独立实例</h2></div><Badge color={running.length ? "success" : "secondary"} variant="soft">{running.length} 个运行中</Badge></div>
      {canvases.length ? <div className="dashboard-list">{canvases.map(({ project, canvas }) => { const publisher = publishers.find((item) => item.id === canvas.publisherId); return <button key={`${project.id}:${canvas.id}`} className="dashboard-row" onClick={() => onOpenCanvas(project.id, canvas.id)}><span className={`canvas-status-dot status-${publisher?.status ?? "draft"}`} /><span><strong>{canvas.name}</strong><small>{project.name} · {publisher?.message ?? (publisher ? publisher.id : "尚未发布")}</small></span><Badge color={publisher?.status === "running" ? "success" : publisher?.status === "paused" ? "warning" : "secondary"} variant="soft">{publisher?.status ?? "草稿"}</Badge></button>; })}</div> : <p className="empty-state">还没有 Canvas。请从左侧 Project 菜单中新建一个。</p>}
    </section>
  </div>;
}
