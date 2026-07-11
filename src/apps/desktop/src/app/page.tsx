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

export default function HomePage() {
  return <AppsSDKUIProvider linkComponent="a"><DesktopApp /></AppsSDKUIProvider>;
}

function DesktopApp() {
  const controller = useDesktopWorkspaceController();
  const [workflowSelection, setWorkflowSelection] = useState<SelectedWorkflowItem>(null);
  useTheme(controller.uiState.themePreference);
  const updateUi = useCallback((next: typeof controller.uiState) => controller.updateUiState(() => next), [controller.updateUiState]);
  const selectWorkflowItem = useCallback((selection: SelectedWorkflowItem) => {
    setWorkflowSelection((current) => sameSelection(current, selection) ? current : selection);
    if (selection) controller.updateUiState((current) => current.shell.rightInspectorOpen ? current : ({ ...current, shell: { ...current.shell, rightInspectorOpen: true } }));
  }, [controller.updateUiState]);

  if (controller.loading) return <div className="boot-screen"><CheckCircle className="size-5" />正在打开本地工作区…</div>;
  if (!controller.snapshot?.initialized) return <Onboarding initialError={controller.error ?? controller.snapshot?.startupError ?? null} onCreated={(snapshot, workspaceIntent) => { controller.setSnapshot(snapshot); controller.updateUiState((current) => ({ ...current, workspaceIntent, lastPage: "overview" })); }} />;

  const snapshot = controller.snapshot;
  const control = snapshot.control!;
  return <AppShell control={control} page={controller.page} uiState={controller.uiState} error={controller.error} onNavigate={controller.navigate} onRefresh={() => void controller.refresh()} onUiState={updateUi} inspector={<ContextInspector page={controller.page} control={control} workflowSelection={workflowSelection} />}><MainWorkspace page={controller.page} snapshot={snapshot} uiState={controller.uiState} onSnapshot={controller.setSnapshot} onUiState={updateUi} onWorkflowSelection={selectWorkflowItem} onNavigate={controller.navigate} /></AppShell>;
}

function sameSelection(left: SelectedWorkflowItem, right: SelectedWorkflowItem) {
  if (!left || !right) return left === right;
  if (left.kind !== right.kind) return false;
  return left.kind === "node" && right.kind === "node" ? left.node.id === right.node.id : left.kind === "edge" && right.kind === "edge" && left.edge.id === right.edge.id;
}
