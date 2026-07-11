import { Button } from "@openai/apps-sdk-ui/components/Button";
import { CloseBold, Expand, MinimizeDown } from "@openai/apps-sdk-ui/components/Icon";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";

export function WindowTitleBar({ title }: { title: string }) {
  const callWindow = async (action: "minimize" | "toggleMaximize" | "close") => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[action]();
  };
  const control = (label: string, action: "minimize" | "toggleMaximize" | "close", icon: React.ReactNode, danger = false) => (
    <Tooltip content={label} side="bottom">
      <Button color={danger ? "danger" : "secondary"} variant="ghost" size="sm" uniform pill={false} aria-label={label} onClick={() => void callWindow(action)}>{icon}</Button>
    </Tooltip>
  );
  return <header className="titlebar" data-tauri-drag-region><span className="window-brand" data-tauri-drag-region>API ARRAY</span><span className="window-title" data-tauri-drag-region>{title}</span><div className="window-controls">{control("最小化", "minimize", <MinimizeDown />)}{control("最大化或恢复", "toggleMaximize", <Expand />)}{control("关闭并隐藏到托盘", "close", <CloseBold />, true)}</div></header>;
}
