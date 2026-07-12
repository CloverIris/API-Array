# API ARRAY V0.7：全局实例机架

## 产品定位

实例机架是 API ARRAY 的统一运行控制面。它回答“当前有哪些本地 API 实例、哪些正在运行、哪些需要修复”，但不保存第三份业务配置。

- API 钱包仍是上游资产事实源。
- `DirectEndpoint` 仍在“审计直出”页面配置。
- Canvas Graph、出口和草稿仍在对应 Canvas 工作台配置。
- 实例机架只把 Direct Endpoint 与 Canvas Publisher 投影为 `ManagedInstance`。

工作区完成 OOBE 后首次进入钱包；正常再次启动默认进入实例机架。

## 实例身份与状态

- 审计直出：`direct:{endpointId}`。
- Canvas：`canvas:{projectId}:{canvasId}`。
- 状态必须同时参考运行意图、Secret、Graph 编译结果、ControlPlane 和 LocalGateway 实际挂载入口。
- `enabled=true` 但入口未挂载时必须显示为失败，禁止误报“运行中”。
- 未发布 Canvas 仍显示在机架中，状态为“尚未发布”。

## 控制语义

- 单项启动在写入运行意图前完成 Token、上游 Secret、钱包引用和 Graph 编译检查。
- Canvas 启动会应用当前草稿，形成新的运行快照。
- 全部启动采用部分成功：健康项一次启用，阻塞项保持原状并返回修复原因。
- 全部停止只关闭所有业务入口，LocalGateway 继续监听，配置和钱包资产不受影响。
- 批量操作最多保存一次工作区、刷新一次 ControlPlane、重建一次网关。
- 网关刷新失败必须恢复原运行意图并尝试恢复原入口。

## UI 与导航

全局主导航依次为：实例机架、API 钱包、审计直出、编组模式。机架提供状态汇总、搜索筛选、单项与批量启停、自检、URL、活文档、审计及编辑跳转。

批量结果集中显示，不为每个失败项生成独立通知。Direct Endpoint 跳转回审计直出配置，Canvas 跳转到显式的 `projectId + canvasId + tab`。

## 安全边界

- `ManagedInstance` 是只读投影，不进入 SQLite 业务配置表。
- IPC 不返回上游 Secret、本地 Token、Authorization Header 或正文。
- 自检由 Rust 宿主读取凭据并调用本机 `/models`，前端只接收延迟、模型数和安全摘要。
- 审计指标只读取脱敏记录。
