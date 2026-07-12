import { ApiKeys, Bell, Branch, Code, History, Home, Plugin, Settings } from "@openai/apps-sdk-ui/components/Icon";
import type { ComponentType, SVGProps } from "react";

export type AppPage = "overview" | "direct" | "workflows" | "runs" | "notifications" | "templates" | "settings";
export type AppIcon = ComponentType<SVGProps<SVGSVGElement>>;

export const navigation: Array<{ id: AppPage; label: string; hint: string; icon: AppIcon; group: "global" | "support" }> = [
  { id: "overview", label: "API 钱包", hint: "API 的增删改查与上游验证", icon: ApiKeys, group: "global" },
  { id: "direct", label: "审计直出", hint: "从钱包创建本地审计端点", icon: Plugin, group: "global" },
  { id: "workflows", label: "编组模式", hint: "Project、Folder 与 Canvas", icon: Branch, group: "global" },
  { id: "runs", label: "运行记录", hint: "全局调用与审计", icon: History, group: "support" },
  { id: "notifications", label: "通知", hint: "聚合运行提醒", icon: Bell, group: "support" },
  { id: "templates", label: "调用模板", hint: "全部端点活文档", icon: Code, group: "support" },
  { id: "settings", label: "设置", hint: "外观与桌面偏好", icon: Settings, group: "support" },
];
