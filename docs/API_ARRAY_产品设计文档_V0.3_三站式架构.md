# API ARRAY 产品设计文档 V0.3：三站式架构

状态：POC/MVP 破坏性变更 · 2026-07-12  
取代范围：桌面端的信息架构、工作区持久化与本地出口模型；V0.1、V0.2 保留为历史决策记录。

## 产品模型

API ARRAY 是本地 API 的三个站点：

1. **存储站（API 钱包）**：用户把端点、Provider 规则、上游 Key、能力和预算规则存入本机钱包。
2. **收费站（审计网关）**：用户可以不做任何编组，直接通过本地 OpenAI-compatible 入口调用单个钱包资产；每一次请求均经过鉴权、协议适配、健康处理和脱敏审计。
3. **编组站（Canvas）**：用户从钱包选择多个资产，配置主备、路由与回退后，发布为独立的本地 OpenAI-compatible API。

这三个站点不是三套运行时。API 钱包是资产源；统一审计网关是唯一 localhost 监听器；Canvas 是独立的编排与运行边界。

## 本地入口与安全

- 网关默认仅监听 `127.0.0.1:7480`，不支持公网绑定。
- 钱包资产入口为 `/wallet/{assetAlias}/v1`；Canvas 入口为 `/canvas/{canvasId}/v1`。
- 每个钱包资产和每个 Canvas 使用独立本地 Token。上游 Key 与本地 Token 仅存 Windows Credential Manager。
- 对外首期仅提供 OpenAI-compatible `chat/completions` 与 `models`；内部继续复用 OpenAI、Anthropic、Gemini 与 OpenAI-compatible Adapter。
- 审计只保存对象类型、对象 ID、模型、结果、延迟、重试、切换、用量与成本估算；不得保存正文、URL、Header、上游 Key 或本地 Token。

## 数据与运行边界

- Workspace Schema 固定为 V4。V1/V2/V3 不迁移、不只读兼容；桌面宿主启动时清理旧工作区文件和其中引用的 API ARRAY 凭据后再创建新工作区。
- API 钱包资产属于 Workspace，可被任意 Canvas 引用；删除 Canvas 永远不删除钱包资产或上游 Key。
- 每个 Canvas 独立保存图、草稿版本、应用版本、运行意图、入口 Token、活文档与审计筛选范围。
- Project 与 Folder 只承担组织职责，不能成为运行或出口边界。
- 资产预算和 Canvas 预算只用于预警。响应用量是事实，金额是按“用户规则 > 内置 YAML > 未知”得到的估算；未知价格不显示伪造金额，也不拦截请求。

## 桌面体验

- OOBE 的唯一首要任务是添加第一个 API，不要求用户先理解节点或 Publisher。
- 首页是 API 钱包：搜索、卡片、资产详情、直用 URL、模板、体检、审计与预算。
- 钱包卡的主操作是“直接使用”；“加入 Canvas”是次级专业操作。
- Project Tree 类似独立会话列表：Canvas 行只导航并显示状态；运行、暂停、刷新和删除只出现在 Canvas 工作台。
- Canvas 保留概览、编排、路由、出口、活文档和运行记录六个页签；未确认的预放置 Provider Group 仅存在于前端内存。

## 页面职责隐喻

页面名称必须直接表达业务动作，不能再使用含义模糊的“API 资产”作为总入口：

| 页面 | 用户问题 | 允许的动作 |
| --- | --- | --- |
| 接入 API | 我怎样把已有 API 放进系统？ | 选择 Provider、保存端点与 Key、验证上游；完成后资产进入钱包 |
| API 钱包 | 我有哪些 API，现在能不能直接用？ | 搜索资产、查看健康和用量、复制直用入口、选择资产加入 Canvas |
| 审计收费站 | 单个 API 怎样被本地审计后调用？ | 查看 localhost 入口、Token 状态、启停直用入口和审计记录 |
| Canvas 编组 | 怎样把多个 API 组合成一个新 API？ | 编排主备、回退与输出，运行独立 Canvas 出口 |

用户从“接入 API”完成保存后，下一步永远回到“API 钱包”；用户不需要进入 Publisher 页面才能直用单个 API。
