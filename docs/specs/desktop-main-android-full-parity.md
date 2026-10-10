# Fabushi Desktop → Android 完整原生迁移总规范

状态：active；文档规范，不代表产品已完成。版本：2026-10-09。

## 1. 最终目标与权威顺序

将 `bhrumom/fabushi-desktop` 当前 canonical `main` 的全部适用于 Android 的产品职责、运行时行为、协议、状态机、用户可见功能、错误恢复和发布质量，完整实现到 `fabu-android/fabushi-android`。不是桌面网页套壳、仅 UI 模仿、只移植 Agent、只补 CI、只生成目录或只完成部分 vertical slice。

权威顺序：用户最新明确要求 → 本规范及 `docs/android-port/` 的明确设计要求 → 本规范绑定的 Desktop main 源码与实际 shipping composition → Android 既有规范的非冲突要求 → 历史 Grok 参考资料。关于现状、文件位置和现有协议的事实，必须以固定 SHA 的实际代码与生产接线为准，不能让旧文字覆盖真实源码。旧规范 `docs/specs/grok-bot-0.18-android-architecture-parity.md` 的原生化、独立源码、边界隔离、逐源审计、进程恢复和删除 legacy 要求保留；其以 Grok 重建仓库作为最终产品权威的表述被本规范取代。Android 是 Fabushi Desktop 的原生 Android 版本，不是另一款 Grok 产品。

本轮发现基线：
- Desktop main：`3bc92400826cc4ca7ac665b467708e22261edc61`；root tree：`3d2a0ad250ca82d0cf3b7bd917d8eca7400e5c31`。
- Android main：`59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`。
- Android 既有迁移 PR：#3，分支 `spec/grok-bot-018-android-parity-20260922`；本轮修改前 HEAD：`5d049353e167d9b91b5c765e1686f1ac8670f2ff`。

每次实现与最终验收都重新读取 Desktop main 和 Android exact HEAD。未合并的 Desktop PR 只进入观察清单，不冒充 main 已有功能。Desktop main 改变后逐 changed path / responsibility rebaseline，不继承失效证据。

## 2. 全范围，不只 source/frontend

必须取得完整 Git root tree 与 recursive inventory，保存 commit、tree、blob SHA、mode/type、路径和来源。除了 `source/**`、`frontend/**`，还审计真实树中的 `native/**`、`desktop/**`、`contracts/**`、`chatgpt-vps-control/**`、`projects/**`、`scripts/**`、工作流、构建配置、依赖锁、文档、资产、原生库、gitlink/submodule 与 Git LFS 指针。将来出现的 `third_party/**`、`manifests/**` 或其他目录也自动纳入；此列举不是白名单，也不宣称这些目录当前全部存在。

`RUST_RUNTIME_SOURCE.md` 与 `DESKTOP_SOURCE_CLOSURE.md` 是历史来源入口，不是可靠的当前路径清单。实际已读 `desktop/package.json` 的 Host 构建指向 `source/host/app/Cargo.toml`、`source/node-agent-coordinator/Cargo.toml`、`source/box-exec-daemon/Cargo.toml`；已读树显示 Mahayana 代码位于 `source/mahayana/mahayana-rs` 和 `source/mahayana/codex-rs`，而非旧说明中的根 `third_party/mahayana/`。其他历史路径必须逐项验证移动后的归宿。完整主仓范围首次在 Actions run `37954441267` 枚举为 9,321 个 tree entries、8,172 个非目录文件条目；这不是完整运行依赖闭包或全部语义已审计的证明。

逐文件登记来源与归宿，逐产品职责实现，不要求一对一文件复制。每个职责必须有 source anchor、唯一 owner、Android target、平台差异、状态机、失败行为、shipping wiring、正反向测试和 exact-HEAD 证据。机器枚举只证明范围，不证明已经理解或实现。

## 3. 固定架构决策

采用 Kotlin + Jetpack Compose 原生 UI、Kotlin Android 系统适配、Android 仓库自有 Rust Coordinator/Host/Runner 与业务核心，以及受类型约束的 JNI 桥接。保留既有模块的可用实现，但必须经当前 Desktop 基线复核。没有测量依据不得新增第二运行时。不得依赖另一个 Fabushi 仓库的源码才能构建；允许有许可证依据地导入到本仓库，随后由 Android 独立维护。

主调用链：Compose → Presentation → Typed Android Bridge → Android Platform Runtime → Mahayana Coordinator → Host → Android Local Runner / 已授权 Remote Runner。UI/ViewModel 不得直连 Host、直接写 canonical transcript、自行重试产生副作用的 tool call 或自行维护第二份账户真相。

保留 `frontend/`、`source/android-main/`、`source/android-preload/`、`source/mahayana-agent-coordinator/`、`source/host/`、`source/local-exec-daemon/`、`source/box-exec-daemon/`、`source/packages/`、`source/shared/` 等职责边界；Rust 的现有依赖闭包可在本仓库内保留有 provenance 的实际目录，不为了外观一致移动成循环依赖。MainActivity 只承担生命周期、Compose root、Intent、权限与系统 UI。最终删除被替代的 legacy 生产路径，禁止长期双轨。

WebView 只用于经过隔离的 Mini App / Web 内容；不是整应用 UI。系统 Browser/Custom Tabs 承担 OAuth，Credential Manager 承担受支持的凭据流程；密钥不进入 WebView 或公共日志。

## 4. 必须保留的产品职责

完整覆盖：账户/登录/会话/恢复/多账户隔离；Human 与 Agent/Bot 会话及其统一导航；联系人、群组、搜索、未读、草稿、消息动作与富文本；模型/provider/inference 路由、工具执行、审批、人类接管、流式响应、中止、工作流、自动化、记忆与多 Agent 协作；插件市场、安装/更新/卸载、连接器、MCP、OAuth、权限、Mini App/App Surface/WebMCP；附件、图库、文件、下载、语音录制、离线转写、播放、摄像头及桌面已经承载的其他媒体责任；远程设备、Computer Use、网关、配对与撤销；通知、深链、后台/前台、持久化、遥测、设置、可访问性、国际化；购买/权益/恢复等 main 实际存在的业务合同；签名、安装、升级、发布、安全和许可证。

这是覆盖域索引，不是认定每个列举的具体子功能都已在 Desktop main 或 Android 实现。最终 scope 由源码、真实入口、依赖闭包与职责分解联合确认。源码中存在但尚未 shipping 的内容必须明确标识；不得把未实现的桌面愿景描述成已交付能力。

## 5. 平台差异与不可伪造的保证

Android 的进程、后台执行、应用沙箱、权限、动态代码、商店分发约束不能通过文字承诺消失。按原生 API 实现移动端可等价的效果；桌面本地 OS 能力确实不能在 Android 沙箱中运行时，明确记录受限机制、用户可见差异、可行的本机替代或显式授权的远程执行路径及离线行为。不得把所有本地能力默认搬到服务器，不得将 core Agent/Tool/MCP/Streaming/Recovery 简单标 N/A。

要求 at-least-once 传输配合 idempotency/reconciliation，不能对不支持幂等的第三方副作用承诺 exactly-once。取消与完成竞争必须有唯一终态；不确定副作用必须标记 outcome-unknown 并核对，禁止盲重试。Android process death、Activity recreation、账户切换与失联后恢复必须恢复同一 durable run，而非重复发送。

## 6. 文档工程与验收

`docs/android-port/` 是可执行文档包：authority / architecture / contracts / features / implementation / verification / security / release / operations / templates / evidence。文件须给出可执行决策、输入输出、状态/错误、验收场景与真实源码定位，不得只有标题或 TBD。

逐文件 inventory 与 parity ledger 区分 `unreviewed`、`mapped`、`implemented`、`verified`。自动初始登记只能设为 unreviewed；历史 implemented/verified 不能跨新基线自动继承。`not-applicable-with-replacement` 必须有理由、替代责任、审核与证据。

所有测试、构建、包生成、文档校验均在 GitHub Actions 执行；不在 bhrum2 或 Mac 本地 build/test/package，不占用这些设备存储。可在本次会话沙箱编写文档，不在沙箱冒充运行产品测试。文档工作流可取得固定 SHA 的只读源码、生成完整 inventory、校验本地链接与数据合同、输出带 checksum 的文档/证据工件。文档 CI 通过不等于 Android 产品功能通过。

最终产品必须在同一 exact Android HEAD 上通过架构/合同/Rust/Kotlin/Compose/集成/安全/性能/打包与安装升级验收；保存真实 APK/AAB 来源、签名、摘要、设备/系统、workflow/run/job/step/artifact 与正反向行为证据。缺少真机、签名、生产授权或发布条件时标为 blocked，不降低门槛。

## 7. 本轮授权与完成边界

本轮执行任务是编写并提交完整迁移文档和其范围/质量校验工具，不是直接完成整个 Android 应用，也不是授权发布 APK、迁移密钥或修改其他仓库。继续现有 PR #3；不覆盖 main、不删除既有实现、不合并未独立验收的产品代码。

遇到必须用户介入的权限/账号/凭据卡点时，先检查 Gmail 是否已有同主题未解决通知；只向 `1315518325@qq.com` 发送必要且不含秘密的通知。卡点不阻止其他可推进的文档、源审计与 CI 工作。


## 8. Generated Subagent durable ownership and production cutover

The Android Host/Runner must preserve the current Desktop shipping generated-subagent responsibility as a single durable Rust-owned state machine. The source anchors for the current Desktop baseline are `source/host/src/runner/subagent_runtime.rs`, `source/host/src/runner/tools/sand_task_subagent_tool.rs`, `source/host/src/runner/tools/sand_subagent_management_tools.rs`, and their generated-subagent production-cutover/runtime-adapter contracts.

The Android implementation must not reduce this responsibility to a UI state enum. The Host/Runner owner must persist a stable subagent request identity derived from the parent tool call; parent request, root-parent request, parent Agent and tool-call lineage; account fence; process epoch; pending wake; steering/abort intent; session snapshot; terminal projection; and computer-use usage/audit material needed for recovery. Task dispatch, CheckSubagent, MessageSubagent and StopSubagent must enter through the shipping Host tool graph and then the Coordinator/JNI production composition. Compose and ViewModel may project immutable state only and must never own a second subagent registry.

Process death or Host reopen must not replay an uncertain child side effect. Active children recovered without a proven terminal result become `outcome-unknown` and require reconciliation. Account switch fences children from the prior account. Old-epoch and stale callbacks are rejected. Duplicate Task identities are idempotent only when their frozen launch identity matches; mismatched reuse fails closed. Abort wins a cancellation/completion race, parent-scoped cancellation targets only that parent's running children, pending wakes are disarmed exactly once, and a steering message causes a continuation with the same stable subagent request identity instead of creating a second child.

Production verification is exact-HEAD only. Focused Rust/contract/integration coverage must include success, failure, duplicate launch, steer continuation, abort, parent cancellation, process reopen, account fence, stale callback, outcome-unknown reconciliation, computer-use usage/audit and Task/CheckSubagent/MessageSubagent/StopSubagent bridge wiring. Responsibility ledger entries remain `not-verified` until the same Android exact HEAD has the source implementation, shipping production wiring, GitHub Actions evidence, final APK/AAB provenance and required device/protected-account acceptance. The full-source strict closure gate remains fail-closed and may not be weakened to make this slice green.

Per-turn generated-subagent type exposure is part of the shipping security boundary. Android Coordinator overwrites any presentation-supplied capability projection with its trusted process/platform snapshot before `chat.send` or direct subagent-tool dispatch reaches the native Host. The Host validates that projection before any durable turn/subagent mutation, derives the exact `Task.subagent_type` enum through the Desktop-equivalent `build_turn_subagent_types` rules, and freezes the allowed type set into child launch identity. Missing or malformed projection must not elevate beyond the minimal path; generated children never inherit Task delegation. `computeruse` and `browseruse` stay unavailable until a real authorized Remote Runner/desktop capability owner is wired for that turn; UI-declared booleans are never authority.
