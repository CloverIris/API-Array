"use client";

import { Component, type ErrorInfo, type ReactNode } from "react";
import { copyText } from "./shared";

type Props = { children: ReactNode };
type State = { error: Error | null; componentStack: string };
export const REACT_310_RECOVERY_KEY = "apiarray.react-310-recovery";
export const REACT_310_MAX_RECOVERIES = 3;

export function isRecoverableReactCacheError(message: string) {
  return message.includes("Minified React error #310") || message.includes("Rendered more hooks than during the previous render");
}

export function nextReactRecoveryAttempt(message: string, stored: string | null) {
  if (!isRecoverableReactCacheError(message)) return null;
  const parsed = Number.parseInt(stored ?? "0", 10);
  const current = Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
  return current < REACT_310_MAX_RECOVERIES ? current + 1 : null;
}

/**
 * Production React errors are minified. Keep the component stack locally so a
 * Preview tester can provide actionable diagnostics without exposing data from
 * the workspace, requests, headers, Keys, or Tokens.
 */
export class ClientErrorBoundary extends Component<Props, State> {
  state: State = { error: null, componentStack: "" };

  static getDerivedStateFromError(error: Error): State {
    return { error, componentStack: "" };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const recoveryAttempt = nextReactRecoveryAttempt(error.message, sessionStorage.getItem(REACT_310_RECOVERY_KEY));
    if (recoveryAttempt !== null) {
      sessionStorage.setItem(REACT_310_RECOVERY_KEY, String(recoveryAttempt));
      window.location.reload();
      return;
    }
    this.setState({ componentStack: info.componentStack ?? "" });
    console.error("API ARRAY client render boundary", error, info.componentStack);
  }

  render() {
    const { error, componentStack } = this.state;
    if (!error) return this.props.children;

    const summary = [
      error.message || "界面渲染发生未知错误。",
      componentStack ? `\n组件栈：${componentStack.trim()}` : "\n组件栈暂未提供。",
    ].join("\n");

    return (
      <main className="client-render-fallback">
        <section role="alert" className="client-render-fallback-card">
          <p className="eyebrow">RENDER DIAGNOSTICS</p>
          <h1>界面无法继续显示</h1>
          <p>该错误没有包含 API Key、Token、请求正文或本地审计内容。</p>
          <pre>{error.message}</pre>
          {componentStack ? <details><summary>查看组件诊断</summary><pre>{componentStack.trim()}</pre></details> : null}
          <div className="client-render-fallback-actions">
            <button type="button" onClick={() => window.location.reload()}>重新加载界面</button>
            <button type="button" onClick={() => void copyText(summary)}>复制诊断摘要</button>
          </div>
        </section>
      </main>
    );
  }
}
