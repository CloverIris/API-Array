import { ApiKeys, Bell, Branch, Document, History, Home, Plugin, Settings } from "@openai/apps-sdk-ui/components/Icon";
import type { ComponentType, SVGProps } from "react";

export type AppPage = "home" | "wallet" | "direct" | "compositions" | "runs" | "notifications" | "templates" | "settings";
export type AppIcon = ComponentType<SVGProps<SVGSVGElement>>;

export const navigation: Array<{ id: AppPage; label: string; hint: string; icon: AppIcon; group: "home" | "global" | "support" }> = [
  { id: "home", label: "主页", hint: "工作区主控台", icon: Home, group: "home" },
  { id: "wallet", label: "API 钱包", hint: "API 的增删改查与上游验证", icon: ApiKeys, group: "global" },
  { id: "direct", label: "审计直出", hint: "从钱包创建本地审计端点", icon: Plugin, group: "global" },
  { id: "compositions", label: "编组模式", hint: "项目、文件夹与编组方案", icon: Branch, group: "global" },
  { id: "runs", label: "运行记录", hint: "全局调用与审计", icon: History, group: "support" },
  { id: "notifications", label: "通知", hint: "聚合运行提醒", icon: Bell, group: "support" },
  { id: "templates", label: "文档", hint: "全部本地端点的接口文档", icon: Document, group: "support" },
  { id: "settings", label: "设置", hint: "外观、存储与桌面偏好", icon: Settings, group: "support" },
];
