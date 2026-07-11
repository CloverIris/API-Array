import { ApiKeys, Bell, Branch, Code, History, Home, Plugin, Settings } from "@openai/apps-sdk-ui/components/Icon";
import type { ComponentType, SVGProps } from "react";

export type AppPage = "overview" | "assets" | "workflows" | "publishers" | "runs" | "notifications" | "templates" | "settings";
export type AppIcon = ComponentType<SVGProps<SVGSVGElement>>;

export const navigation: Array<{ id: AppPage; label: string; hint: string; icon: AppIcon; group: "global" | "support" }> = [
  { id: "overview", label: "运行总览", hint: "所有 Canvas 与本地端点", icon: Home, group: "global" },
  { id: "assets", label: "API 钱包", hint: "共享 Provider 与 Secret", icon: ApiKeys, group: "global" },
  { id: "workflows", label: "综合器", hint: "所有 Canvas 的编排摘要", icon: Branch, group: "global" },
  { id: "publishers", label: "Publisher", hint: "全部本地发布实例", icon: Plugin, group: "global" },
  { id: "runs", label: "运行记录", hint: "全局调用与审计", icon: History, group: "support" },
  { id: "notifications", label: "通知", hint: "聚合运行提醒", icon: Bell, group: "support" },
  { id: "templates", label: "调用模板", hint: "全部端点活文档", icon: Code, group: "support" },
  { id: "settings", label: "设置", hint: "外观与桌面偏好", icon: Settings, group: "support" },
];
