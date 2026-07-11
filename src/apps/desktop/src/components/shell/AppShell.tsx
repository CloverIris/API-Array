import type { ReactNode } from "react";
import type { DesktopSnapshot, WorkspaceUiState } from "../../lib/desktop";
import type { AppPage } from "../navigation";
import { navigation } from "../navigation";
import { LeftSidebar } from "./LeftSidebar";
import { RightInspector } from "./RightInspector";
import { RuntimeStatusBar } from "./RuntimeStatusBar";
import { UnifiedTopBar } from "./UnifiedTopBar";

export function AppShell({ control, page, uiState, error, children, inspector, onNavigate, onRefresh, onUiState }: { control: NonNullable<DesktopSnapshot["control"]>; page: AppPage; uiState: WorkspaceUiState; error: string | null; children: ReactNode; inspector: ReactNode; onNavigate: (page: AppPage) => void; onRefresh: () => void; onUiState: (next: WorkspaceUiState) => void }) {
  const selected = navigation.find((item) => item.id === page)!;
  const shell = uiState.shell;
  const patchShell = (patch: Partial<WorkspaceUiState["shell"]>) => onUiState({ ...uiState, shell: { ...shell, ...patch } });
  const docked = shell.rightInspectorOpen && shell.rightInspectorPinned;
  const columns = `${shell.leftSidebarCollapsed ? 60 : shell.leftWidth}px minmax(0, 1fr)${docked ? ` ${shell.rightWidth}px` : ""}`;
  return <main className="desktop-shell"><UnifiedTopBar workspaceName={control.workspaceName} pageLabel={selected.label} pageIcon={selected.icon} viewMode={uiState.viewMode} theme={uiState.themePreference} onViewMode={(viewMode) => onUiState({ ...uiState, viewMode })} onTheme={(themePreference) => onUiState({ ...uiState, themePreference })} onRefresh={onRefresh} onOpenInspector={() => patchShell({ rightInspectorOpen: true })} /><div className="three-columns" style={{ gridTemplateColumns: columns }}><LeftSidebar page={page} collapsed={shell.leftSidebarCollapsed} onNavigate={onNavigate} onToggle={() => patchShell({ leftSidebarCollapsed: !shell.leftSidebarCollapsed })} /><section className={`content-panel page-${page}`}>{error ? <p className="error-message" role="alert">{error}</p> : null}<div className={page === "workflows" && uiState.viewMode === "professional" ? "canvas-page" : "reading-page"}>{children}</div></section><RightInspector open={shell.rightInspectorOpen} pinned={shell.rightInspectorPinned} width={shell.rightWidth} title={selected.label} onClose={() => patchShell({ rightInspectorOpen: false })} onTogglePin={() => patchShell({ rightInspectorPinned: !shell.rightInspectorPinned })} onWidthChange={(rightWidth) => patchShell({ rightWidth })}>{inspector}</RightInspector></div><RuntimeStatusBar control={control} /></main>;
}
