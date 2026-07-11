import {
  ApiKeys,
  Bell,
  Branch,
  Code,
  History,
  Home,
  Plugin,
  Settings,
} from "@openai/apps-sdk-ui/components/Icon";
import type { ComponentType, SVGProps } from "react";

export type AppPage =
  | "overview"
  | "assets"
  | "workflows"
  | "publishers"
  | "runs"
  | "notifications"
  | "templates"
  | "settings";

export type AppIcon = ComponentType<SVGProps<SVGSVGElement>>;

export const navigation: Array<{
  id: AppPage;
  label: string;
  hint: string;
  icon: AppIcon;
  group: "workspace" | "operations" | "settings";
}> = [
  { id: "overview", label: "首页", hint: "任务进度与运行概况", icon: Home, group: "workspace" },
  { id: "assets", label: "API 资产", hint: "Provider 与 Secret", icon: ApiKeys, group: "workspace" },
  { id: "workflows", label: "工作流", hint: "路由与故障切换", icon: Branch, group: "workspace" },
  { id: "publishers", label: "Publisher", hint: "本机端点生命周期", icon: Plugin, group: "workspace" },
  { id: "runs", label: "运行记录", hint: "调用与审计时间线", icon: History, group: "operations" },
  { id: "notifications", label: "通知", hint: "聚合的运行提醒", icon: Bell, group: "operations" },
  { id: "templates", label: "调用模板", hint: "活文档与代码", icon: Code, group: "operations" },
  { id: "settings", label: "设置", hint: "外观与桌面偏好", icon: Settings, group: "settings" },
];
