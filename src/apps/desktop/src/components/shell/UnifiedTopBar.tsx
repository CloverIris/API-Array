"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Menu } from "@openai/apps-sdk-ui/components/Menu";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import {
  ApiKeys,
  ArrowRotateCcw,
  Branch,
  CloseBold,
  ColorTheme,
  Expand,
  MinimizeDown,
  Moon,
  MoonSunSystem,
  Plugin,
  Search,
  SidebarOpenRight,
  Sun,
} from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import type {
  ControlCenterSnapshot,
  ProjectTree,
  ThemePreference,
  WalletCard,
} from "../../lib/desktop";
import type { AppIcon, AppPage } from "../navigation";

type Props = {
  workspaceName: string;
  pageLabel: string;
  pageIcon: AppIcon;
  runningCount: number;
  wallet: WalletCard[];
  controlCenter: ControlCenterSnapshot | null;
  projectTree: ProjectTree;
  theme: ThemePreference;
  onTheme: (theme: ThemePreference) => void;
  onNavigate: (page: AppPage) => void;
  onOpenCanvas: (projectId: string, canvasId: string) => void;
  onRefresh: () => void;
  onOpenInspector: () => void;
};

export function UnifiedTopBar({
  workspaceName,
  pageLabel,
  pageIcon: PageIcon,
  runningCount,
  wallet,
  controlCenter,
  projectTree,
  theme,
  onTheme,
  onNavigate,
  onOpenCanvas,
  onRefresh,
  onOpenInspector,
}: Props) {
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const normalized = query.trim().toLocaleLowerCase();
  const results = useMemo(() => {
    if (!normalized) return [];
    const walletResults = wallet
      .filter((item) => `${item.name} ${item.providerId}`.toLocaleLowerCase().includes(normalized))
      .map((item) => ({ id: `wallet:${item.id}`, label: item.name, detail: "API 钱包", icon: ApiKeys, action: () => onNavigate("wallet") }));
    const instanceResults = (controlCenter?.instances ?? [])
      .filter((item) => `${item.name} ${item.ownership} ${item.assetNames.join(" ")} ${item.publicModels.join(" ")}`.toLocaleLowerCase().includes(normalized))
      .map((item) => ({
        id: item.id,
        label: item.name,
        detail: item.kind === "canvas" ? item.ownership : "审计直出",
        icon: item.kind === "canvas" ? Branch : Plugin,
        action: () => item.kind === "canvas" && item.projectId && item.canvasId
          ? onOpenCanvas(item.projectId, item.canvasId)
          : onNavigate("direct"),
      }));
    const canvasResults = Object.values(projectTree.projects).flatMap((project) =>
      Object.values(project.canvases)
        .filter((canvas) => `${project.name} ${canvas.name}`.toLocaleLowerCase().includes(normalized))
        .map((canvas) => ({
          id: `canvas:${project.id}:${canvas.id}`,
          label: canvas.name,
          detail: project.name,
          icon: Branch,
          action: () => onOpenCanvas(project.id, canvas.id),
        })),
    );
    return [...walletResults, ...instanceResults, ...canvasResults]
      .filter((item, index, items) => items.findIndex((candidate) => candidate.id === item.id) === index)
      .slice(0, 8);
  }, [controlCenter?.instances, normalized, onNavigate, onOpenCanvas, projectTree.projects, wallet]);

  const callWindow = async (action: "minimize" | "toggleMaximize" | "close") => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[action]();
  };
  const choose = (action: () => void) => {
    action();
    setQuery("");
    setFocused(false);
  };

  return (
    <header className="unified-topbar">
      <div className="topbar-drag" data-tauri-drag-region>
        <strong className="topbar-brand" data-tauri-drag-region>API ARRAY</strong>
        <span className="topbar-divider" />
        <span className="topbar-workspace" data-tauri-drag-region>{workspaceName}</span>
        <span className="topbar-crumb"><PageIcon aria-hidden="true" />{pageLabel}</span>
        {runningCount ? <Badge color="success" variant="soft">{runningCount} 运行中</Badge> : null}
      </div>

      <div className="topbar-search" role="search">
        <Search aria-hidden="true" />
        <Input
          aria-label="搜索工作区"
          placeholder="搜索 API、实例或 Canvas"
          value={query}
          onFocus={() => setFocused(true)}
          onBlur={() => window.setTimeout(() => setFocused(false), 120)}
          onChange={(event) => setQuery(event.target.value)}
        />
        {focused && normalized ? (
          <div className="topbar-search-results" role="listbox" aria-label="搜索结果">
            {results.length ? results.map((result) => {
              const Icon = result.icon;
              return (
                <button
                  type="button"
                  role="option"
                  key={result.id}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => choose(result.action)}
                >
                  <Icon aria-hidden="true" />
                  <span><strong>{result.label}</strong><small>{result.detail}</small></span>
                </button>
              );
            }) : <p>没有匹配的 API、实例或 Canvas</p>}
          </div>
        ) : null}
      </div>

      <div className="topbar-actions">
        <Menu>
          <Menu.Trigger><Tooltip content="切换主题"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="切换主题"><ColorTheme /></Button></Tooltip></Menu.Trigger>
          <Menu.Content align="end" side="bottom" minWidth={160}>
            <Menu.RadioGroup value={theme} onChange={onTheme}>
              <Menu.RadioItem value="system"><MoonSunSystem />跟随系统</Menu.RadioItem>
              <Menu.RadioItem value="light"><Sun />亮色</Menu.RadioItem>
              <Menu.RadioItem value="dark"><Moon />暗色</Menu.RadioItem>
            </Menu.RadioGroup>
          </Menu.Content>
        </Menu>
        <Tooltip content="刷新状态"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="刷新状态" onClick={onRefresh}><ArrowRotateCcw /></Button></Tooltip>
        <Tooltip content="打开属性栏"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="打开属性栏" onClick={onOpenInspector}><SidebarOpenRight /></Button></Tooltip>
        <span className="window-controls">
          <Button color="secondary" variant="ghost" size="sm" uniform aria-label="最小化" onClick={() => void callWindow("minimize")}><MinimizeDown /></Button>
          <Button color="secondary" variant="ghost" size="sm" uniform aria-label="最大化或恢复" onClick={() => void callWindow("toggleMaximize")}><Expand /></Button>
          <Button color="danger" variant="ghost" size="sm" uniform aria-label="关闭到托盘" onClick={() => void callWindow("close")}><CloseBold /></Button>
        </span>
      </div>
    </header>
  );
}
