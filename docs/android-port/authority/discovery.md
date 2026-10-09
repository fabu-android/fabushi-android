# 本轮实际发现与尚未证明的内容

发现日期：2026-10-09。全部 SHA 见 `baseline.json`。

## 已读取的事实

Desktop main 固定在 `3bc92400826cc4ca7ac665b467708e22261edc61`。Android main 为 `59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`，已有 PR #3，修改前 HEAD 为 `5d049353e167d9b91b5c765e1686f1ac8670f2ff`。因此继续现有分支而不复制另一套迁移。

Android 原有主分支 AGENTS 仍提共享平台核心，PR 分支已经明确 standalone ownership；本任务采用 PR 分支规定并以 Desktop main 代替历史 Grok 为产品权威。旧文档含 `bhrumom/fabushi-android` 名称，实际目的仓库是 `fabu-android/fabushi-android`。

已读取 `mobile/android/app/build.gradle`：这是 Groovy 文件，不是 `.kts`；applicationId/namespace 为 `com.ombhrum.fabushi`，minSdk 26，compileSdk/targetSdk 37。Compose/平台 Kotlin 已通过 sourceSets 指向 `frontend/src/main/kotlin`、`source/android-main/src/main/kotlin`、`source/android-preload/src/main/kotlin`。存在 release、githubRelease、ciAcceptance 三类配置，不能重建一个与现有源脱节的 app 目录。

已读取 root `Cargo.toml`：包括 source/shared、internal、mahayana-agent-coordinator、host、android-host-jni、local-exec-daemon、box-exec-daemon 和 agent-core/agent-kv/agent-transcript/chat-inference-proto/context/constants/mcp-core/redaction/utils。存在 workspace 不代表这些组件已经达到 Desktop 当前语义。

已读取 Desktop MCP `bridge.ts`：私有 MessagePort、pluginInstanceId、每次 load nonce、grants、ready、pending 和 dispose reject 是必须迁移的实际语义。已读取 `mahayana-host-features.ts`：登录、runtime ready、聊天、Mini App 安装/打开、能力审批、中断及清会话有共享 journey 类型。

## 发现的文档/发布风险

Desktop `DESKTOP_SOURCE_CLOSURE.md` 列出的 `frontend/apps/web/src/lib/mahayana-host/contracts.ts` 在固定 commit 读取返回 404。这说明旧来源说明不能直接当当前文件清单；应从真实树追踪其移动/替代路径，禁止继续引用失效路径作为已读证据。

Android `githubRelease` 当前将 `CI_ACCOUNT_SESSION_IMPORT_ENABLED` 设为 true。只凭开关不能确认完整攻击路径，但其在可分发包中存在必须成为安全审计项：证明 session import 的调用边界、授权与清理，最终公开生产产物应关闭/移除测试账户入口。`ciAcceptance` 的测试成功也不能代替经 R8/minify 的生产包验收。

本轮未证明：全部 Desktop 文件已逐行理解、全部 wire RPC 已精确提取、全部 Android 已有模块当前行为、既有 CI 全绿、真机安装升级、生产登录、Play 签名与发布权限。相应工作由台账、阶段与验收门约束，不标完成。

## 事实、设计与待证据的区别

文档中的 owner、状态机和错误处理是新迁移规范；不是对当前 Android 已实现状态的断言。范围清单由 Actions 从真实 Git tree 生成；函数级合同需要实现者阅读源和 tests 后建立映射。路径不存在必须定位替代，不能新建同名空文件掩盖缺失。
