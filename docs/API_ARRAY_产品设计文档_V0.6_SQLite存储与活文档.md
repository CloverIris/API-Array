# API ARRAY V0.6：SQLite 存储服务与活文档

## 1. 架构边界

- Core 定义 Workspace、Graph、LiveDocument 与验证规则，不依赖数据库和桌面系统。
- Runtime 的 `StorageService` 是唯一持久化语义入口；SQLite 表结构不得泄漏给 Core、Tauri IPC 或前端。
- Tauri 负责选择工作区、Windows Credential Manager 与系统验证；前端不得直接访问文件或数据库。
- Secret 明文和 Publisher Token 永不进入 SQLite、审计、通知、错误、UI State 或导出。

## 2. 工作区目录

```text
workspace-root/
├─ workspace.sqlite3
├─ attachments/
├─ exports/
└─ backups/
```

SQLite 启用 WAL、外键、FULL synchronous 与 busy timeout。主业务实体规范化保存；Graph、Provider Manifest 和演进中的类型配置使用带 Schema 版本的 JSON payload。`workspace_snapshot` 是同一事务内写入的恢复聚合，不是第二份业务事实来源。

## 3. 存储语义 API

`StorageService` 提供 Workspace 加载/保存、描述信息、健康检查、备份、压缩、设置、审计和体检报告能力。调用方不能取得 SQLite Connection，也不能自行编写 SQL。

- Workspace 保存必须先通过 Core 校验，并在单一事务中更新规范化表、聚合快照和 revision。
- 审计按主体与时间建立索引，只保存脱敏执行元数据。
- 备份先执行 WAL checkpoint，再使用 SQLite Online Backup API 写入 `backups/`。
- 完整性检查失败只报告和进入恢复 UI，不自动删除数据库或凭据。
- UI State 是工作区设置项，损坏时可回退默认值，不阻止业务数据库打开。

## 4. Secret 与系统验证

SQLite 仅保存 `secret://` 引用。Runtime 调用上游可按需解析 Secret，不触发交互验证；只有向人显示或复制明文时才触发 Windows Hello。Windows 10 不提供明文查看降级，只允许替换和删除。

## 5. 活文档

Core 从真实入口配置生成结构化 `LiveDocument`，其中包含入口事实、准备步骤、当前语言代码、审计说明和故障排查。Markdown 是结构化文档的确定性投影；前端不得拼接 Provider 专用静态说明，也不得渲染任意 HTML。

## 6. 禁止事项

- 禁止恢复启动时 POC 自动清理或旧 Schema 递归删除。
- 禁止把 `workspace.json` 当作正式运行存储。
- 禁止在前端、Tauri 命令或业务模块中直接执行 SQL。
- 禁止把 Secret 明文写入数据库以换取跨设备迁移。
