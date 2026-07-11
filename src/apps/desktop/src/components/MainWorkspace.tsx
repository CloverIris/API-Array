import type { DesktopRoute } from "../hooks/useDesktopWorkspaceController";
import type { DesktopSnapshot, ProjectTree, WorkspaceUiState } from "../lib/desktop";
import { CanvasWorkspace } from "./canvas/CanvasWorkspace";
import type { SelectedWorkflowItem } from "./workflow/WorkflowInspector";
import { AssetsPage } from "./pages/AssetsPage";
import { NotificationsPage, RunsPage, SettingsPage, TemplatesPage } from "./pages/BasicPages";
import { CompositionsPage } from "./pages/CompositionsPage";
import { GlobalOverview } from "./pages/GlobalOverview";
import { PublishersPage } from "./pages/PublishersPage";

export function MainWorkspace({ route, projectTree, snapshot, uiState, onSnapshot, onTree, onUiState, onCanvasTab, onOpenCanvas, onWorkflowSelection, onChanged }: {
  route: DesktopRoute;
  projectTree: ProjectTree;
  snapshot: DesktopSnapshot;
  uiState: WorkspaceUiState;
  onSnapshot: (snapshot: DesktopSnapshot) => void;
  onTree: (tree: ProjectTree) => void;
  onUiState: (state: WorkspaceUiState) => void;
  onCanvasTab: (tab: import("../lib/desktop").CanvasTab) => void;
  onOpenCanvas: (projectId: string, canvasId: string) => void;
  onWorkflowSelection: (selection: SelectedWorkflowItem) => void;
  onChanged: () => void;
}) {
  const control = snapshot.control!;
  const publishers = control.supervisor.publishers ?? [];
  if (route.kind === "canvas") return <CanvasWorkspace projectId={route.projectId} canvasId={route.canvasId} tab={route.tab} uiState={uiState} onTab={onCanvasTab} onUiState={onUiState} onDesktopSnapshot={onSnapshot} onWorkflowSelection={onWorkflowSelection} onChanged={onChanged} />;
  if (route.page === "overview") return <GlobalOverview tree={projectTree} publishers={publishers} onOpenCanvas={onOpenCanvas} />;
  if (route.page === "assets") return <AssetsPage />;
  if (route.page === "workflows") return <CompositionsPage tree={projectTree} publishers={publishers} onOpenCanvas={onOpenCanvas} />;
  if (route.page === "publishers") return <PublishersPage tree={projectTree} publishers={publishers} onSnapshot={onSnapshot} onOpenCanvas={onOpenCanvas} />;
  if (route.page === "runs") return <RunsPage />;
  if (route.page === "notifications") return <NotificationsPage notifications={control.notifications} />;
  if (route.page === "templates") return <TemplatesPage publishers={publishers} />;
  return <SettingsPage onSnapshot={onSnapshot} uiState={uiState} onUiState={onUiState} />;
}
