# API ARRAY V0.8：主页主控台与 Apps SDK UI

## 1. 变更目标

“实例机架”不再作为用户可见产品概念。工作区正常启动后进入“主页”，主区域标题为“主控台”。主控台负责回答三件事：当前本地服务是否可用、哪些实例需要处理、下一步应前往哪个配置页面。

它仍然只是 `DirectEndpoint` 与 Canvas Publisher 的运行投影和控制面，不保存第三份运行配置。

## 2. 信息架构

侧栏固定为：

1. 主页；
2. API 管理：API 钱包、审计直出、编组模式；
3. 运行与设置：运行记录、通知、文档、设置。

Project → Folder → Canvas 树只在编组模式展开。OOBE 添加第一个 API 后进入钱包；后续启动默认进入主页。

## 3. 主控台

主控台提供“概览 / 全部实例”两个视图。

- 顶部栏承载工作区级搜索，可统一查找钱包资产、运行实例和 Project/Canvas；搜索结果必须位于窗口内容之上，不得被主页或 Inspector 遮挡。
- 概览包含工作区与网关横幅、五个业务快捷卡、需要处理和最近活动。
- 业务快捷卡整卡即为操作入口，不在卡片内部重复放置“打开”按钮。
- 全部实例提供名称、Project、钱包资产、模型、类型和状态筛选，并使用响应式卡片。
- 卡片主操作只有启动或停止；自检、复制 URL、文档、审计和编辑进入更多菜单。
- “启动可用实例”允许部分成功；“停止全部”必须二次确认，且不停止网关监听器、不删除配置或 Secret。

## 4. 状态边界

Rust `InstanceControlService` 从 Workspace、ControlPlane、LocalGateway、审计记录和 Secret 就绪状态生成 `ControlCenterSnapshot`。前端控制器统一读取该快照并监听 `desktop:instances-changed`；顶栏、底栏和主页复用同一状态。

旧 UI State 的 `instances` 页面值读取时映射为 `home`。旧 IPC `instance_rack_snapshot` 不再注册。

## 5. Apps SDK UI 与主题

Tailwind 4 的全局导入顺序固定为 `tailwindcss`、`@openai/apps-sdk-ui/css`、Apps SDK UI `@source`。表单、按钮、Badge、选择器、分段控制、菜单、弹层、提示、空状态和图标优先使用 Apps SDK UI。

卡片使用语义 HTML、Tailwind 和 Apps SDK UI 主题令牌。窗口材质与 React Flow 等宿主级样式保留在 `globals.css`，页面不得恢复独立二级 CSS import。亮暗主题不得依靠组件内写死颜色表达业务状态。

Apps SDK UI 0.2.2 的 `CodeBlock` CSS 目前不能由 Next.js 16 Turbopack 解析。当前文档代码区使用语义令牌和官方 Button 的本地兼容封装，不修改 `node_modules`；升级组件包并验证构建后再替换为官方 `CodeBlock`。

## 6. 产品术语

所有用户可见位置统一使用“文档”或“接口文档”。内部 `LiveDocument`、Markdown 生成器和 IPC 数据结构不做无价值改名。历史冻结文档保留原术语。

## 7. 验收

- 默认路由和托盘导航打开主页。
- 主控台统计、待处理、最近活动、搜索筛选与详情跳转来自同一快照。
- 单项和批量启停、自检、文档与审计跳转保持 Direct/Canvas 隔离。
- 开发和生产构建均加载完整样式，favicon 可用，客户端错误边界可复制安全摘要。
- 现行 UI 不出现“实例机架”和“活文档”。
