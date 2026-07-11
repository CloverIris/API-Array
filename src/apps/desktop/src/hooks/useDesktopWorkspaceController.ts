"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDesktopSnapshot,
  getWorkspaceUiState,
  saveWorkspaceUiState,
  type DesktopSnapshot,
  type WorkspaceUiState,
} from "../lib/desktop";
import type { AppPage } from "../components/navigation";
import { readError } from "../components/shared";

export type WorkspaceSelection =
  | { kind: "workspace" }
  | { kind: "node"; id: string }
  | { kind: "edge"; id: string };

export const defaultUiState: WorkspaceUiState = {
  schemaVersion: 2,
  themePreference: "system",
  viewMode: "simple",
  lastPage: "overview",
  workspaceIntent: "manage_apis",
  shell: {
    leftSidebarCollapsed: false,
    rightInspectorOpen: false,
    rightInspectorPinned: false,
    leftWidth: 188,
    rightWidth: 320,
  },
  workflows: {},
};

export function useDesktopWorkspaceController() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot | null>(null);
  const [page, setPage] = useState<AppPage>("overview");
  const [selection, setSelection] = useState<WorkspaceSelection>({ kind: "workspace" });
  const [uiState, setUiState] = useState<WorkspaceUiState>(defaultUiState);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      setSnapshot(await getDesktopSnapshot());
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void Promise.all([refresh(), getWorkspaceUiState().then((next) => {
      setUiState(next);
      if (isAppPage(next.lastPage)) setPage(next.lastPage);
    }).catch(() => undefined)]);
  }, [refresh]);

  const updateUiState = useCallback((updater: (current: WorkspaceUiState) => WorkspaceUiState) => {
    setUiState((current) => {
      const next = updater(current);
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => {
        void saveWorkspaceUiState(next).catch((reason) => setError(readError(reason)));
      }, 350);
      return next;
    });
  }, []);

  useEffect(() => () => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
  }, []);

  const navigate = useCallback((next: AppPage) => {
    setPage(next);
    setSelection({ kind: "workspace" });
    updateUiState((current) => ({ ...current, lastPage: next }));
  }, [updateUiState]);

  return {
    snapshot,
    setSnapshot,
    page,
    navigate,
    selection,
    setSelection,
    uiState,
    updateUiState,
    loading,
    error,
    setError,
    refresh,
  };
}

function isAppPage(value: string): value is AppPage {
  return ["overview", "assets", "workflows", "publishers", "runs", "notifications", "templates", "settings"].includes(value);
}
