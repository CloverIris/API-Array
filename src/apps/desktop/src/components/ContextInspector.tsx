import type { DesktopRoute } from "../hooks/useDesktopWorkspaceController";
import type { DesktopSnapshot, ProjectTree } from "../lib/desktop";
import { StateLine } from "./shared";
import { WorkflowInspector, type SelectedWorkflowItem } from "./workflow/WorkflowInspector";

export function ContextInspector({ route, tree, control, workflowSelection }: { route: DesktopRoute; tree: ProjectTree; control: NonNullable<DesktopSnapshot["control"]>; workflowSelection: SelectedWorkflowItem }) {
  if (route.kind === "canvas" && route.tab === "workflow" && workflowSelection) return <WorkflowInspector selected={workflowSelection} />;
  if (route.kind === "canvas") {
    const canvas = tree.projects[route.projectId]?.canvases[route.canvasId];
    return <><StateLine label="Canvas" value={canvas?.name ?? route.canvasId} /><StateLine label="草稿版本" value={`r${canvas?.draftRevision ?? 0}`} /><StateLine label="已应用版本" value={`r${canvas?.appliedRevision ?? 0}`} /><StateLine label="Publisher" value={canvas?.publisherId ?? "尚未绑定"} /><div className="next-step"><strong>独立运行边界</strong><p>这里的图、Publisher、活文档与记录只属于当前 Canvas。钱包资产仍由整个工作区共享。</p></div></>;
  }
  const publishers = control.supervisor.publishers ?? [];
  return <><StateLine label="工作区" value={control.workspaceId} /><StateLine label="恢复来源" value={control.recoveredFromBackup ? "备份恢复" : "主工作区"} /><StateLine label="Provider" value={`${control.providerCount} 个`} /><StateLine label="缺失 Secret" value={`${control.missingSecretCount} 项`} /><StateLine label="运行 Publisher" value={`${publishers.filter((item) => item.status === "running").length} 个`} /><div className="next-step"><strong>安全边界</strong><p>Publisher 仅监听回环地址，Secret 值不会通过 IPC 返回到界面。</p></div></>;
}
