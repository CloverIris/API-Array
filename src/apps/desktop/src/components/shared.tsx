import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { EmptyMessage } from "@openai/apps-sdk-ui/components/EmptyMessage";
import type { AppIcon } from "./navigation";
import type { PublisherLifecycle } from "../lib/desktop";

export function StatusBadge({ status }: { status: PublisherLifecycle }) {
  const color = status === "running" ? "success" : status === "failed" ? "danger" : status === "paused" ? "warning" : "secondary";
  return <Badge color={color} variant="soft" pill>{statusLabel(status)}</Badge>;
}

export function EmptyPage({ page, hint, icon: Icon }: { page: string; hint: string; icon: AppIcon }) {
  return <EmptyMessage className="empty-state" fill="static"><EmptyMessage.Icon><Icon /></EmptyMessage.Icon><EmptyMessage.Title>{page}</EmptyMessage.Title><EmptyMessage.Description>{hint}</EmptyMessage.Description></EmptyMessage>;
}

export function Metric({ label, value, detail, icon: Icon }: { label: string; value: string; detail: string; icon: AppIcon }) {
  return <article className="metric"><Icon className="metric-icon" aria-hidden="true" /><p>{label}</p><strong>{value}</strong><small>{detail}</small></article>;
}

export function StateLine({ label, value }: { label: string; value: string }) {
  return <div className="state-line"><span>{label}</span><strong>{value}</strong></div>;
}

export function readError(reason: unknown) {
  if (typeof reason === "string") return reason;
  if (reason instanceof Error) return reason.message;
  return "桌面服务暂时不可用，请检查运行日志。";
}

export function formatTimestamp(value?: number) {
  return value ? new Date(value).toLocaleString("zh-CN") : "本地运行事件";
}

function statusLabel(status: PublisherLifecycle) {
  return ({ running: "运行中", paused: "已暂停", stopped: "已停止", failed: "失败", starting: "启动中", stopping: "停止中" } as Record<string, string>)[status] ?? status;
}
