import { Handle, Position } from "@xyflow/react";

const labels: Record<string, string> = { candidate: "候选", service_plan: "服务计划", health_signal: "健康信号" };
export function TypedHandle({ id, dataType, direction, index, count }: { id: string; dataType: string; direction: "input" | "output"; index: number; count: number }) {
  const top = `${((index + 1) / (count + 1)) * 100}%`;
  return <div className={`typed-port ${direction} port-${dataType}`} style={{ top }}><Handle id={id} type={direction === "input" ? "target" : "source"} position={direction === "input" ? Position.Left : Position.Right} aria-label={`${labels[dataType] ?? dataType} ${direction === "input" ? "输入" : "输出"}端口 ${id}`} /><span>{labels[dataType] ?? dataType}</span></div>;
}
