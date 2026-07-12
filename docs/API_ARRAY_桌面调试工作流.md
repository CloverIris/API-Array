# API ARRAY 桌面调试工作流

## 两种模式

开发调试使用：

```powershell
cd E:\APIArray\src\apps\desktop
npm.cmd run dev
```

这个命令由 Tauri CLI 先启动 Next 开发服务器 `127.0.0.1:3000`，再启动桌面窗口。`devUrl` 只属于这个模式。

可独立运行的 Debug 桌面产物使用：

```powershell
cd E:\APIArray\src\apps\desktop
npm.cmd run build:desktop:debug
```

它先生成 Next 静态导出，再由 Tauri CLI 构建 Debug 桌面产物。不要直接运行 `cargo build` 生成的 Debug exe 来代替这个命令：在 Tauri Debug 配置下它可能仍会尝试访问 `127.0.0.1:3000`，而此时没有开发服务器。


## Runtime 关系

桌面版不需要另一个 Runtime exe。Tauri 宿主进程内直接加载 `apiarray-core`、`apiarray-runtime` 和 `LocalGateway`：

`Tauri host → ControlPlane / LocalGateway → Adapter / ResilientExecutor → 上游 API`

工作区存在时，宿主启动统一回环网关；首次 OOBE 尚未创建工作区时，网关显示为等待状态。创建工作区并添加启用资产后，网关会重载入口。调试日志只输出监听地址和阶段摘要，不输出 Key、Token 或请求正文。

## 快速检查

- 窗口能显示但浏览器提示 `localhost:3000 refused`：运行的是直接 Cargo Debug exe；关闭它并使用 `npm.cmd run dev` 或 `npm.cmd run build:desktop:debug`。
- APP 已打开但网关未运行：先完成 OOBE；首页右上角应显示网关状态和入口数量。
- 已添加资产但没有入口：确认上游 Key、本地网关 Token、Provider 启用状态都已保存。
- `7480` 被占用：在设置中调整回环端口后重启统一网关；禁止改成公网地址。
