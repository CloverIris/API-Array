"use client";

import { AppsSDKUIProvider } from "@openai/apps-sdk-ui/components/AppsSDKUIProvider";
import { CheckCircle } from "@openai/apps-sdk-ui/components/Icon";
import { useCallback, useState } from "react";
import { ContextInspector } from "../components/ContextInspector";
import { MainWorkspace } from "../components/MainWorkspace";
import { Onboarding } from "../components/Onboarding";
import { AppShell } from "../components/shell/AppShell";
import type { SelectedWorkflowItem } from "../components/workflow/WorkflowInspector";
import { useDesktopWorkspaceController } from "../hooks/useDesktopWorkspaceController";
import { useTheme } from "../hooks/useTheme";

export default function HomePage() { return <AppsSDKUIProvider linkComponent="a"><DesktopApp /></AppsSDKUIProvider>; }

function DesktopApp() {
  const controller = useDesktopWorkspaceController();
  const [workflowSelection, setWorkflowSelection] = useState<SelectedWorkflowItem>(null);
  useTheme(controller.uiState.themePreference);
  const updateUi = useCallback((next: typeof controller.uiState) => controller.updateUiState(() => next), [controller.updateUiState]);
  const selectWorkflowItem = useCallback((selection: SelectedWorkflowItem) => { setWorkflowSelection(selection); if (selection) controller.updateUiState((current) => current.shell.rightInspectorOpen ? current : ({ ...current, shell: { ...current.shell, rightInspectorOpen: true } })); }, [controller.updateUiState]);
  if (controller.loading) return <div className="boot-screen"><CheckCircle className="size-5" />正在打开本地工作区…</div>;
  if (!controller.snapshot?.initialized) return <Onboarding initialError={controller.error ?? controller.snapshot?.startupError ?? null} onCreated={(snapshot, workspaceIntent) => { controller.setSnapshot(snapshot); controller.updateUiState((current) => ({ ...current, workspaceIntent, lastPage: "wallet" })); void controller.refresh(); }} />;
  const snapshot = controller.snapshot;
  const control = snapshot.control!;
  const refreshAll = () => { void controller.refresh(); void controller.refreshControlCenter(); };
  return <AppShell control={control} controlCenter={controller.controlCenter} wallet={controller.wallet} route={controller.route} projectTree={controller.projectTree} uiState={controller.uiState} error={controller.error} onNavigate={controller.openGlobal} onOpenCanvas={controller.openCanvas} onTree={controller.setProjectTree} onRefresh={refreshAll} onUiState={updateUi} inspector={<ContextInspector route={controller.route} tree={controller.projectTree} control={control} workflowSelection={workflowSelection} />}><MainWorkspace route={controller.route} projectTree={controller.projectTree} snapshot={snapshot} controlCenter={controller.controlCenter} wallet={controller.wallet} uiState={controller.uiState} onSnapshot={controller.setSnapshot} onControlCenter={controller.setControlCenter} onTree={controller.setProjectTree} onUiState={updateUi} onCanvasTab={controller.setCanvasTab} onOpenCanvas={controller.openCanvas} onNavigate={controller.openGlobal} onWorkflowSelection={selectWorkflowItem} onChanged={refreshAll} /></AppShell>;
}
