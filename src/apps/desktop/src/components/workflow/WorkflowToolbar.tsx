import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { ArrowRotateCcw, ArrowRotateCw, CheckCircle, Expand } from "@openai/apps-sdk-ui/components/Icon";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import type { WorkflowGraph } from "../../lib/desktop";
import { NodePalette } from "./NodePalette";

export type SaveState = "saved" | "dirty" | "saving" | "failed";

export function WorkflowToolbar({ saveState, canUndo, canRedo, onAdd, onUndo, onRedo, onLayout, onFit, onValidate, onSave }: { saveState: SaveState; canUndo: boolean; canRedo: boolean; onAdd: (kind: WorkflowGraph["nodes"][number]["kind"]) => void; onUndo: () => void; onRedo: () => void; onLayout: () => void; onFit: () => void; onValidate: () => void; onSave: () => void }) {
  const state = { saved: ["success", "已保存"], dirty: ["warning", "未保存"], saving: ["info", "保存中"], failed: ["danger", "保存失败"] }[saveState] as ["success" | "warning" | "info" | "danger", string];
  return <div className="workflow-toolbar"><div className="workflow-toolbar-group"><NodePalette onAdd={onAdd} /><Tooltip content="撤销"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="撤销" disabled={!canUndo} onClick={onUndo}><ArrowRotateCcw /></Button></Tooltip><Tooltip content="重做"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="重做" disabled={!canRedo} onClick={onRedo}><ArrowRotateCw /></Button></Tooltip></div><div className="workflow-toolbar-group"><Button color="secondary" variant="ghost" size="sm" onClick={onLayout}>自动布局</Button><Tooltip content="适应画布"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="适应画布" onClick={onFit}><Expand /></Button></Tooltip><Button color="secondary" variant="soft" size="sm" onClick={onValidate}><CheckCircle />校验</Button><Button color="primary" size="sm" loading={saveState === "saving"} onClick={onSave}><CheckCircle />保存</Button><Badge color={state[0]} variant="soft">{state[1]}</Badge></div></div>;
}
