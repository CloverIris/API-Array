# API ARRAY V1.1：第二阶段可用性完成度与可靠性审计

## 目标

第二阶段不改变 `1.0.0 Preview` 版本号，目标是把 API ARRAY 从“可公开试用”推进到“可长期使用的本地 API 基础设施工具”。本阶段重点不是继续增加松散页面，而是把 API 钱包、审计直出、编组方案、统一网关、SQLite 存储、审计记录、通知和应用生命周期连成可验证的闭环。

## 产品边界

- **主页 / 主控台**：只负责全局运行状态、批量启停、异常定位和跳转。
- **API 钱包**：只负责上游资产、Key、能力、预算、体检和引用影响。
- **审计直出**：只负责把钱包资产发布为本地受审计入口。
- **编组模式**：只负责在项目、文件夹和编组方案中组合多个钱包资产。
- **文档**：只展示当前 Direct Endpoint 或编组方案出口的真实调用方式。
- **设置**：只管理网关、工作区存储、备份、安全、主题和发布诊断。

`Canvas` 继续作为内部 Rust/IPC/数据库命名；所有用户可见文本必须使用“编组方案”。

## 可靠性硬化

- 运行变更必须按“预检 → SQLite 保存 → Runtime 编译 → Gateway 候选构建 → 成功切换 / 失败回滚”推进。
- LocalGateway 继续只监听一个 localhost 地址和端口，入口固定为 `/direct/{alias}/v1` 与 `/canvas/{projectId}/{canvasId}/v1`。
- Gateway 必须能报告 configured/bound 地址与端口、已挂载入口、阻塞入口、重复路由和最近错误。
- Graph V2 的“结构可保存”和“运行可发布”必须分离校验；运行前使用 `validate_runtime_semantics` 和 `CanvasCompilationReport`。
- 审计写入失败后入口进入降级状态，后续请求拒绝继续伪装成“审计正常”。
- 工作区切换、备份、压缩和完整性检查只能通过 Storage Service 语义接口，不得让 UI 或 Tauri 命令直接依赖 SQL 表布局。

## 安全边界

- 上游 Key 与本地入口 Token 只存在于 Windows Credential Manager；SQLite、日志、通知、UI State 和导出文件只能保存引用或脱敏摘要。
- Windows 11 查看或复制 Key 必须经过系统验证；Windows 10 保持不显示明文的安全降级。
- 复制 Secret 不得主动清空用户剪贴板，也不得把明文写入任何持久化路径。
- 真实 Provider 验证只能使用用户明确授权的 `.env` 或已录入凭据，默认不执行计费探测。

## 验证门槛

每轮第二阶段开发至少按改动范围运行：

- 前端：`npm.cmd test`、`npm.cmd run build`
- Rust：`cargo check --workspace` 或目标 crate 检查；涉及 Graph/Gateway/Storage 时运行对应测试
- 桌面：Tauri Debug 冒烟，重点检查启动、退出、托盘、主题、主控台、钱包、直出和编组方案
- 发布前：MSI 安装、启动、卸载、重装、工作区恢复、图标和 SmartScreen 提示确认

真实链路发布前验证固定为 `/v1/models`、最小非流式、最小流式、受控主备切换和审计脱敏检查；禁止购买、删除、计费探测或资源变更操作。
