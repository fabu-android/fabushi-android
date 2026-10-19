# 权威、完整范围与逐源审计

## 范围的算法

取得 Desktop `main` commit → Git commit 的 root tree → recursive tree，确认 `truncated=false`。API 返回 truncated 时逐子树遍历并保留相对路径；不得把截断结果当全量。每个 blob、symlink、gitlink 均在 inventory 中保留 path、SHA、mode、type。另保留目录树及全部 API 返回数据。gitlink 不是已展开的源码；必须登记其仓库、固定 commit、许可、需要的递归闭包。LFS 指针不是原始资产；记录对象 OID/size 并在发布前校验实际资产。

范围没有 `source/frontend` 白名单。Rust 原生依赖、测试、配置、锁文件、安装资源、CI、安全策略、根文档均需 disposition。生成物和历史文档可以被判定为非生产源，但需理由，不可静默丢弃。旧闭包文档只作线索，当前树和真实 import/Cargo/Gradle/package graph 才证明存在与可达性。

## 两级台账

一级：文件范围表，精确覆盖所有 tracked leaves。自动生成行只能 `unreviewed`。二级：职责表，一个文件可拆多个责任，多个文件可共同证明一项责任；必须保存 source symbols/line ranges、caller/callee、唯一状态 owner、用户效果、Android targets、依赖、平台 delta、实现状态、生产和测试证据。不得以文件数量或相同目录名计算“功能完成百分比”。

合法状态为 `unreviewed → mapped → implemented → verified`。合法 disposition 为 `direct-port`、`android-adapted`、`not-applicable-with-replacement`。`mapped` 需要读完源职责与调用链；`implemented` 需要生产代码及实际入口 wiring；`verified` 需要同一 Android exact HEAD、Desktop 基线和完整验收证据。`verified` 不是“测试文件存在”。

`not-applicable-with-replacement` 只排除桌面机制，不随意删用户能力。必须写明无法使用的 OS 机制、原生/远程替代、可见差异、无权限和离线行为、审核人、测试。没有替代的核心功能仍为 gap。

## 基线改变

实施开始和最终验收结束分别读取 main。改变时取得旧、新 commit 的完整树，按 path+blob SHA 比较 added/modified/deleted/type-changed；compare API 若分页必须读全。重命名不以名称猜测语义。追踪受影响的职责、依赖、数据迁移和 tests，保留旧记录为历史，不能继承新的 verified。全文件清单与模块依赖同时更新后才建立新 evidence bundle。

Android PR #3 是当前工作承载，main 是目标合并点；不得借用旧 HEAD CI 作为新 HEAD 结论。上游 open PR 可观察，不自动加入 main 合同，也不阻止独立职责先实现。
