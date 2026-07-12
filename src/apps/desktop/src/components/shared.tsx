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

export async function copyText(value: string) {
  if (document.hasFocus() && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value);
      return;
    } catch {
      // WebView2 may briefly lose focus when a native menu or dialog closes.
    }
  }
  const input = document.createElement("textarea");
  input.value = value;
  input.readOnly = true;
  input.setAttribute("aria-hidden", "true");
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.appendChild(input);
  input.select();
  const copied = document.execCommand("copy");
  input.remove();
  if (!copied) throw new Error("应用窗口当前没有焦点，请点击窗口后重试复制。");
}

export function formatTimestamp(value?: number) {
  return value ? new Date(value).toLocaleString("zh-CN") : "本地运行事件";
}

function statusLabel(status: PublisherLifecycle) {
  return ({ running: "运行中", paused: "已暂停", stopped: "已停止", failed: "失败", starting: "启动中", stopping: "停止中" } as Record<string, string>)[status] ?? status;
}
