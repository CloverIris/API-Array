import { Bell, CheckCircle, Home } from "@openai/apps-sdk-ui/components/Icon";
import type { DesktopSnapshot } from "../../lib/desktop";

export function RuntimeStatusBar({ control, runningCount }: { control: NonNullable<DesktopSnapshot["control"]>; runningCount: number }) {
  return <footer className="statusbar"><span><CheckCircle className="size-3.5" />Runtime 已连接</span><span><Home className="size-3.5" />{runningCount} 个实例运行中</span><span><Bell className="size-3.5" />{control.notifications.at(-1)?.message ?? "暂无新的运行通知"}</span></footer>;
}
