"use client";

import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { CloseBold } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useRef, useState } from "react";

export type DialogOptions = {
  title: string;
  description?: string;
  inputLabel?: string;
  defaultValue?: string;
  confirmLabel?: string;
  danger?: boolean;
};

type DialogRequest = { options: DialogOptions; resolve: (value: string | null) => void };
const DIALOG_EVENT = "apiarray:dialog";
let dialogQueue: DialogRequest[] = [];

export function askAppDialog(options: DialogOptions) {
  return new Promise<string | null>((resolve) => {
    dialogQueue.push({ options, resolve });
    window.dispatchEvent(new CustomEvent<DialogOptions>(DIALOG_EVENT, { detail: options }));
  });
}

export function AppDialogHost() {
  const [request, setRequest] = useState<DialogRequest | null>(null);
  const [value, setValue] = useState("");
  const cardRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const open = () => {
      setRequest((current) => {
        if (current || dialogQueue.length === 0) return current;
        const next = dialogQueue.shift() ?? null;
        if (next) setValue(next.options.defaultValue ?? "");
        return next;
      });
    };
    window.addEventListener(DIALOG_EVENT, open);
    open();
    return () => window.removeEventListener(DIALOG_EVENT, open);
  }, []);

  useEffect(() => {
    if (!request) return;
    cardRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); close(null); return; }
      if (event.key !== "Tab" || !cardRef.current) return;
      const focusable = Array.from(cardRef.current.querySelectorAll<HTMLElement>("button, input, [tabindex]:not([tabindex='-1'])"));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [request]);

  const close = (result: string | null) => {
    if (!request) return;
    request.resolve(result);
    const next = dialogQueue.shift() ?? null;
    setValue(next?.options.defaultValue ?? "");
    setRequest(next);
  };

  if (!request) return null;
  const options = request.options;
  return (
    <div className="app-dialog-layer" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(null); }}>
      <section ref={cardRef} className="app-dialog-card" role="dialog" tabIndex={-1} aria-modal="true" aria-labelledby="app-dialog-title" aria-describedby={options.description ? "app-dialog-description" : undefined}>
        <Button className="app-dialog-close" color="secondary" variant="ghost" uniform aria-label="关闭弹窗" onClick={() => close(null)}><CloseBold /></Button>
        <p className="eyebrow">API ARRAY</p>
        <h2 id="app-dialog-title">{options.title}</h2>
        {options.description ? <p id="app-dialog-description" className="app-dialog-description">{options.description}</p> : null}
        {options.inputLabel ? <label className="field-label">{options.inputLabel}<Input autoFocus value={value} onChange={(event) => setValue(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && value.trim()) close(value.trim()); }} /></label> : null}
        <div className="app-dialog-actions">
          <Button color="secondary" variant="ghost" onClick={() => close(null)}>取消</Button>
          <Button color={options.danger ? "danger" : "primary"} disabled={Boolean(options.inputLabel) && !value.trim()} onClick={() => close(options.inputLabel ? value.trim() : "confirmed")}>{options.confirmLabel ?? "确认"}</Button>
        </div>
      </section>
    </div>
  );
}
