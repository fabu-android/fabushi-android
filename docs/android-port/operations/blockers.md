# 卡点与决策记录

状态只允许 open / resolved / obsolete；每项有 owner、受影响范围、解除条件和下一次检查点。

| ID | 状态 | 影响 | 解除条件 | 可并行内容 |
| --- | --- | --- | --- | --- |
| DOC-WRITE-001 | resolved | 历史部分批量写入受安全检查拦截；2026-10-10 已通过原仓库授权的常规 GitHub 内容写入，补齐 05–12 八份正文 | 已在 PR #3 当前分支确认全部 12 份手册存在，等待 exact-HEAD docs CI 再验证 | 下一步继续 source-to-target 职责审计、产品实现和验收 |
| PRODUCT-BASELINE-001 | open | 旧 Grok 级 ledger 不能证明当前 Desktop 全量 parity | 完整 inventory 与逐职责复核、production wiring + exact HEAD evidence | 已确定域的源审计与实现规划 |
| RELEASE-001 | open | 真机、签名、商店、授权账户证据本轮未取得 | 受保护 Actions 环境及云设备完成真实验收 | 不依赖生产凭据的合同和静态检查 |
| CI-AUTH-001 | open | Android emulator protected-account instrumentation 在准备 session 前因 `FABUSHI_CI_TEST_USERNAME` / `FABUSHI_CI_TEST_PASSWORD` 为空而失败；不影响无凭据 Rust/Gradle/静态/包级工作 | 在仓库或受保护 Environment 配置两项 Actions secret，并由新 exact HEAD 重新运行 emulator journey | Rust、架构、release package、安全门、source/responsibility audit |

DOC-WRITE-001 曾向用户指定邮箱通知，threadId `1a1215529c220244`；目前正常 GitHub 写入已完成，已核实 PR #3 的 `docs/android-port/features/01–12` 全部存在。后续不能把历史 safety-block / CI 失败错误地当作仍未入库；但只有新 exact-HEAD Actions 全绿，才能将文档 CI 标为通过。无需对此主题再次发送同样的邮件。

CI-AUTH-001 已于 2026-10-10 向用户指定邮箱发送一次必要通知，threadId `1a1218d568460e79`；邮件不包含秘密，并明确要求不要通过邮件发送用户名/密码。后续先检查该线程/Actions 实际状态，未解决时不得重复发送同主题邮件。

本记录不声称上述项目是全项目仅有卡点。主实现必须持续登记新发现，并把 `blocked` 具体关联到责任和依赖，不得以一个全局 blocked 停止全部工作。

## CI-AUTH-001 — protected Android emulator account secrets

Current exact evidence at Android HEAD 140a971b7bb7cc32ed657b868a1c42fad6da288a:
- Full CI run 37967638680 package job 113945895073 completed successfully, including debug/ciAcceptance/githubRelease APK, githubRelease AAB, unit test, lint, release CI-marker stripping, SHA-256 identity recording, and artifact 11634950184.
- dependent emulator job 113949114567 downloaded and verified those exact-HEAD packages, then failed before emulator startup because FABUSHI_CI_TEST_USERNAME and FABUSHI_CI_TEST_PASSWORD were both empty.
- Gmail thread 1a1218d568460e79 already notified 1315518325@qq.com; as of this update it has no reply confirming configuration. Do not send a duplicate notice.

This blocks only protected authenticated emulator/device journeys. It does not permit weakening package, architecture, source-closure, Rust/Kotlin, release-security, or unauthenticated gates; continue all independent work.


## RELEASE-SIGNING-001 — GitHub release signing secrets unavailable

- Status: **blocked / user-action-required**.
- Current exact-head observation: Android Parity Full CI run `38051418454`, job `114211145593`, on `a82bba3cab2d234391f4f1f6ed5ec707a0402cb3` stopped at mandatory release signing preparation after the arm64-v8a/x86_64 Rust/JNI release builds completed.
- Missing protected Actions secrets: `ANDROID_RELEASE_KEYSTORE_BASE64`, `ANDROID_RELEASE_KEYSTORE_PASSWORD`, `ANDROID_RELEASE_KEY_ALIAS`, and `ANDROID_RELEASE_KEY_PASSWORD`.
- Product rule: do not weaken the release gate, substitute a CI test key, or allow unsigned/public `githubRelease` artifacts to count as production acceptance.
- Notification: the same blocker was already emailed to `1315518325@qq.com` in Gmail thread `1a124e8a981230ff`; do not send duplicate mail while it remains unresolved.
- Parallel progress: source/responsibility closure, Rust/architecture work, and other non-signing tasks remain ready and must continue.
