# API ARRAY 产品设计文档 V0.4：V5 三页面模型

状态：当前 POC 基线 · 2026-07-12  
覆盖：V0.3 中“钱包资产自动直出”的设计。

## 唯一主线

1. **API 钱包**回答“我有哪些 API？”：在同一页面完成添加、编辑、替换 Key、启停、删除、自定义 YAML 和上游验证。
2. **审计直出**回答“怎样把某个 API 经审计后直接使用？”：从钱包资产创建一个或多个独立本地端点。
3. **编组模式**回答“怎样把多个 API 组合成新的 API？”：使用 Project → Folder → Canvas 三级体系组织独立编组实例。

## 数据边界

- `ApiAsset` 只表示上游资产，不拥有本地 alias、直出 Token、直出 URL 或直出运行状态。
- `DirectEndpoint` 引用一个 `ApiAsset`，独立持有 alias、Token 引用、模型映射、超时、重试、审计标签、预算覆盖和运行状态。
- 同一资产允许创建多个直出端点；端点之间的 Token、alias、策略和审计互不共享。
- LocalGateway 只挂载已启用的 `/direct/{alias}/v1` 和运行中的 `/canvas/{canvasId}/v1`。钱包资产本身永远不会自动暴露。
- Canvas 只能引用钱包资产，不能把 DirectEndpoint 当作上游。

## 桌面信息架构

- 左侧主导航固定为 API 钱包、审计直出、编组模式。
- Project/Folder/Canvas 树仅在编组模式或 Canvas 工作台中显示。
- 运行记录、通知、调用模板和设置保留为辅助入口。
- 删除钱包资产前必须列出所有 DirectEndpoint 和 Canvas 引用；删除直出端点不删除资产或上游 Secret。

## 版本策略

Workspace Schema V5 是破坏性 POC reset。V4 不迁移；桌面首次启动 V5 时清理旧工作区及其中引用的 API ARRAY 凭据。
