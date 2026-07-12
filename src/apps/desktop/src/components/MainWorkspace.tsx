import type { DesktopRoute } from "../hooks/useDesktopWorkspaceController";
import type { ControlCenterSnapshot, DesktopSnapshot, ProjectTree, WalletCard, WorkspaceUiState } from "../lib/desktop";
import { CanvasWorkspace } from "./canvas/CanvasWorkspace";
import type { SelectedWorkflowItem } from "./workflow/WorkflowInspector";
import { NotificationsPage, RunsPage, SettingsPage, TemplatesPage } from "./pages/BasicPages";
import { CompositionsPage } from "./pages/CompositionsPage";
import { DirectEndpointsPage } from "./pages/PublishersPage";
import { WalletStation } from "./pages/WalletStation";
import { HomeDashboard } from "./pages/HomeDashboard";
import { EmptyMessage } from "@openai/apps-sdk-ui/components/EmptyMessage";
import { Home } from "@openai/apps-sdk-ui/components/Icon";

export function MainWorkspace({ route, projectTree, snapshot, controlCenter, wallet, uiState, onSnapshot, onControlCenter, onTree, onUiState, onCanvasTab, onOpenCanvas, onNavigate, onWorkflowSelection, onChanged }: {
  route: DesktopRoute;
  projectTree: ProjectTree;
  snapshot: DesktopSnapshot;
  controlCenter: ControlCenterSnapshot | null;
  wallet: WalletCard[];
  uiState: WorkspaceUiState;
  onSnapshot: (snapshot: DesktopSnapshot) => void;
  onControlCenter: (snapshot: ControlCenterSnapshot) => void;
  onTree: (tree: ProjectTree) => void;
  onUiState: (state: WorkspaceUiState) => void;
  onCanvasTab: (tab: import("../lib/desktop").CanvasTab) => void;
  onOpenCanvas: (projectId: string, canvasId: string, tab?: import("../lib/desktop").CanvasTab) => void;
  onNavigate: (page: import("./navigation").AppPage) => void;
  onWorkflowSelection: (selection: SelectedWorkflowItem) => void;
  onChanged: () => void;
}) {
  const control = snapshot.control!;
  const publishers = control.supervisor.publishers ?? [];
  if (route.kind === "canvas") return <CanvasWorkspace projectId={route.projectId} canvasId={route.canvasId} tab={route.tab} uiState={uiState} onTab={onCanvasTab} onUiState={onUiState} onDesktopSnapshot={onSnapshot} onWorkflowSelection={onWorkflowSelection} onChanged={onChanged} />;
  if (route.page === "home" && !controlCenter) return <EmptyMessage fill="absolute"><EmptyMessage.Icon><Home /></EmptyMessage.Icon><EmptyMessage.Title>正在打开主控台</EmptyMessage.Title><EmptyMessage.Description>正在读取本地网关与实例状态。</EmptyMessage.Description></EmptyMessage>;
  if (route.page === "home") return <HomeDashboard workspaceName={control.workspaceName} controlCenter={controlCenter!} wallet={wallet} projectTree={projectTree} onControlCenter={onControlCenter} onRefresh={onChanged} onNavigate={onNavigate} onOpenCanvas={onOpenCanvas} />;
  if (route.page === "wallet") return <WalletStation onOpenDirect={(assetId) => { sessionStorage.setItem("apiarray.direct.asset", assetId); onNavigate("direct"); }} />;
  if (route.page === "compositions") return <CompositionsPage tree={projectTree} publishers={publishers} onOpenCanvas={onOpenCanvas} />;
  if (route.page === "direct") return <DirectEndpointsPage snapshot={snapshot} />;
  if (route.page === "runs") return <RunsPage />;
  if (route.page === "notifications") return <NotificationsPage />;
  if (route.page === "templates") return <TemplatesPage publishers={publishers} />;
  return <SettingsPage snapshot={snapshot} onSnapshot={onSnapshot} uiState={uiState} onUiState={onUiState} />;
}
