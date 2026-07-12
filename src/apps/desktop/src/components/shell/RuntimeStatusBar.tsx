"use client";
import { Bell, CheckCircle, Grid } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useState } from "react";
import { getInstanceRackSnapshot, type DesktopSnapshot } from "../../lib/desktop";

export function RuntimeStatusBar({ control }: { control: NonNullable<DesktopSnapshot["control"]> }) {
  const [running, setRunning] = useState(0);
  useEffect(() => {
    const refresh = () => void getInstanceRackSnapshot().then((snapshot) => setRunning(snapshot.runningCount)).catch(() => undefined);
    refresh();
    let dispose: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => listen("desktop:instances-changed", refresh)).then((next) => { dispose = next; }).catch(() => undefined);
    return () => dispose?.();
  }, []);
  return <footer className="statusbar"><span><CheckCircle className="size-3.5" />Runtime 已连接</span><span><Grid className="size-3.5" />{running} 个实例运行中</span><span><Bell className="size-3.5" />{control.notifications.at(-1)?.message ?? "暂无新的运行通知"}</span></footer>;
}
