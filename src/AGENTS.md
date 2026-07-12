# API ARRAY 开发约定

## V0.7 全局实例机架强制约束

开始运行控制、导航、网关或实例状态相关工作前，必须阅读 `../docs/API_ARRAY_产品设计文档_V0.7_全局实例机架.md`。

- 实例机架是 Direct Endpoint 与 Canvas Publisher 的只读运行投影，不是新的配置事实源。
- `ManagedInstance` 不得持久化为第三份业务实体。
- 运行中状态必须由 LocalGateway 实际挂载与运行意图共同确认。
- 批量启停必须由 Rust 一次预检、一次保存和一次网关刷新完成，禁止前端循环调用单项命令。
- Canvas 实例命令必须保留显式 `projectId + canvasId` 归属校验。
- 全部停止只关闭业务入口，不删除配置、钱包资产、Secret 或工作区数据。

## V0.6 SQLite 存储与活文档强制约束

开始任何 Workspace、持久化、审计、体检、Secret 查看或活文档工作前，必须阅读 `../docs/API_ARRAY_产品设计文档_V0.6_SQLite存储与活文档.md`。

- Runtime `StorageService` 是唯一持久化语义边界；Core、Tauri 命令和前端不得直接编写 SQL。
- 正式工作区为 `workspace.sqlite3` 目录包；JSON 仅允许作为版本化 payload 或显式脱敏交换格式。
- 启动失败不得自动删除工作区、审计、报告或 Windows 凭据。
- Secret 明文只存在于 Windows Credential Manager 和受控短时内存中。
- 活文档必须由 Core 的结构化 `LiveDocument` 生成，禁止前端维护静态 Provider 代码说明。

## V0.5 Canonical 编组图强制约束

开始任何 Canvas、Graph、Publisher 或 Runtime 路由工作前，必须阅读 `../docs/API_ARRAY_产品设计文档_V0.5_Canonical编组图与协议综合器.md`。

- Graph 是服务计划编译器，不是 HTTP Request/Response 数据流引擎。
- 运行端口只允许 Candidate、ServicePlan、HealthSignal。
- Provider 协议转换由 Rust Adapter 层自动完成；Composer 是多 Provider 唯一汇聚点。
- 每个 Canvas 恰好一个 Composer 和一个 Publisher；Group 不参与运行边。
- Workspace V6 与 Graph V2 不兼容旧数据，不得新增 V5 或旧端口迁移逻辑。

> **V0.2 钱包与项目画布约束**：开始桌面、Workspace 或工作流相关工作前，必须阅读
> `docs/API_ARRAY_产品设计文档_V0.2_钱包与项目画布变更.md`。API 钱包是工作区级资产；
> 每个新 Canvas 只有一个总输出器；未确认的预放置绝不持久化；Secret 与真实/推测价格
> 不得进入工作区、UI 状态、日志、错误或导出文件。

本文件适用于 `src/` 下的全部源代码、配置、测试、脚本与资源。开始开发前必须同时阅读根目录 `README.md` 和 `docs/API_ARRAY_产品设计文档_V0.1.md`。产品设计文档中的冻结决策优先于一般实现偏好。

## 1. 当前阶段

项目当前处于 MVP 的 Rust Core 优先开发阶段。

在 Rust 业务核心能够独立编译、通过单元测试并完成模块整合之前：

- 不创建 Tauri App；
- 不开发正式 UI；
- 不让业务逻辑依赖 Tauri、WebView、Next.js 或浏览器运行环境；
- 不为了 UI 提前设计 IPC 细节；
- 不接入真实外部 API；
- 不编写虚假的网络 Provider 或 API Mock。

Rust Core 完成一轮较完整实现并通过逻辑检查后，再向用户请求用于真实联调的 API 配置。未经用户提供和授权，不得自行寻找、猜测或使用真实凭据。

## 2. 开发顺序

开发按以下阶段推进：

1. 设计并实现可独立运行的 Rust Core 模块。
2. 为纯逻辑、配置解析、Schema、图校验、路由决策、错误归一化等能力编写单元测试。
3. 将核心模块整合进独立 CLI 调试入口。
4. 完成一次集中构建、单元测试和 CLI 输入输出验证。
5. 复核核心逻辑、安全边界和模块接口。
6. 向用户请求真实 API 配置。
7. 创建被 `.gitignore` 排除的本地 `.env`，由用户按照模板填写。
8. 使用用户明确提供的凭据进行受控真实联调。
9. 核心业务稳定后再创建 Tauri App，并以模块或 workspace crate 的方式复用 Rust Core。
10. 最后接入 Next.js/React UI。

不得为了展示界面而跳过 Rust Core 的独立验证。

## 3. 推进方式

MVP 阶段优先进行完整、连贯的一轮实现，而不是机械地一次只写一个极小函数、频繁停下来等待确认。

每一轮开发应尽可能形成可验证的纵向能力，例如一次完成：

- 数据模型；
- 配置解析；
- 核心处理逻辑；
- 错误类型；
- CLI 调试入口；
- 关键单元测试；
- 构建与测试验证。

在边界清楚、风险可控且不改变冻结产品决策时，直接作出合理实现判断。只有涉及产品范围变化、真实凭据、外部副作用、安全策略改变或不可逆操作时才暂停并请求用户决定。

避免过度工程设计：

- 不为尚未进入 MVP 的团队版、公网版或 Docker 版提前实现完整抽象；
- 不引入暂时没有实际调用方的复杂框架；
- 不为了理论上的任意 Provider 写通用脚本运行时；
- 不反复重构尚未形成实际使用证据的接口；
- 不以“未来可能需要”为由增加不必要的服务、进程或依赖。

同时不得用“快速 MVP”作为牺牲密钥安全、错误处理、可测试性和模块边界的理由。

## 4. Rust Core 边界

核心业务必须保持为可被多个宿主复用的普通 Rust crate，不依赖 Tauri 生命周期。

优先保持以下逻辑边界：

- Provider YAML 加载与 Schema 校验；
- Adapter 协议接口；
- Secret 引用，不在 Core 配置对象中长期持有明文；
- Capability Manifest；
- 节点、端口、连接与图校验；
- Transform、Router、Guard 和 Publisher 的核心模型；
- 标准错误与上游错误归一化；
- 健康状态、探测结果与路由决策；
- 审计事件与通知事件；
- 可序列化的 CLI 输入输出协议。

Tauri 后续只能作为宿主和界面桥接层，不得复制一份业务逻辑。

## 5. CLI 调试入口

Rust Core 编译后必须能够通过独立 CLI 调试，不依赖 UI。

CLI 的目标是让简单脚本能够：

- 从 stdin 传入结构化参数；
- 调用指定核心模块或完整业务流程；
- 从 stdout 读取结构化结果；
- 从 stderr 捕获简短诊断日志；
- 根据进程退出码判断成功或失败。

首选使用逐行 JSON（JSON Lines）或单个 JSON 文档作为 stdin/stdout 协议。不要把 API Key 放在命令行 argv 中，因为命令行参数可能被进程列表、Shell 历史或调试工具记录。

建议约定：

- stdin：请求和参数；
- stdout：机器可读结果，默认不混入普通日志；
- stderr：简短的人类可读诊断日志；
- exit code `0`：成功；
- 非 `0`：失败，并在 stdout 或 stderr 中提供稳定错误码。

CLI 输入输出结构必须版本化，至少预留 `schema_version` 字段。

## 6. 日志规范

调试日志必须简短、结构稳定、便于 stdout/stderr 捕获，且不能大量占用终端或模型上下文。

默认要求：

- 正常机器输出写入 stdout；
- 诊断日志写入 stderr；
- 每个重要阶段最多输出一条摘要；
- 循环、流式数据和批量节点不得逐项刷屏；
- 重复错误应聚合或限频；
- 默认使用 `info`、`warn`、`error`，详细追踪只在显式开启时输出；
- 错误日志包含稳定错误码、模块、阶段和简短原因；
- 测试成功时不输出大段内部状态。

任何日志都不得包含：

- 完整 API Key；
- Authorization Header；
- 完整 Publisher Token；
- `.env` 原文；
- 默认关闭内容审计时的完整请求或响应正文；
- 可用于还原密钥的调试对象。

必要时只能显示经过统一脱敏的尾部短标识，例如 `sk-***abcd`，且脱敏逻辑必须集中实现。

## 7. 测试边界

在真实 API 联调前，允许并应当编写不依赖网络的确定性测试，包括：

- YAML/JSON 解析；
- Schema 校验；
- 数据结构序列化与迁移；
- 图合法性检查；
- 路由决策表；
- 超时和重试策略的纯逻辑；
- 错误类型映射；
- Capability Manifest 合并；
- 日志脱敏；
- CLI 输入输出协议；
- 使用本地静态 fixture 的解析测试。

禁止在当前阶段：

- 编写伪造真实供应商行为的网络 Mock Server；
- 用永远成功的 Fake Provider 掩盖未实现逻辑；
- 将 Mock 响应当作供应商兼容性证明；
- 为通过测试而硬编码虚构模型、余额或能力；
- 在没有用户凭据的情况下访问真实外部 API。

如果某项能力只有连接真实供应商才能验证，应明确标记为“待真实联调”，先完成其接口、输入校验、错误边界和可测试的纯逻辑，不伪造成功结果。

## 8. 真实 API 联调

只有在核心代码完成一轮较完整实现、单元测试通过并经逻辑复核后，才向用户请求 API。

届时应创建：

- 被 `.gitignore` 排除的 `.env`；
- 不包含真实值的 `.env.example` 或等效模板；
- 最小权限、最小费用、无副作用的联调步骤。

用户负责填写真实值。程序读取 `.env` 后不得打印原文。真实联调默认先运行非计费或最低成本检查；任何可能计费的生成请求必须在执行前明确说明。

真实凭据不得进入：

- Git；
- 测试 fixture；
- 快照；
- 示例代码；
- 错误报告；
- 文档；
- CLI 命令行参数。

## 9. 构建与验证

每个较完整开发轮次结束后，集中执行一次与改动相称的验证：

1. 格式检查；
2. 编译检查；
3. 单元测试；
4. CLI stdin/stdout 冒烟验证；
5. 日志和 Secret 泄漏检查；
6. 对新增模块进行一次端到端逻辑复核。

避免在没有新信息的情况下反复运行同一组耗时命令。遇到失败时先定位原因，集中修改后再重新验证。

最终汇报必须说明：

- 本轮完成的纵向能力；
- 实际运行的验证命令和结果；
- 尚未连接真实 API 而无法确认的部分；
- 下一轮是否已经达到请求用户提供 API 配置的条件。

## 10. 文件与仓库结构

根目录继续只保留 `README.md`、`docs/` 和 `src/` 三个产品入口。全部源代码、Cargo workspace、Provider YAML、Schema、模板、脚本、测试和资源均放在 `src/` 下。

除非构建工具存在无法规避的根目录要求，否则不得在仓库根目录散落开发文件。需要改变这一冻结结构时，先更新产品设计决策，而不是直接添加文件。

# V0.3 三站式架构强制约束（2026-07）

> 当前实现已升级为 V5。后续工作首先阅读
> `../docs/API_ARRAY_产品设计文档_V0.4_V5三页面模型.md`；若与下方 V0.3 规则冲突，以 V0.4 为准。

- 钱包资产与审计直出端点必须是两个实体；禁止把 alias、直出 Token 或本地 URL 放回 `ApiAsset`。
- 左侧核心入口固定为 API 钱包、审计直出、编组模式；项目树只在编组上下文显示。
- LocalGateway 只挂载 DirectEndpoint 与运行中 Canvas；钱包资产不得自动暴露。

后续桌面与 Runtime 开发必须先阅读
`../docs/API_ARRAY_产品设计文档_V0.3_三站式架构.md`。它覆盖此前 V0.2 的桌面信息架构：

- API 钱包是唯一资产源；钱包直用和 Canvas 编组都必须经过同一个 localhost 审计网关。
- 钱包入口使用 `/wallet/{assetAlias}/v1`，Canvas 入口使用 `/canvas/{canvasId}/v1`；每个入口独立 Token。
- Project/Folder 仅做组织，Canvas 才是独立运行边界；禁止恢复无归属的全局 Publisher 创建路径。
- 工作区 Schema V4 是 POC reset，禁止新增 V1/V2/V3 迁移或兼容读取逻辑。
- 成本是估算、预算仅预警；任何出口、日志、UI State 或导出不得包含上游 Key、本地 Token、Authorization Header 或请求正文。
- 桌面开发必须使用 `npm.cmd run dev`；可运行 Debug 产物必须使用 `npm.cmd run build:desktop:debug`，不能把直接 `cargo build` 的 Debug exe 当作完整桌面调试入口。

# Canvas V3 历史约束（2026-07）

后续桌面开发必须先阅读 `../docs/API_ARRAY_产品设计文档_V0.2_钱包与项目画布变更.md` 的“Canvas 独立工作台与两级信息架构”。以下规则不可绕过：

- 全局层管理钱包、运行总览、综合器、Publisher 总面板和审计；Project/Folder 仅组织 Canvas。
- Canvas 是唯一编辑与运行边界，拥有独立 Graph、草稿/应用版本、Publisher、活文档和记录。
- 所有 Canvas IPC 变更必须显式提供 `projectId + canvasId`，不得从当前 UI 选择推断后端目标。
- Workspace V6 不再包含全局 `workspace.graph`；每个 Canvas 的 Graph V2 是唯一编组事实来源。
- API Wallet 是 Workspace 共享资产；删除 Canvas 不得删除钱包资产或上游 Secret。
- 复制 Canvas 不复制 Token、端口或运行状态；Secret 不通过 IPC 返回，也不进入日志、UI State 或导出。
- 前端边界必须规范化 camelCase/snake_case 旧数据，缺失数组和映射一律安全降级，禁止未检查的 `.map()`。
- Canvas 行只做导航与状态表达；运行、暂停、停止、刷新属于 Canvas 工作台顶部操作。
