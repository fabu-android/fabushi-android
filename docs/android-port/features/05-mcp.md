# 05 — 插件市场、MCP 与连接器

## 用户目标与职责边界

用户应能查找插件、查看来源与权限、安装、更新、连接账户、发现工具、审批调用、查看结果、取消、断开连接和卸载。市场项目存在、已经安装、账户已连接、工具可用、用户已授权是五个不同事实，不得用一个 `isInstalled` 布尔值混为一谈。

沿用 Android 自有 `source/packages/mcp-core` 与 Host 的职责边界。Host 内的插件安装服务管理版本和状态；连接器会话管理认证及传输；CapabilityBroker 管理资源级授权；Coordinator 管理 run 与工具调用关联；Compose 只呈现投影。平台认证适配位于 `source/android-main`，有类型的入口位于 `source/android-preload`，Rust JNI 复用 `source/android-host-jni`。这些是职责归属，不要求新建重复服务。

## Desktop 审计入口与需要提取的精确合同

已确认存在的 `frontend/packages/mcp-app-sdk/src/{bridge,webmcp}.ts` 提供 UI 调用与工具注册行为。共享 `mahayana-host-features.ts` 明确有 `marketplace.install`、`miniapp.open`、`capability.approval` 等旅程。

旧 `DESKTOP_SOURCE_CLOSURE.md` 中市场相关路径只能作为搜索线索，不能当成当前可用文件。在完整 source tree 中定位安装状态机、marketplace 查询、MCP transport、OAuth、权限撤销和生产 Host composition，逐项提取：方法名、版本协商、分页、错误码、重连游标、工具 schema、授权域和卸载语义。输出持久 contract register；尚未取得真实 wire 的项目不得声称连接器已兼容。

## 数据、版本与状态设计

| 聚合 | 必须区分的信息 | 状态或不变量 |
| --- | --- | --- |
| 插件定义 | pluginId、来源、版本、内容摘要、声明权限、兼容范围 | 插件名不作为稳定身份；展示声明与已批准权限分离 |
| 安装 | accountId、pluginId、operationId、目标版本、上个有效版本 | absent → resolving → staging → validating → installed；失败回到可解释状态 |
| 连接器 | connectorId、accountId、sessionEpoch、授权范围、认证句柄 | disconnected → authorizing → connecting → ready；expired/revoked/error 单独呈现 |
| 工具目录 | serverIdentity、schemaVersion、工具定义、目录版本 | 目录变化不自动获得新权限 |
| 调用 | runId、callId、plugin实例、参数摘要、审批凭据、deadline | 唯一 owner 维护 pending/running/settled/outcome-unknown |

这里是应用内部数据需求，外部协议字段必须使用已核实的真实 wire 名称。秘密只通过受保护的凭据句柄提供给实际 transport，不能放入插件 metadata、WebView、Compose state 或普通数据库导出。

安装先在 staging 校验大小、路径、完整性、兼容性和来源，再以原子激活方式切换版本。更新失败保留此前可用版本；运行中的旧版本实例须按既定策略完成或显式取消，不能半途用新 schema 解释旧结果。恢复时检查激活指针与操作日志，不重复安装副作用。卸载同时处理 UI surface、pending calls、授权、缓存和持久引用，保留用户可选择的数据保留规则。

## 认证、调用与恢复顺序

用户点击连接后创建一次性认证事务，记录账户与 epoch，经系统浏览器完成回调验证。只有同一事务成功，Host 才发布 ready。用户取消、返回两次、账户切换或旧回调到达均不能连接到另一账户。

工具调用必须先验证当前会话和授权，再校验参数 schema、大小、资源范围与 deadline；取得唯一 callId 后提交给 transport。`readOnlyHint` 等注解不能代替安全判断。Android 不支持的桌面 stdio/可执行插件须选择可审核的本机静态实现或明确配对的远程 Runner，不下载任意原生代码绕过平台限制。

重试由 Host/Coordinator 负责，UI 不另起调用。只读操作可按明确策略重试；有副作用的请求只有远端有可信幂等合同才可重放。断网发生在提交后时，先查询结果或标记 outcome-unknown；不得把 timeout 自动等同“没执行”。取消必须传递到 transport，晚到结果按原会话归属处理，不能写入新账户。

## 用户可见错误

区分未安装、未连接、授权过期、权限不足、协议不兼容、工具移除、参数错误、配额限制、离线、远程执行位置不可用、结果不确定。每一类都提供与状态匹配的动作，例如重新认证、调整参数、查看已提交操作或切换到已授权执行位置；禁止通用“重试”按钮对所有错误盲发请求。

## GitHub Actions 验收

必须有真实 Host 组合测试：安装成功和损坏包失败；相同 operationId 重入；更新中断恢复；卸载取消 pending；OAuth 旧 epoch 回调；工具目录变化不自动授权；撤销后调用拒绝；断线后只读恢复和副作用不盲重放。UI 测试必须从市场进入，真实完成安装、连接、审批、调用与显示结果，不能直接构造 ready fixture 代替关键路径。

每个场景记录插件版本、来源摘要、实际 Android checkout、Desktop SHA、callId 的脱敏关联及 assertion。公开发布包不得带测试账户导入捷径。

## 参考

Desktop SDK：[bridge.ts](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/bridge.ts)、[webmcp.ts](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/webmcp.ts)。外部认证和 MCP 协议版本需要由源码 lock 与实际服务协商结果进一步固定，不以本文猜测版本。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现作为 Android 仓库中的实施规范；不代表代码已实现或产品测试通过。本文中新模块、内部模型、状态名称与错误类别是 Android 设计要求；除明确引用的 Desktop 符号外，不冒充真实后端 wire API。开发者须把职责绑定至 Desktop 固定 SHA 的源码符号、调用者与测试，再落实到 Android shipping 模块。构建、测试及真机测试调度统一通过 GitHub Actions。

源锁：`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61`。Actions 已登记 8,172 个文件条目，但不等于全部源码已人工审阅。
