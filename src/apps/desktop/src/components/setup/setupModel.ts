import type { DesktopSnapshot, ProviderInstanceItem, WorkspaceIntent } from "../../lib/desktop";

export type SetupStepId = "provider" | "inspection" | "publisher" | "call";
export type SetupStep = { id: SetupStepId; title: string; description: string; complete: boolean; action: string };
export type SetupProgress = { current: SetupStepId; completed: number; steps: SetupStep[] };

export function buildSetupProgress(control: NonNullable<DesktopSnapshot["control"]>, providers: ProviderInstanceItem[]): SetupProgress {
  const hasProvider = control.providerCount > 0;
  const inspected = providers.some((provider) => provider.inspectionAvailable);
  const hasPublisher = control.publisherCount > 0;
  const steps: SetupStep[] = [
    { id: "provider", title: "接入 Provider", description: "选择供应商并将密钥写入 Windows 凭据库。", complete: hasProvider && control.missingSecretCount === 0, action: "添加 API" },
    { id: "inspection", title: "执行安全体检", description: "验证鉴权、网络、模型发现和协议兼容性。", complete: inspected, action: "开始体检" },
    { id: "publisher", title: "创建本地端点", description: "将健康线路发布到仅本机可访问的地址。", complete: hasPublisher, action: "创建 Publisher" },
    { id: "call", title: "验证并复制调用", description: "检查 /v1/models，并获取多语言调用模板。", complete: hasPublisher, action: "查看调用方式" },
  ];
  return { current: steps.find((step) => !step.complete)?.id ?? "call", completed: steps.filter((step) => step.complete).length, steps };
}

export function intentCopy(intent: WorkspaceIntent) {
  if (intent === "unified_endpoint") return { title: "创建一个可靠的统一 AI 端点", detail: "先接入供应商，再完成体检与本地发布。" };
  if (intent === "reliability") return { title: "让关键 API 拥有可解释的备用线路", detail: "先建立第一条健康线路，随后配置回退策略。" };
  if (intent === "import") return { title: "检查导入的工作区", detail: "确认 Secret 绑定、Provider 健康和 Publisher 状态。" };
  return { title: "整理并验证你的 API 资产", detail: "从第一个 Provider 开始，逐步完成可调用的本地端点。" };
}
