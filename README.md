# API ARRAY

**API ARRAY 1.0.0 Preview** 是面向 Windows 10/11 的本地优先 API 钱包、审计网关与编组工具。

它把产品主线固定为三步：

1. 在 **API 钱包** 安全保存已有 Provider、端点与 Key 引用。
2. 在 **审计直出** 为单个钱包资产创建受审计的本地 OpenAI-compatible 入口。
3. 在 **编组模式** 用项目、文件夹与编组方案组合多个上游，并发布新的本地统一 API。

所有入口只监听回环地址；上游 Key 与本地 Token 只保存于 Windows Credential Manager，SQLite 仅保存脱敏配置、审计和工作区数据。

## 1.0.0 Preview 状态

- Windows x64 桌面应用，Next.js 静态导出 + Tauri + 嵌入式 Rust Runtime。
- API 钱包、审计直出、Canonical 编组图、主控台、SQLite 工作区、文档、托盘、审计与通知中心。
- Preview 安装包是未签名的每用户 MSI；Windows SmartScreen 可能提示风险。
- 不支持公网暴露、团队共享、Docker 服务端或自动升级。

## 开发与调试

```powershell
cd src/apps/desktop
npm.cmd run dev
```

该命令会同时启动 Next.js 前端、Tauri 宿主和进程内 Rust Runtime；不要直接双击 `target/debug` 下的旧可执行文件来代替开发流程。

完整检查：

```powershell
cd src
cargo test --workspace
cargo check --workspace

cd apps/desktop
npm.cmd test
npm.cmd run build
```

## Preview MSI 发布

```powershell
cd src/apps/desktop
npm.cmd run release:preflight
npm.cmd run release:verify
npm.cmd run release:msi
```

MSI 仅能在 Windows 上通过 WiX Toolset v3 构建；需要启用 Windows 的 VBSCRIPT 可选功能。产物、SHA-256 和 Preview 发布说明会写入 `src/release/`（该目录不进入 Git）。详细规则见 [1.0.0 Preview 发布就绪文档](docs/API_ARRAY_产品设计文档_V1.0.0_Preview_发布就绪.md)。

## 仓库结构

```text
APIArray/
├── README.md
├── docs/
└── src/
    ├── product-version.json
    ├── crates/
    └── apps/desktop/
```

许可证：MIT。
