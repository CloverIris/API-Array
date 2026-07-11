import { Bell, CheckCircle, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import type { DesktopSnapshot } from "../../lib/desktop";

export function RuntimeStatusBar({ control }: { control: NonNullable<DesktopSnapshot["control"]> }) {
  const publishers = control.supervisor.publishers ?? [];
  const running = publishers.filter((item) => item.status === "running").length;
  return <footer className="statusbar"><span><CheckCircle className="size-3.5" />Runtime 已连接</span><span><Plugin className="size-3.5" />{running} 个 Publisher 运行中</span><span><Bell className="size-3.5" />{control.notifications.at(-1)?.message ?? "暂无新的运行通知"}</span></footer>;
}
