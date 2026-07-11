import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import type { WorkflowValidationResult } from "../../lib/desktop";

export function WorkflowValidationPanel({ result, localError }: { result: WorkflowValidationResult | null; localError: string | null }) {
  if (!result && !localError) return null;
  const valid = result?.valid && !localError;
  return <div className={`workflow-validation ${valid ? "valid" : "invalid"}`} role={valid ? "status" : "alert"}><Badge color={valid ? "success" : "danger"} variant="soft">{valid ? "图校验通过" : "需要修复"}</Badge>{localError ? <p>{localError}</p> : null}{result?.errors.map((error) => <p key={error}>{error}</p>)}{valid && result?.summary ? <p>{result.summary.node_count} 个节点、{result.summary.edge_count} 条连接，拓扑顺序有效。</p> : null}</div>;
}
