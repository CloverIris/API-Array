# API ARRAY

API ARRAY 是一个面向 Windows 10/11 的本地优先 API 控制平面。它帮助个人开发者和小型组织安全管理多个 AI API，通过可视化节点编排完成能力探测、协议归一化、路由、故障切换、审计与本地高性能发布，并为发布后的能力生成可直接运行的活文档和多语言调用模板。

当前项目已完成产品设计冻结，进入 V0.2 Rust Core 可行性原型阶段。Tauri 与正式 UI 尚未开始开发，当前核心可以通过独立 CLI 编译、测试和调试。

## 仓库结构

```text
APIArray/
├─ README.md
├─ docs/       # 产品、架构和设计文档
└─ src/        # 后续全部源代码、配置、资源和测试
```

## 当前基线

- 产品设计基线：[API ARRAY 产品设计文档 V0.1](docs/API_ARRAY_产品设计文档_V0.1.md)
- Rust Core 开发约定：[src/AGENTS.md](src/AGENTS.md)
- 产品设计状态：V0.1 已冻结
- 开发状态：V0.2 Rust Core 原型
- 许可证方向：MIT

“冻结”表示本文档中的产品边界可作为首版设计和实现依据。任何改变冻结项的提案都应记录变更原因、影响范围和新的版本号。

## Rust Core

当前 Rust 实现已包含 Provider YAML、统一请求与响应模型、OpenAI/Anthropic/Gemini Adapter、字节级 SSE 解析、工作流与路由、Runtime 配置编译、健康状态、Publisher 安全约束、活文档模板、JSON Lines CLI，以及独立的异步 HTTP 数据面和本地 OpenAI-compatible Publisher。

在 PowerShell 中执行完整验证：

```powershell
cd src
.\scripts\Test-Core.ps1
```

通过 stdin 文件调试完整 Dispatch 计划：

```powershell
cd src
.\scripts\Run-Cli.ps1 -InputFile .\examples\cli\plan-dispatch.jsonl
```

构建并启动本地 Publisher：

```powershell
cd src
.\scripts\Run-Publisher.ps1 -InputFile .\examples\cli\launch-publisher.jsonl
```

示例启动文件只包含 Secret 引用到环境变量名的绑定，不包含真实密钥。当前尚未进行真实上游 API 联调。
