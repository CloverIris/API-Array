import { Button } from "@openai/apps-sdk-ui/components/Button";
import { CloseBold, Pin, PinFilled } from "@openai/apps-sdk-ui/components/Icon";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import type { KeyboardEvent, PointerEvent, ReactNode } from "react";

export function RightInspector({ open, pinned, width, title, children, onClose, onTogglePin, onWidthChange }: { open: boolean; pinned: boolean; width: number; title: string; children: ReactNode; onClose: () => void; onTogglePin: () => void; onWidthChange: (width: number) => void }) {
  if (!open) return null;
  const resizeFromPointer = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startWidth = width;
    const move = (moveEvent: globalThis.PointerEvent) => onWidthChange(Math.min(420, Math.max(296, startWidth + startX - moveEvent.clientX)));
    const stop = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", stop); };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  };
  const resizeFromKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    onWidthChange(Math.min(420, Math.max(296, width + (event.key === "ArrowLeft" ? 12 : -12))));
  };
  return <aside className={`inspector ${pinned ? "pinned" : "overlay"}`} style={{ width }}><div className="panel-resizer" role="separator" aria-label="调整属性栏宽度" aria-orientation="vertical" aria-valuemin={296} aria-valuemax={420} aria-valuenow={width} tabIndex={0} onPointerDown={resizeFromPointer} onKeyDown={resizeFromKeyboard} /><div className="inspector-heading"><div><p className="eyebrow">属性与状态</p><h2>{title}</h2></div><div className="inspector-actions"><Tooltip content={pinned ? "取消固定" : "固定属性栏"}><Button color="secondary" variant="ghost" size="sm" uniform aria-label={pinned ? "取消固定属性栏" : "固定属性栏"} onClick={onTogglePin}>{pinned ? <PinFilled /> : <Pin />}</Button></Tooltip><Tooltip content="关闭属性栏"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="关闭属性栏" onClick={onClose}><CloseBold /></Button></Tooltip></div></div>{children}</aside>;
}
