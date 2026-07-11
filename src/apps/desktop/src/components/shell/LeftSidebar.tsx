import { Button } from "@openai/apps-sdk-ui/components/Button";
import { SidebarCollapseLeft, SidebarOpenLeft } from "@openai/apps-sdk-ui/components/Icon";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import { navigation, type AppPage } from "../navigation";

const groups = [
  { id: "workspace", label: "工作区" },
  { id: "operations", label: "运行与审计" },
  { id: "settings", label: "" },
] as const;

export function LeftSidebar({ page, collapsed, onNavigate, onToggle }: { page: AppPage; collapsed: boolean; onNavigate: (page: AppPage) => void; onToggle: () => void }) {
  return <nav className={`sidebar ${collapsed ? "collapsed" : ""}`} aria-label="主导航"><div className="sidebar-heading"><span>{collapsed ? "" : "导航"}</span><Tooltip content={collapsed ? "展开导航" : "折叠导航"}><Button color="secondary" variant="ghost" size="sm" uniform aria-label={collapsed ? "展开导航" : "折叠导航"} onClick={onToggle}>{collapsed ? <SidebarOpenLeft /> : <SidebarCollapseLeft />}</Button></Tooltip></div>{groups.map((group) => <section className="nav-group" key={group.id}>{!collapsed && group.label ? <p>{group.label}</p> : null}{navigation.filter((item) => item.group === group.id).map((item) => { const Icon = item.icon; const button = <button className={`nav-item ${page === item.id ? "selected" : ""}`} aria-label={collapsed ? `${item.label}：${item.hint}` : undefined} aria-current={page === item.id ? "page" : undefined} onClick={() => onNavigate(item.id)}><Icon className="size-4" aria-hidden="true" />{collapsed ? null : <span>{item.label}</span>}</button>; return collapsed ? <Tooltip key={item.id} content={`${item.label} · ${item.hint}`} side="right">{button}</Tooltip> : <span key={item.id}>{button}</span>; })}</section>)}</nav>;
}
