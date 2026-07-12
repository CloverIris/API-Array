import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { ApiKeys, Branch, Plugin } from "@openai/apps-sdk-ui/components/Icon";
import type { DesktopSnapshot, WorkspaceIntent } from "../../lib/desktop";
import type { AppPage } from "../navigation";

export function GuidedHome({ control, onNavigate }: { control: NonNullable<DesktopSnapshot["control"]>; intent: WorkspaceIntent; onNavigate: (page: AppPage) => void }) {
  return <div className="guided-home"><section className="hero-panel"><div><Badge color="info" variant="soft">三条清晰路径</Badge><h1>{control.workspaceName}</h1><p>先把 API 放进钱包，再选择审计直出或 Canvas 编组。</p></div><Button color="primary" onClick={() => onNavigate("overview")}><ApiKeys />打开 API 钱包</Button></section><div className="dashboard-grid"><button className="dashboard-card" onClick={() => onNavigate("overview")}><ApiKeys /><span>API 钱包</span><strong>{control.providerCount}</strong><small>管理上游 API 与 Secret</small></button><button className="dashboard-card" onClick={() => onNavigate("direct")}><Plugin /><span>审计直出</span><strong>Local</strong><small>创建独立本地审计端点</small></button><button className="dashboard-card" onClick={() => onNavigate("workflows")}><Branch /><span>编组模式</span><strong>Canvas</strong><small>组合多个钱包资产</small></button></div></div>;
}
