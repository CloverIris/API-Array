# API ARRAY Rust Core 对 PRD 覆盖审计 V0.2

审计日期：2026-07-11  
审计范围：冻结 PRD 中属于 Rust Core、Control Plane 和 Runtime 的要求。Tauri、Windows 安装器、托盘和正式 UI 不计入 Core 完成度。

## 结论

当前代码已经形成可独立运行的数据面原型，完成了 Provider 配置、三类协议 Adapter、统一请求模型、本地 OpenAI-compatible Publisher、SSE、健康路由、重试与故障切换、脱敏审计、关联 ID 和八语言模板。

本轮补齐了此前阻塞后续宿主开发的三个核心边界：版本化体检报告、安全工作区传播、审计用量与切换摘要；同时增加简单元数据条件路由和节点影响分析。

按阶段判断：

- V0.2 Rust Core 可行性原型：基本完成，可进入 Tauri 宿主与 Control Plane 生命周期原型。
- PRD 完整 MVP：尚未完成，主要剩余工作在 Windows 凭据保险库、工作区持久化恢复、Runtime Supervisor、通知接线、图到 Runtime 的完整编译，以及 Tauri/UI/安装发布。

## 覆盖矩阵

| PRD 能力 | 状态 | 当前证据 | 后续缺口 |
|---|---|---|---|
| Provider YAML 与严格校验 | 已实现 | 四个内置 Provider；Schema、安全探测约束与交叉引用测试 | Provider 版本迁移、社区签名不属于当前阶段 |
| OpenAI、Anthropic、Gemini Adapter | 已实现 | 请求、响应、错误和 SSE 统一转换 | 更多真实供应商偏差需要持续联调 |
| Secret 引用与不泄漏 | 核心已实现 | `secret://`、零化 Secret、环境解析限制、日志与导出扫描 | Windows Credential Manager 尚未接入 |
| 能力探测与体检报告 | 核心模型已实现 | 探测授权、频率限制、可信度、声明值/实测值分离、体检聚合 | 后台调度器和 Provider 探测结果自动回写待 Control Plane |
| 节点图 | 部分实现 | 七类节点、强类型端口、无环校验、拓扑排序、下游影响分析 | Transform/Guard/Group 的完整执行与图到 Runtime 编译尚未完成 |
| 路由策略 | 已实现 | 主备优先级、健康选择、超时、有限重试、错误切换、模型映射、元数据简单条件 | 负载均衡、预算 Guard 和稳定回切策略待后续 |
| OpenAI-compatible Publisher | 已实现 | 回环绑定、独立 Token、流式/非流式、请求限制、优雅停止 | 暂停/恢复和多 Publisher Supervisor 尚未完成 |
| Capability Manifest | 部分实现 | 版本化模型、证据、合并和健康状态 | 尚未从工作流自动计算完整 Publisher Manifest/OpenAPI |
| 活文档与模板 | 首批要求已实现 | cURL、Python、JS/TS、Go、Rust、Java、C#、C++ | SDK 专用模板、Manifest 参数裁剪和 UI 测试按钮待后续 |
| 审计 | 核心已实现 | 非阻塞 JSONL、关联 ID、线路、错误、延迟、首字节（流式）、用量、缓存、重试与切换计数 | 保留期限、清除和诊断包待 Control Plane |
| 通知聚合 | 部分实现 | 状态事件和相同事件合并模型 | Runtime 健康变化尚未接入通知中心与 Windows 通知 |
| 工作区导入导出 | 核心格式已实现 | 运行语义/UI 分离、确定性 JSON、明文凭据拒绝、未知版本只读、缺失 Secret/Publisher 就绪状态 | 原子落盘、恢复副本和真实旧版本迁移尚未完成 |
| 独立 Runtime | 已实现 | 独立二进制、stdin 启动、UI 无依赖、本地 Publisher | 开机恢复、崩溃拉起和升级交接待 Supervisor/Tauri |
| 公网发布禁止 | 已实现 | Publisher 强制回环地址 | 高级局域网/公网模块明确留后 |

## 本轮之后仍不应伪装为“已完成”的内容

1. `.env` 是联调 Secret 来源，不是 PRD 要求的 Windows 系统凭据保险库。
2. `WorkflowGraph` 已能校验和解释影响，但当前 `RuntimeConfig` 仍是直接编译输入，不等于七类节点均已具备执行器。
3. 真实 API Probe 已可受控运行，但还没有常驻调度、暂停恢复和完整体检历史库。
4. Publisher 进程可以独立运行和优雅停止，但还没有桌面级 Supervisor 自动恢复多个 Publisher。
5. 通知数据模型存在，但切换事件尚未持久化并接入通知中心。

## 建议的下一阶段入口

Rust Core 已达到创建 Tauri 宿主原型的前置条件。下一轮应先实现不含正式 UI 的 Windows Control Plane/Supervisor：系统凭据保险库、工作区原子持久化、Runtime 子进程生命周期与状态查询。完成后再承载 OOBE 和三栏界面。
