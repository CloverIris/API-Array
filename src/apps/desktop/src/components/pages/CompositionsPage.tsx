import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Branch } from "@openai/apps-sdk-ui/components/Icon";
import type { ProjectTree, PublisherSnapshot } from "../../lib/desktop";

export function CompositionsPage({ tree, publishers, onOpenCanvas }: { tree: ProjectTree; publishers: PublisherSnapshot[]; onOpenCanvas: (projectId: string, canvasId: string) => void }) {
  const items = Object.values(tree.projects).flatMap((project) => Object.values(project.canvases).map((canvas) => ({ project, canvas, publisher: publishers.find((item) => item.id === canvas.publisherId) })));
  return <div className="reading-page-inner"><header className="page-heading"><div><p className="eyebrow">COMPOSITIONS</p><h1>综合器</h1><p>跨项目查看路由组合。编辑行为始终跳转到对应 Canvas，不读取旧的全局工作流。</p></div></header>{items.length ? <div className="composition-grid">{items.map(({ project, canvas, publisher }) => <article key={`${project.id}:${canvas.id}`} className="composition-card"><div className="composition-icon"><Branch /></div><div><strong>{canvas.name}</strong><p>{project.name} · {canvas.graph?.nodes?.length ?? 0} 个节点 · {canvas.graph?.edges?.length ?? 0} 条连接</p></div><Badge color={publisher?.status === "running" ? "success" : "secondary"} variant="soft">{publisher?.status ?? "未发布"}</Badge><Button color="secondary" variant="soft" size="sm" onClick={() => onOpenCanvas(project.id, canvas.id)}>打开 Canvas</Button></article>)}</div> : <p className="empty-state">创建 Canvas 后，其路由组合会出现在这里。</p>}</div>;
}
