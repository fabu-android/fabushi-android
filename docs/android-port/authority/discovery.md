# 本轮实际发现与尚未证明的内容

## 当前 authority lock — 2026-10-18

bhrumom/fabushi-desktop@main = `12887d2a44ba450b13ca8390066a39418c52e0c9` (root tree `4bc59314edd76a51bd8f0e2d0654935d9e8ffb41`). GitHub recursive tree: 9,431 entries / 8,276 tracked non-tree entries, `truncated=false`. Exact path/mode/type/blob reconciliation against the immutable raw inventory candidate has zero differences. Android migration PR #3 was read at `84318e1cd5fa8497d49b1c8309f1f25acf0db560` on branch `spec/grok-bot-018-android-parity-20260922`; this is candidate-input provenance, not evidence for any later commit.

Raw inventory candidate SHA-256: `2b821bc55738c6d96b3a16f7144854eaefe982b7b03856b97de7b4695c082ce0`. The separately linked inventory ledger SHA-256: `84f65fcb92803c0eaaadc8ffcf16840534553c76af08b54ddb3699e328846392`. The 306-item responsibility ledger has 6,089 Desktop anchors over 5,891 unique paths; all anchor source commits and blob SHA values are rebound to this Desktop authority. Four stale blob anchors were corrected. Bidirectional links are complete for the 5,891 already anchored paths; 2,385 inventory paths remain unassigned and closure status remains incomplete. All 8,276 inventory rows remain `unreviewed`; all responsibility verification remains `not-verified`. No prior Actions/package/device evidence is inherited.

Any older Desktop SHA mentioned below is predecessor/historical provenance only; it is not current authority or exact-HEAD verification evidence.

## 2026-10-15 Desktop authority rebaseline

Historical predecessor: Desktop canonical main `8148bfb9e747d446dd3d97342c3cf04a0ed407a2`, root tree `daa4cfbf6851e312e280050f81ef18915bca0c0c`.. Relative to the prior locked `b6a66fd2b6aa1619565e3d25015bbbf0b4b8263d`, GitHub reports six commits and seven changed paths: three new G11 real-browser workflow/evidence files plus changed `sand_browser_driver_source.rs`, `sand_browser_tools.rs`, and their two focused contract tests. Recursive authority is 9,364 tree entries / 8,213 tracked non-tree entries, not truncated.

The incremental rebaseline preserves human review state only for unchanged path+blob rows, updates their `source_commit` to the new authority, resets the four changed existing rows to `unreviewed`, and adds the three new paths as `unreviewed`. Older Desktop-SHA evidence remains predecessor evidence and cannot verify the new blobs.

## 2026-10-11 当前权威 rebaseline

本会话最新一次重新读取 GitHub 后，历史 predecessor：Desktop canonical main `371b72f1cd20de03dc4dff9a15ed761054968093`，root tree `3116c54a1e36da95189c740bdc789ad543ce2d20`。；Android main（historical baseline）为 `59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`，迁移继续使用 PR #3。Desktop 相对上一锁定的 `62d0c99f95d83f0c7c135a802a31c8b2fcf2cc8d` 仅 ahead 1 commit；compare 只修改 `docs/specs/fabushi-plugins-marketplace-owner-inventory.md`（+32），没有 shipping source/code 路径变化。

当前 Desktop recursive tree 仍为 9,326 tree entries、8,177 non-tree tracked entries；`source` 7,607、`frontend` 344、`projects` 89、`native` 46、`desktop` 45、`docs` 14。8177 个 source rows 按 path+blob 做增量 rebaseline：除 owner-inventory 新 blob 外其余路径/blob 不变，因此保留已有人工 review 状态但将 source_commit 更新为当前 authority；owner-inventory 重新语义审计，不继承 verified。

本次 owner audit 进一步锁定 Skills：Host `plugin_skills.rs` 拥有 lifecycle/sync，`plugin_skills_cache.rs` 持久化当前 user/skill/auth-block 投影，`skill_publish.rs` 拥有 team publish/resync/unpublish ownership fencing；Electron loader 只是 marketplace SDK/materialization edge。Canonical Registry 迁移必须保留 legacyPluginId/version-or-commit/relativePath/workflow identity/publisher-team migration facts，并显式去重；legacy publisher facts 不得自动升级为 verified publisher。Android 当前通用 Marketplace/plugin runtime 不能据此被误报为 Skills lifecycle parity。


发现日期：2026-10-09。全部 SHA 见 `baseline.json`。

## 已读取的事实

历史 predecessor：Desktop main `3bc92400826cc4ca7ac665b467708e22261edc61`。。Android main 为 `59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`，已有 PR #3，修改前 HEAD 为 `5d049353e167d9b91b5c765e1686f1ac8670f2ff`。因此继续现有分支而不复制另一套迁移。

Android 原有主分支 AGENTS 仍提共享平台核心，PR 分支已经明确 standalone ownership；本任务采用 PR 分支规定并以 Desktop main 代替历史 Grok 为产品权威。旧文档含 `bhrumom/fabushi-android` 名称，实际目的仓库是 `fabu-android/fabushi-android`，根 AGENTS 已据此修正并增加当前文档入口。

已读取 `mobile/android/app/build.gradle`：这是 Groovy 文件，不是 `.kts`；applicationId/namespace 为 `com.ombhrum.fabushi`，minSdk 26，compileSdk/targetSdk 37。Compose/平台 Kotlin 已通过 sourceSets 指向 `frontend/src/main/kotlin`、`source/android-main/src/main/kotlin`、`source/android-preload/src/main/kotlin`。存在 release、githubRelease、ciAcceptance 三类配置，不能重建一个与现有源脱节的 app 目录。

已读取 root `Cargo.toml`：包括 source/shared、internal、mahayana-agent-coordinator、host、android-host-jni、local-exec-daemon、box-exec-daemon 和 agent-core/agent-kv/agent-transcript/chat-inference-proto/context/constants/mcp-core/redaction/utils。存在 workspace 不代表这些组件已经达到 Desktop 当前语义。

已读取 Desktop MCP `bridge.ts`：私有 MessagePort、pluginInstanceId、每次 load nonce、grants、ready、pending 和 dispose reject 是必须迁移的实际语义。已读取 `mahayana-host-features.ts`：登录、runtime ready、聊天、Mini App 安装/打开、能力审批、中断及清会话有共享 journey 类型。

## 已取得的完整主仓范围

Actions run `37954441267`、attempt 1、source-inventory job `113901245894` 在 Android checkout `7117a8da27c9abfa5b1cec8dab355775c7dfa5ac` 实际成功。开始/结束读取的 Desktop main 均为固定 SHA；root tree 匹配；9,321 个 tree entries、8,172 个非目录条目；没有 truncated、gitlink 或 symlink。14 个入口的内容通过 Git blob SHA 校验。

其中 source 7,606、frontend 344、projects 89、native 46、desktop 45，其余根目录和文件详见 Actions 工件。初始 ledger 全部 unreviewed，SHA-256 为 `213bb7d476283dbdbdef13267459ea7c7e58f62ff18f58f57122b28510e3d3ba`。这证明完整主仓被登记，不证明所有源码已阅读、LFS/动态依赖已闭包或功能已迁移。

本次历史证据 artifact `11627207949` 含文档与清单，artifact ZIP digest 为 `89a5c45284bc0a3cf9f5ec3d8d0a610d99b9b69f1cf580117b964f214160b3e7`。该记录仅对应 `7117a8da...`；后续 HEAD 需要新 run，不能拿此历史工件声明后续文档或产品已通过。

## 发现的文档/发布风险

Desktop `RUST_RUNTIME_SOURCE.md` 的根 `third_party/mahayana/*` 是历史路径。已读 `desktop/package.json` 明确实际 build:host 指向 `source/host/app/Cargo.toml`、`source/node-agent-coordinator/Cargo.toml`、`source/box-exec-daemon/Cargo.toml`；已读 `source/mahayana` 树显示 `mahayana-rs` 和 `codex-rs` 在本目录下。迁移以实际 Cargo/打包入口为准，不按过时文字复制旧根。

Desktop `DESKTOP_SOURCE_CLOSURE.md` 列出的 `frontend/apps/web/src/lib/mahayana-host/contracts.ts` 在固定 commit 读取返回 404。这说明旧来源说明不能直接当当前文件清单；应从真实树追踪其移动/替代路径，禁止继续引用失效路径作为已读证据。

当前 `mobile/android/app/build.gradle` 已重新核实：defaultConfig=false、release 继承 false、githubRelease 明确 false，只有 ciAcceptance=true。历史 githubRelease 测试账号导入风险已在配置层关闭；仍需在最终 minified githubRelease APK/AAB 上做 R8 后二进制/运行行为验收，不能只凭 BuildConfig 声明安全完成。

历史 run 的 documentation-contract job `113901245494` 当时确因缺少 05–12 八份手册而失败；错误报告 artifact `11627626106`。2026-10-10 已继续使用当前 PR #3 的正式 GitHub 写入接口补齐 05–12，且再次读取仓库文件列表确认 01–12 全部存在。历史失败保持历史失败；新提交尚须取得 exact-HEAD 文档校验成功后才可宣告文档 CI 通过。

## 事实、设计与待证据的区别

文档中的 owner、状态机和错误处理是新迁移规范；不是对当前 Android 已实现状态的断言。范围清单由 Actions 从真实 Git tree 生成；函数级合同需要实现者阅读源和 tests 后建立映射。路径不存在必须定位替代，不能新建同名空文件掩盖缺失。

本轮未证明：全部 Desktop 文件已逐行理解、全部 wire RPC 已精确提取、全部 Android 已有模块当前行为、既有产品 CI 全绿、真机安装升级、生产登录、Play 签名与发布权限。相应工作由台账、阶段与验收门约束，不标完成。
