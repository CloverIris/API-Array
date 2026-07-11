import { Handle, Position } from "@xyflow/react";
import type { PortType } from "./graphModel";

const labels: Record<string, string> = { request: "请求", response: "响应", capability: "能力", health: "健康", control: "控制", error: "错误" };

export function TypedHandle({ id, dataType, direction, index, count }: { id: string; dataType: PortType; direction: "input" | "output"; index: number; count: number }) {
  const top = `${((index + 1) / (count + 1)) * 100}%`;
  return <div className={`typed-port ${direction} port-${dataType}`} style={{ top }}><Handle id={id} type={direction === "input" ? "target" : "source"} position={direction === "input" ? Position.Left : Position.Right} aria-label={`${labels[dataType] ?? dataType} ${direction === "input" ? "输入" : "输出"}端口 ${id}`} /><span>{labels[dataType] ?? dataType}</span></div>;
}
