# 06 — Mini App、App Surface 与 WebMCP

## 产品目标

用户可以从 Bot/插件进入 Mini App，看到与当前会话有关的内容，经过明确授权调用工具，把用户主动提交的消息或上下文送回同一个会话，并随时关闭。主应用仍为原生 Compose；WebView 仅承载受隔离的插件内容，不拥有账户真相、模型密钥或 Host 任意访问权。

Android 的 SurfaceSession owner 管理一个页面加载生命周期，Host/CapabilityBroker 管理实际授权与工具执行。View 层重建不等于新 run；实际 page reload 则必须生成新 load session 并销毁旧 bridge。

## 已核实的 Desktop bridge 合同

源文件：固定 Desktop commit 下的 `frontend/packages/mcp-app-sdk/src/bridge.ts`。

Bootstrap 使用 `type = mahayana/bridge-init`、`bridgeVersion = 2.0`、`pluginInstanceId`、`nonce`、`grants` 和一个 MessagePort。Desktop 客户端校验 bootstrap 的来源、nonce 至少 16 字符和恰好一个端口；后续消息经私有端口传递，不继续使用全局 wildcard message 通道。

端口 envelope 为 `{pluginInstanceId, nonce, payload}`。已读方法包括 `tools/call`、`ui/message`、`ui/update-model-context`；通知包括 `ui/notifications/tool-input` 和 `ui/notifications/tool-result`。JSON-RPC requestId 支持数字或字符串。Android 需要保留这些语义，并让原 SDK 在平台桥接适配后运行；不能不经协商换成只发送 name/input 的不受会话约束接口。

## Android 会话约束

原生持有以下逻辑记录：accountId、sessionEpoch、conversationId、pluginInstanceId、loadGeneration、cryptographically-random nonce、origin/内容摘要、明确 grants、pending map、disposed。上述原生字段是内部约束，不是宣称 Desktop envelope 具有这些额外字段。

创建页面前确认安装版本和来源，创建新 load token，配置只针对该页的消息通道。接到调用时依次校验：surface 仍活动、来自允许的页面、实例和 nonce 匹配、requestId 未重复、方法已授权、参数满足 schema、账户与会话仍一致。只允许固定消息处理入口；不向任意网页暴露具应用权限的 Java 对象。

页面导航、账户切换、插件卸载、surface close 和 WebView renderer 退出，都必须触发统一 dispose。dispose 原子关闭会话，拒绝未完成的 JS Promise，取消原生 Tasks，关闭通道、移除注册和监听器。只有原 load token 仍 active 的结果可以回写；旧任务不能因为 WebView 对象被复用就把结果送入新页面。

## WebMCP 兼容路径

已读固定 Desktop commit 下的 `webmcp.ts`优先检测 `document.modelContext`；以 AbortSignal 注册，在 disposer 中 abort；还保留 `window.__fabushiWebMcp` version 1 的本地 list/call registry。

不能假定所有 Android WebView 都支持该 draft API。以能力检测选择 native registry 或兼容 registry；native 注册被策略拒绝时仍保留兼容路径，但不能产生双份工具 owner。注销要按实例身份移除，旧实例 dispose 不得删掉同名新注册。fallback 与 native 都必须穿过同一权限和会话边界，不能让 fallback 绕开授权。

## 消息、上下文与展示

`ui/message` 只允许由明确用户动作或已授权协议动作产生；页面自动刷新不能偷偷发聊天 turn。`ui/update-model-context` 必须有数据大小和敏感信息边界；页面返回值是外部内容，不是系统指令。富内容的 URI、HTML、文件引用需校验来源、大小与允许类型。

加载中、待授权、ready、离线、工具处理中、加载失败、权限撤销、已关闭分别呈现。原生返回键优先按定义关闭临时 surface，关闭前保留用户可恢复的表单/会话状态，不保留旧 nonce。

## 核心反例与测试

| 场景 | 必须断言 |
| --- | --- |
| 缺少或错误 nonce / pluginInstanceId | 原生端拒绝，工具完全不执行 |
| 正确实例但缺少 grant | 返回受控错误，没有隐式弹框自动许可 |
| 同一 pending requestId 再次调用 | 明确拒绝重复，而非覆盖原 Promise |
| load A 运行中切换 load B | A 的结果不能进入 B；A pending 正确终结 |
| 注册同名工具后旧实例 dispose | 新注册仍有效 |
| native WebMCP 不存在或注册拒绝 | 兼容 registry 可用且没有第二授权入口 |
| dispose 前从未 bootstrap | ready/pending 不会永久挂起 |
| hosted 外链替代本地 Mini App | 不继承本地 surface 的 grants 和密钥 |
| 旋转/进程死亡 | 恢复可解释页面状态，不重放已发消息 |

单位测试覆盖 owner 状态机和协议校验；仪器测试通过真实 WebView、实际 SDK bootstrap 和真实 Host 受控工具执行，覆盖消息通道、导航与 renderer 生命周期。给出 release 包的负例证据，不只在 debug bridge 中验证。

## 参考

[Desktop bridge](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/bridge.ts)；[Desktop WebMCP](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/webmcp.ts)；[Android WebView native bridge 风险](https://developer.android.com/privacy-and-security/risks/insecure-webview-native-bridges)。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现作为 Android 仓库中的实施规范；不代表代码已实现或产品测试通过。本文中的新模块、内部模型、状态名称和错误类别是 Android 设计要求，除明确引用的 Desktop 符号外不冒充真实 wire API。开发者须绑定到 Desktop 固定 SHA 的真实符号、调用者、测试和 Android shipping 路由；构建和验收仅在 GitHub Actions 执行。

统一源锁：`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61`。源码清单已登记 8,172 个文件条目，不代表人工逐文件验收。
