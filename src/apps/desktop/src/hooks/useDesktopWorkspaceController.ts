"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDesktopSnapshot,
  getControlCenterSnapshot,
  getProjectTree,
  getWalletGallery,
  getWorkspaceUiState,
  saveWorkspaceUiState,
  type CanvasTab,
  type DesktopSnapshot,
  type ControlCenterSnapshot,
  type ProjectTree,
  type WorkspaceUiState,
  type WalletCard,
} from "../lib/desktop";
import type { AppPage } from "../components/navigation";
import { readError } from "../components/shared";

export type DesktopRoute =
  | { kind: "global"; page: AppPage }
  | { kind: "canvas"; projectId: string; canvasId: string; tab: CanvasTab };

export const defaultUiState: WorkspaceUiState = {
  schemaVersion: 6,
  themePreference: "system",
  lastPage: "home",
  workspaceIntent: "manage_apis",
  shell: { leftSidebarCollapsed: false, rightInspectorOpen: false, rightInspectorPinned: false, leftWidth: 248, rightWidth: 320 },
  workflows: {},
  selectedProjectId: null,
  selectedCanvasId: null,
  expandedProjectIds: [],
  expandedFolderIds: [],
  canvasTabs: {},
};

export function useDesktopWorkspaceController() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot | null>(null);
  const [projectTree, setProjectTree] = useState<ProjectTree>({ projects: {} });
  const [uiState, setUiState] = useState<WorkspaceUiState>(defaultUiState);
  const [controlCenter, setControlCenter] = useState<ControlCenterSnapshot | null>(null);
  const [wallet, setWallet] = useState<WalletCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const [nextSnapshot, nextTree, nextWallet] = await Promise.all([getDesktopSnapshot(), getProjectTree(), getWalletGallery()]);
      setSnapshot(nextSnapshot);
      setProjectTree(normalizeProjectTree(nextTree));
      setWallet(nextWallet.filter((item) => item.source === "asset"));
    } catch (reason) { setError(readError(reason)); } finally { setLoading(false); }
  }, []);

  const refreshControlCenter = useCallback(async () => {
    try { setControlCenter(await getControlCenterSnapshot()); }
    catch { setControlCenter(null); }
  }, []);

  useEffect(() => {
    void Promise.all([refresh(), refreshControlCenter(), getWorkspaceUiState().then((next) => setUiState(normalizeUiState(next))).catch(() => undefined)]);
  }, [refresh, refreshControlCenter]);

  const updateUiState = useCallback((updater: (current: WorkspaceUiState) => WorkspaceUiState) => {
    setUiState((current) => {
      const next = normalizeUiState(updater(current));
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => void saveWorkspaceUiState(next).catch((reason) => setError(readError(reason))), 300);
      return next;
    });
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => listen<string>("desktop:navigate", (event) => {
      if (isAppPage(event.payload)) updateUiState((current) => ({ ...current, lastPage: event.payload, selectedProjectId: null, selectedCanvasId: null }));
    })).then((dispose) => { unlisten = dispose; }).catch(() => undefined);
    return () => unlisten?.();
  }, [updateUiState]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => listen("desktop:instances-changed", () => void refreshControlCenter())).then((dispose) => { unlisten = dispose; }).catch(() => undefined);
    const timer = window.setInterval(() => void refreshControlCenter(), 15_000);
    return () => { unlisten?.(); window.clearInterval(timer); };
  }, [refreshControlCenter]);

  useEffect(() => () => { if (saveTimer.current) clearTimeout(saveTimer.current); }, []);

  const openGlobal = useCallback((page: AppPage) => updateUiState((current) => ({ ...current, lastPage: page, selectedProjectId: null, selectedCanvasId: null })), [updateUiState]);
  const openCanvas = useCallback((projectId: string, canvasId: string, tab?: CanvasTab) => updateUiState((current) => ({ ...current, selectedProjectId: projectId, selectedCanvasId: canvasId, expandedProjectIds: unique([...current.expandedProjectIds, projectId]), canvasTabs: tab ? { ...current.canvasTabs, [canvasId]: tab } : current.canvasTabs })), [updateUiState]);
  const setCanvasTab = useCallback((tab: CanvasTab) => updateUiState((current) => current.selectedCanvasId ? ({ ...current, canvasTabs: { ...current.canvasTabs, [current.selectedCanvasId]: tab } }) : current), [updateUiState]);
  const toggleProject = useCallback((id: string) => updateUiState((current) => ({ ...current, expandedProjectIds: toggle(current.expandedProjectIds, id) })), [updateUiState]);
  const toggleFolder = useCallback((id: string) => updateUiState((current) => ({ ...current, expandedFolderIds: toggle(current.expandedFolderIds, id) })), [updateUiState]);

  const selectedCanvas = uiState.selectedProjectId && uiState.selectedCanvasId ? projectTree.projects[uiState.selectedProjectId]?.canvases[uiState.selectedCanvasId] : null;
  const route: DesktopRoute = selectedCanvas && uiState.selectedProjectId && uiState.selectedCanvasId
    ? { kind: "canvas", projectId: uiState.selectedProjectId, canvasId: uiState.selectedCanvasId, tab: uiState.canvasTabs[uiState.selectedCanvasId] ?? "overview" }
    : { kind: "global", page: isAppPage(uiState.lastPage) ? uiState.lastPage : "home" };

  return { snapshot, setSnapshot, controlCenter, setControlCenter, refreshControlCenter, wallet, projectTree, setProjectTree, route, uiState, updateUiState, openGlobal, openCanvas, setCanvasTab, toggleProject, toggleFolder, loading, error, setError, refresh };
}

function normalizeProjectTree(tree: ProjectTree): ProjectTree {
  const projects = Object.fromEntries(Object.entries(tree?.projects ?? {}).map(([id, project]) => [id, { ...project, folders: Object.fromEntries(Object.entries(project?.folders ?? {}).map(([folderId, folder]) => [folderId, { ...folder, canvasIds: Array.isArray(folder?.canvasIds) ? folder.canvasIds : [] }])), canvases: project?.canvases ?? {} }]));
  return { projects };
}

function normalizeUiState(state: WorkspaceUiState): WorkspaceUiState {
  const oldPage = state?.lastPage;
  const lastPage = oldPage === "overview" ? "wallet" : oldPage === "workflows" ? "compositions" : oldPage === "instances" ? "home" : oldPage;
  return { ...defaultUiState, ...state, schemaVersion: 6, lastPage: isAppPage(lastPage) ? lastPage : "home", shell: { ...defaultUiState.shell, ...(state?.shell ?? {}) }, workflows: state?.workflows ?? {}, selectedProjectId: state?.selectedProjectId ?? null, selectedCanvasId: state?.selectedCanvasId ?? null, expandedProjectIds: Array.isArray(state?.expandedProjectIds) ? state.expandedProjectIds : [], expandedFolderIds: Array.isArray(state?.expandedFolderIds) ? state.expandedFolderIds : [], canvasTabs: state?.canvasTabs ?? {} };
}

function toggle(items: string[], id: string) { return items.includes(id) ? items.filter((item) => item !== id) : [...items, id]; }
function unique(items: string[]) { return [...new Set(items)]; }
function isAppPage(value: unknown): value is AppPage { return typeof value === "string" && ["home", "wallet", "direct", "compositions", "runs", "notifications", "templates", "settings"].includes(value); }
