import { Branch } from "@openai/apps-sdk-ui/components/Icon";
import type { ReactNode } from "react";
import type { DesktopSnapshot, ProjectTree, WorkspaceUiState } from "../../lib/desktop";
import type { DesktopRoute } from "../../hooks/useDesktopWorkspaceController";
import type { AppPage } from "../navigation";
import { navigation } from "../navigation";
import { LeftSidebar } from "./LeftSidebar";
import { RightInspector } from "./RightInspector";
import { RuntimeStatusBar } from "./RuntimeStatusBar";
import { UnifiedTopBar } from "./UnifiedTopBar";

export function AppShell({ control, route, projectTree, uiState, error, children, inspector, onNavigate, onOpenCanvas, onTree, onRefresh, onUiState }: { control: NonNullable<DesktopSnapshot["control"]>; route: DesktopRoute; projectTree: ProjectTree; uiState: WorkspaceUiState; error: string | null; children: ReactNode; inspector: ReactNode; onNavigate: (page: AppPage) => void; onOpenCanvas: (projectId: string, canvasId: string) => void; onTree: (tree: ProjectTree) => void; onRefresh: () => void; onUiState: (next: WorkspaceUiState) => void }) {
  const global = route.kind === "global" ? navigation.find((item) => item.id === route.page)! : null;
  const canvas = route.kind === "canvas" ? projectTree.projects[route.projectId]?.canvases[route.canvasId] : null;
  const shell = uiState.shell;
  const patchShell = (patch: Partial<WorkspaceUiState["shell"]>) => onUiState({ ...uiState, shell: { ...shell, ...patch } });
  const docked = shell.rightInspectorOpen && shell.rightInspectorPinned;
  const columns = `${shell.leftSidebarCollapsed ? 52 : shell.leftWidth}px minmax(0, 1fr)${docked ? ` ${shell.rightWidth}px` : ""}`;
  return <main className="desktop-shell"><UnifiedTopBar workspaceName={control.workspaceName} pageLabel={global?.label ?? canvas?.name ?? "Canvas"} pageIcon={global?.icon ?? Branch} theme={uiState.themePreference} onTheme={(themePreference) => onUiState({ ...uiState, themePreference })} onRefresh={onRefresh} onOpenInspector={() => patchShell({ rightInspectorOpen: true })} /><div className="three-columns" style={{ gridTemplateColumns: columns }}><LeftSidebar page={global?.id ?? null} tree={projectTree} publishers={control.supervisor.publishers ?? []} uiState={uiState} collapsed={shell.leftSidebarCollapsed} onNavigate={onNavigate} onOpenCanvas={onOpenCanvas} onTree={onTree} onUiState={onUiState} onToggle={() => patchShell({ leftSidebarCollapsed: !shell.leftSidebarCollapsed })} /><section className={`content-panel ${route.kind === "canvas" ? "page-canvas" : `page-${route.page}`}`}>{error ? <p className="error-message" role="alert">{error}</p> : null}<div className={route.kind === "canvas" || route.page === "overview" ? "canvas-page" : "reading-page"}>{children}</div></section><RightInspector open={shell.rightInspectorOpen} pinned={shell.rightInspectorPinned} width={shell.rightWidth} title={global?.label ?? canvas?.name ?? "Canvas"} onClose={() => patchShell({ rightInspectorOpen: false })} onTogglePin={() => patchShell({ rightInspectorPinned: !shell.rightInspectorPinned })} onWidthChange={(rightWidth) => patchShell({ rightWidth })}>{inspector}</RightInspector></div><RuntimeStatusBar control={control} /></main>;
}
