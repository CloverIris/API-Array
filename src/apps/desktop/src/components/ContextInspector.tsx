import type { DesktopSnapshot } from "../lib/desktop";
import type { AppPage } from "./navigation";
import { StateLine } from "./shared";
import { WorkflowInspector, type SelectedWorkflowItem } from "./workflow/WorkflowInspector";

export function ContextInspector({ page, control, workflowSelection }: { page: AppPage; control: NonNullable<DesktopSnapshot["control"]>; workflowSelection: SelectedWorkflowItem }) {
  if (page === "workflows") return <WorkflowInspector selected={workflowSelection} />;
  const publishers = control.supervisor.publishers ?? [];
  return <><StateLine label="工作区" value={control.workspaceId} /><StateLine label="恢复来源" value={control.recoveredFromBackup ? "备份恢复" : "主工作区"} /><StateLine label="Provider" value={`${control.providerCount} 个`} /><StateLine label="缺失 Secret" value={`${control.missingSecretCount} 项`} /><StateLine label="运行 Publisher" value={`${publishers.filter((item) => item.status === "running").length} 个`} /><div className="next-step"><strong>安全边界</strong><p>所有 Publisher 只监听回环地址。Secret 值不会通过 IPC 返回到界面。</p></div></>;
}
