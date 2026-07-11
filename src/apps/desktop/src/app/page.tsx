"use client";

import { AppsSDKUIProvider } from "@openai/apps-sdk-ui/components/AppsSDKUIProvider";
import { useCallback, useEffect, useState } from "react";

import {
  changePublisherState,
  getDesktopSnapshot,
  initializeWorkspace,
  type DesktopSnapshot,
  type PublisherSnapshot,
} from "../lib/desktop";

type Page = "overview" | "assets" | "workflows" | "publishers" | "runs" | "notifications" | "templates" | "settings";

const navigation: Array<{ id: Page; label: string; hint: string }> = [
  { id: "overview", label: "概览", hint: "工作区健康与运行概况" },
  { id: "assets", label: "API 资产", hint: "Provider 接入将在下一增量开放" },
  { id: "workflows", label: "工作流", hint: "节点画布将在下一增量开放" },
  { id: "publishers", label: "Publisher", hint: "本机统一端点的生命周期" },
  { id: "runs", label: "运行记录", hint: "调用审计将在 Provider 接入后出现" },
  { id: "notifications", label: "通知", hint: "静默汇总的运行提醒" },
  { id: "templates", label: "模板", hint: "可复制调用代码将在发布器可用后出现" },
  { id: "settings", label: "设置", hint: "桌面偏好与开机启动" },
];

export default function Home() {
  return (
    <AppsSDKUIProvider linkComponent="a">
      <DesktopApp />
    </AppsSDKUIProvider>
  );
}

function DesktopApp() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot | null>(null);
  const [page, setPage] = useState<Page>("overview");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      setSnapshot(await getDesktopSnapshot());
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (loading) {
    return <div className="boot-screen">正在打开本地工作区…</div>;
  }

  if (!snapshot?.initialized) {
    return <Onboarding initialError={error ?? snapshot?.startupError ?? null} onCreated={setSnapshot} />;
  }

  return (
    <Console
      error={error}
      onRefresh={refresh}
      onSnapshot={setSnapshot}
      page={page}
      setPage={setPage}
      snapshot={snapshot}
    />
  );
}

function Onboarding({ initialError, onCreated }: { initialError: string | null; onCreated: (snapshot: DesktopSnapshot) => void }) {
  const [name, setName] = useState("我的 API ARRAY 工作区");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      onCreated(await initializeWorkspace(name));
    } catch (reason) {
      setError(readError(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="oobe">
      <WindowTitlebar title="欢迎使用 API ARRAY" />
      <section className="oobe-card">
        <p className="eyebrow">本地 API 控制台</p>
        <h1>把分散的 API，整理成可靠的本机端点。</h1>
        <p className="lead">先创建一个空工作区。Provider、密钥和发布器不会被自动伪造或上传。</p>
        <div className="path-grid">
          <Path number="01" title="接入 API" text="下一桌面增量提供 Provider YAML 与安全密钥录入。" />
          <Path number="02" title="统一与路由" text="工作流、故障切换和适配器画布将在后续提供。" />
          <Path number="03" title="本地发布" text="已有 Publisher 可在此统一启动、暂停与审计。" />
        </div>
        <label className="field-label" htmlFor="workspace-name">工作区名称</label>
        <input id="workspace-name" value={name} maxLength={80} onChange={(event) => setName(event.target.value)} />
        {error ? <p className="error-message">{error}</p> : null}
        <button className="button primary" disabled={busy} onClick={() => void create()}>{busy ? "正在创建…" : "创建本地工作区"}</button>
      </section>
    </main>
  );
}

function Console({ snapshot, page, setPage, onSnapshot, onRefresh, error }: {
  snapshot: DesktopSnapshot;
  page: Page;
  setPage: (page: Page) => void;
  onSnapshot: (snapshot: DesktopSnapshot) => void;
  onRefresh: () => Promise<void>;
  error: string | null;
}) {
  const control = snapshot.control!;
  const selected = navigation.find((item) => item.id === page)!;
  const publishers = control.supervisor.publishers ?? [];
  const notifications = control.notifications ?? [];
  const missingSecrets = control.missingSecretCount;

  return (
    <main className="desktop-shell">
      <WindowTitlebar title={control.workspaceName} />
      <div className="workspace-toolbar">
        <div><p className="eyebrow">API ARRAY / {selected.label}</p><h1>{selected.label}</h1></div>
        <button className="button quiet" onClick={() => void onRefresh()}>刷新状态</button>
      </div>
      <div className="three-columns">
        <nav className="sidebar" aria-label="主导航">
          {navigation.map((item) => <button key={item.id} className={`nav-item ${page === item.id ? "selected" : ""}`} onClick={() => setPage(item.id)}><span>{item.label}</span><small>{item.hint}</small></button>)}
        </nav>
        <section className="content-panel">
          {error ? <p className="error-message">{error}</p> : null}
          {page === "overview" ? <Overview control={control} publishers={publishers} missingSecrets={missingSecrets} /> : null}
          {page === "publishers" ? <Publishers publishers={publishers} onSnapshot={onSnapshot} /> : null}
          {page === "notifications" ? <Notifications notifications={notifications} /> : null}
          {page === "settings" ? <Settings /> : null}
          {!["overview", "publishers", "notifications", "settings"].includes(page) ? <EmptyPage page={selected.label} hint={selected.hint} /> : null}
        </section>
        <aside className="inspector">
          <p className="eyebrow">当前状态</p>
          <h2>{control.workspaceName}</h2>
          <StateLine label="工作区" value={control.workspaceId} />
          <StateLine label="恢复来源" value={control.recoveredFromBackup ? "备份恢复" : "主工作区"} />
          <StateLine label="缺失 Secret" value={`${missingSecrets} 项`} />
          <StateLine label="运行 Publisher" value={`${publishers.filter((item) => item.status === "running").length} 个`} />
          <div className="next-step"><strong>下一步</strong><p>本轮只建立桌面壳层。请等待 Provider 配置与 Publisher 创建向导接入后再录入 API 密钥。</p></div>
        </aside>
      </div>
      <footer className="statusbar"><span>Runtime 已连接</span><span>{publishers.filter((item) => item.status === "running").length} 个 Publisher 运行中</span><span>{notifications.at(-1)?.message ?? "暂无新的运行通知"}</span></footer>
    </main>
  );
}

function Overview({ control, publishers, missingSecrets }: { control: NonNullable<DesktopSnapshot["control"]>; publishers: PublisherSnapshot[]; missingSecrets: number }) {
  return <><p className="lead">本地 ControlPlane 的只读快照。未配置 Provider 前，不会产生任何上游请求。</p><div className="metric-grid"><Metric label="API 资产" value={`${control.requiredSecretCount}`} detail="已登记的 Secret 状态" /><Metric label="Publisher" value={`${publishers.length}`} detail="可由桌面端管理" /><Metric label="缺失 Secret" value={`${missingSecrets}`} detail="密钥值永不发送到前端" /><Metric label="通知" value={`${control.notifications.length}`} detail="静默聚合的事件" /></div><EmptyPage page="准备开始" hint="工作区已创建。下一增量将支持通过 Provider YAML 录入 API，再创建统一发布器。" /></>;
}

function Publishers({ publishers, onSnapshot }: { publishers: PublisherSnapshot[]; onSnapshot: (snapshot: DesktopSnapshot) => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const act = async (action: "start" | "pause" | "stop", id: string) => { setBusy(`${action}:${id}`); try { onSnapshot(await changePublisherState(action, id)); } finally { setBusy(null); } };
  if (!publishers.length) return <EmptyPage page="尚无 Publisher" hint="当前空工作区没有暴露任何本机端点。Publisher 创建向导属于下一桌面增量。" />;
  return <div className="publisher-list">{publishers.map((publisher) => <article className="publisher-card" key={publisher.id}><div><h2>{publisher.id}</h2><p><span className={`status-dot ${publisher.status}`} />{publisher.status}{publisher.message ? ` · ${publisher.message}` : ""}</p></div><div className="actions"><button className="button quiet" disabled={busy !== null} onClick={() => void act("start", publisher.id)}>启动</button><button className="button quiet" disabled={busy !== null} onClick={() => void act("pause", publisher.id)}>暂停</button><button className="button danger" disabled={busy !== null} onClick={() => void act("stop", publisher.id)}>停止</button></div></article>)}</div>;
}

function Notifications({ notifications }: { notifications: NonNullable<DesktopSnapshot["control"]>["notifications"] }) {
  if (!notifications.length) return <EmptyPage page="暂无通知" hint="通知将合并重复错误，避免在系统通知栏循环刷屏。" />;
  return <div className="notification-list">{notifications.map((notice, index) => <article key={notice.id ?? index}><strong>{notice.message}</strong><small>{notice.createdAt ?? "本地运行事件"}</small></article>)}</div>;
}

function Settings() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { void import("@tauri-apps/plugin-autostart").then(({ isEnabled }) => isEnabled().then(setEnabled)).catch(() => setEnabled(false)); }, []);
  const toggle = async () => { setBusy(true); try { const plugin = await import("@tauri-apps/plugin-autostart"); if (enabled) await plugin.disable(); else await plugin.enable(); setEnabled(!enabled); } finally { setBusy(false); } };
  return <section className="setting-card"><div><h2>开机启动</h2><p>仅保存为此设备的桌面偏好，不写入工作区配置。</p></div><button className="button quiet" disabled={busy || enabled === null} onClick={() => void toggle()}>{enabled ? "已开启" : "已关闭"}</button></section>;
}

function EmptyPage({ page, hint }: { page: string; hint: string }) { return <div className="empty-state"><h2>{page}</h2><p>{hint}</p></div>; }
function Metric({ label, value, detail }: { label: string; value: string; detail: string }) { return <article className="metric"><p>{label}</p><strong>{value}</strong><small>{detail}</small></article>; }
function StateLine({ label, value }: { label: string; value: string }) { return <div className="state-line"><span>{label}</span><strong>{value}</strong></div>; }
function Path({ number, title, text }: { number: string; title: string; text: string }) { return <article className="path"><span>{number}</span><h2>{title}</h2><p>{text}</p></article>; }

function WindowTitlebar({ title }: { title: string }) {
  const callWindow = async (action: "minimize" | "toggleMaximize" | "close") => { const { getCurrentWindow } = await import("@tauri-apps/api/window"); await getCurrentWindow()[action](); };
  return <header className="titlebar" data-tauri-drag-region><span data-tauri-drag-region>API ARRAY</span><span className="window-title" data-tauri-drag-region>{title}</span><div className="window-controls"><button aria-label="最小化" onClick={() => void callWindow("minimize")}>—</button><button aria-label="最大化或恢复" onClick={() => void callWindow("toggleMaximize")}>□</button><button aria-label="关闭并隐藏到托盘" onClick={() => void callWindow("close")}>×</button></div></header>;
}

function readError(reason: unknown) { return typeof reason === "string" ? reason : "桌面服务暂时不可用，请检查运行日志。"; }
