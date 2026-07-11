# API ARRAY

API ARRAY 是面向 Windows 10/11 的本地优先 API 控制平面。它帮助个人开发者和小型组织安全管理多个 API，通过节点编排完成能力探测、协议归一化、路由、故障切换、审计与本地高性能发布，并为发布后的能力生成可直接运行的活文档和多语言调用模板。

项目当前已完成 UI 前的 Rust Control Plane 基础层。Tauri 与正式 UI 尚未开始；核心可通过独立 CLI 编译、测试和调试。

## 仓库结构

```text
APIArray/
├── README.md
├── docs/       # 产品、架构和设计文档
└── src/        # 源代码、配置、资源、脚本和测试
```

## 当前基线

- [API ARRAY 产品设计文档 V0.1](docs/API_ARRAY_产品设计文档_V0.1.md)
- [Rust Core 开发约定](src/AGENTS.md)
- 产品设计状态：V0.1 已冻结
- 开发状态：V0.2 Rust Core 原型
- 许可证方向：MIT

## Rust Core

当前实现包括：

- YAML Provider 清单与 OpenAI、Anthropic、Gemini Adapter；
- 统一请求/响应模型和字节安全 SSE 转换；
- Runtime 配置编译、健康路由、请求级重试与自动故障切换；
- 简单元数据条件路由、节点影响分析与版本化体检报告；
- Secret 引用、环境变量安全解析和本地 Bearer Token；
- Windows Credential Manager SecretStore、原子工作区保存和备份恢复；
- OpenAI-compatible 本地 Publisher 与请求关联 ID；
- 多 Publisher Supervisor：启动、暂停、停止、恢复意图与结构化状态快照；
- 不记录正文、Header、URL 或 Secret 的 JSONL 请求审计；
- 安全工作区导入导出、未知版本只读打开和缺失凭据状态；
- Control Plane 宿主接口与聚合通知模型，供下一阶段 Tauri/React UI 直接复用；
- 八种语言的活文档模板与 JSON Lines CLI；
- 独立真实 API 探测入口。

完整验证：

```powershell
cd src
.\scripts\Test-Core.ps1
```

调试 Dispatch 计划：

```powershell
cd src
.\scripts\Run-Cli.ps1 -InputFile .\examples\cli\plan-dispatch.jsonl
```

启动本地 Publisher：

```powershell
cd src
.\scripts\Run-Publisher.ps1 -InputFile .\examples\cli\launch-publisher.jsonl
```

调试工作区的保存与加载：

```powershell
cd src
.\scripts\Run-Control.ps1 -InputFile .\examples\cli\control-request.jsonl
```

Publisher 将脱敏运行记录追加到 `src/runtime-audit.jsonl`；该文件和 `.env` 均被 Git 忽略。示例启动文件只包含 Secret 引用与环境变量名，不包含真实密钥。

当前 Rust Core 与冻结 PRD 的逐项覆盖情况见 [Rust Core 对 PRD 覆盖审计 V0.2](docs/RUST_CORE_PRD_覆盖审计_V0.2.md)。
