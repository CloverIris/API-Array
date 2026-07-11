import { invoke } from "@tauri-apps/api/core";

export type PublisherLifecycle = "stopped" | "running" | "paused" | "failed" | string;

export interface PublisherSnapshot {
  id: string;
  status: PublisherLifecycle;
  message?: string | null;
}

export interface ControlSnapshot {
  workspaceId: string;
  workspaceName: string;
  recoveredFromBackup: boolean;
  requiredSecretCount: number;
  missingSecretCount: number;
  supervisor: { publishers: PublisherSnapshot[] };
  notifications: Array<{ id?: string; message: string; createdAt?: number }>;
}

export interface DesktopSnapshot {
  initialized: boolean;
  startupError: string | null;
  control: ControlSnapshot | null;
}

export const getDesktopSnapshot = () => invoke<DesktopSnapshot>("desktop_snapshot");

export const initializeWorkspace = (name: string) =>
  invoke<DesktopSnapshot>("initialize_workspace", { name });

export const changePublisherState = (
  action: "start" | "pause" | "stop",
  publisherId: string,
) => invoke<DesktopSnapshot>(`${action}_publisher`, { publisherId });
