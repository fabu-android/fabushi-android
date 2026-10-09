# Fabushi Desktop → Android 原生实现手册

版本：2026-10-09。文档状态：实现规范；产品状态：未宣告完成。

这是交给主实现工程师、模块实现者及独立验收者的同一套合同。目标是把 Desktop canonical main 的全部适用职责实现到 Android，而不是照搬 Electron、把主界面放进 WebView、生成同名空文件，或只做到聊天 Demo。

## 阅读顺序

先读 [总规范](../specs/desktop-main-android-full-parity.md)、[来源政策](authority/source-policy.md) 和 [实际发现](authority/discovery.md)，再读 [系统架构](architecture/system.md)、[目录职责](architecture/repository-layout.md)、[生命周期](architecture/lifecycle.md) 与 [平台差异](architecture/platform-deltas.md)。实现任何领域前，必须读该领域文档及其引用的固定 SHA 源码。实际运行时合同优先于文件名和旧文字描述。

## 文件导航

| 层次 | 文件与用途 |
| --- | --- |
| 权威 | [baseline.json](authority/baseline.json)：机器可读源锁；[source-policy](authority/source-policy.md)：完整范围、rebaseline；[discovery](authority/discovery.md)：已核实事实与缺口 |
| 架构 | [system](architecture/system.md)、[repository-layout](architecture/repository-layout.md)、[ADR](architecture/adr-001-native-standalone.md)、[lifecycle](architecture/lifecycle.md)、[platform-deltas](architecture/platform-deltas.md) |
| 合同 | [runtime](contracts/runtime.md)、[storage](contracts/storage.md)、[bridge](contracts/bridge.md)、[backend-compatibility](contracts/backend-compatibility.md) |
| 功能 01–04 | [账户](features/01-account.md)、[消息](features/02-messaging.md)、[Agent](features/03-agent.md)、[工具与审批](features/04-tools.md) |
| 功能 05–08 | [MCP/连接器](features/05-mcp.md)、[Mini App](features/06-miniapp.md)、[文件与媒体](features/07-media.md)、[远程电脑](features/08-remote.md) |
| 功能 09–12 | [商业与权益](features/09-commerce.md)、[导航与原生体验](features/10-navigation.md)、[工作流与自动化](features/11-automation.md)、[设置与通知](features/12-settings.md) |
| 执行 | [阶段依赖](implementation/phases.md)、[任务队列](implementation/task-queue.md)、[工具链](implementation/toolchain.md)、[切换与回滚](implementation/cutover.md) |
| 验证 | [完成定义](verification/acceptance.md)、[端到端场景](verification/journeys.md)、[CI 合同](verification/ci.md)、[性能](verification/performance.md) |
| 安全/发布 | [威胁模型](security/threat-model.md)、[来源与许可](security/provenance.md)、[分发](release/distribution.md) |
| 接力 | [主实现与独立验收提示](operations/continuation.md)、[卡点登记](operations/blockers.md) |
| 模板 | [职责记录](templates/responsibility.md)、[平台替代](templates/platform-delta.md)、[证据记录](templates/verification-record.json) |
| 证据 | [证据规则](evidence/README.md)、[外部与源码来源](sources.md) |

## 如何依此实施

从 `implementation/task-queue.md` 的最早可推进职责开始。先把真实源路径、调用者、状态与副作用填入职责记录，写出 shipping 路由和反例，再实现 Android 自有代码。每个提交更新对应 ledger，而不是用整目录粗略打勾。依赖未解除的任务登记 blocker 后，切换到依赖已满足的任务；不能绕过权限或降低最终标准。

`.github/workflows/android-port-docs.yml` 只校验文档与源范围，在 Actions 读取 Desktop 固定提交，生成完整 tracked-file inventory 与初始 `unreviewed` ledger，检查文档链接、source anchor、schema、tree 完整性并归档。它不编译 Android 产品、不读取生产秘密、不发布应用。完整产品的 CI 要按 `verification/ci.md` 建立并独立验收。

`generated/` 为机器生成文件；其存在证明文件被纳入范围，不证明每一行源码已被人工理解。逐职责审计、wire contract 精确提取、生产入口接线与真机验收仍是实现阶段的硬门，不能用这本文档代替源码或实测。
