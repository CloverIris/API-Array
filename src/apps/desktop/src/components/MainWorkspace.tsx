import type { DesktopSnapshot, WorkspaceUiState } from "../lib/desktop";
import type { AppPage } from "./navigation";
import type { SelectedWorkflowItem } from "./workflow/WorkflowInspector";
import { WorkflowCanvas } from "./workflow/WorkflowCanvas";
import { AssetsPage } from "./pages/AssetsPage";
import { PublishersPage } from "./pages/PublishersPage";
import { NotificationsPage, RunsPage, SettingsPage, TemplatesPage } from "./pages/BasicPages";
import { GuidedHome } from "./pages/GuidedHome";
import { WorkflowSummary } from "./workflow/WorkflowSummary";

export function MainWorkspace({ page, snapshot, uiState, onSnapshot, onUiState, onWorkflowSelection, onNavigate }: { page: AppPage; snapshot: DesktopSnapshot; uiState: WorkspaceUiState; onSnapshot: (snapshot: DesktopSnapshot) => void; onUiState: (state: WorkspaceUiState) => void; onWorkflowSelection: (selection: SelectedWorkflowItem) => void; onNavigate: (page: AppPage) => void }) {
  const control = snapshot.control!;
  const publishers = control.supervisor.publishers ?? [];
  if (page === "overview") return <GuidedHome control={control} intent={uiState.workspaceIntent} onNavigate={onNavigate} />;
  if (page === "assets") return <AssetsPage />;
  if (page === "workflows") return uiState.viewMode === "professional" ? <WorkflowCanvas uiState={uiState} onUiState={onUiState} publisherRunning={publishers.some((item) => item.status === "running")} onSelection={onWorkflowSelection} /> : <WorkflowSummary control={control} onProfessional={() => onUiState({ ...uiState, viewMode: "professional", shell: { ...uiState.shell, leftSidebarCollapsed: true } })} />;
  if (page === "publishers") return <PublishersPage publishers={publishers} onSnapshot={onSnapshot} />;
  if (page === "runs") return <RunsPage />;
  if (page === "notifications") return <NotificationsPage notifications={control.notifications} />;
  if (page === "templates") return <TemplatesPage publishers={publishers} />;
  return <SettingsPage onSnapshot={onSnapshot} uiState={uiState} onUiState={onUiState} />;
}
