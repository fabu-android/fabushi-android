# Android 打包、安装升级与分发

## 渠道与身份

保留现有 `com.ombhrum.fabushi` applicationId，除非用户明确批准迁移身份。Play、GitHub、自测渠道的版本、签名、更新入口和测试能力必须明确区分。当前 build.gradle 含 release、githubRelease、ciAcceptance；它们只是现有配置，不是本轮已经通过的发布结果。

release 与 githubRelease 共享同一个产品能力合同，平台/商店不允许的机制必须按平台差异记录交付替代效果。ciAcceptance 只能用于受控验收；不可上传到公开应用分发渠道。禁止把测试 session 注入接口带入面向用户的包。

## 可复现的 Actions pipeline

以完整 Android SHA checkout，验证 Desktop authority，固定 JDK/SDK/NDK/Gradle/Kotlin/Rust 和依赖锁。先运行源码/架构/合同检查，再运行 Rust 与 Kotlin tests、lint、Compose/device tests、release shrinker 构建及打包。只允许在 GitHub Actions 及由其调度的已批准云设备上构建测试。

所有 native ABI 单独构建并验证；首要 release ABI 为 arm64-v8a，模拟器覆盖 x86_64。是否发布 32 位 ABI 须由设备覆盖和全部 native 依赖能力决定，不得虚报支持。核查 ELF LOAD/RELRO 与 APK ZIP alignment，并在 16 KB page-size 环境启动实际包；官方要求见 [来源](../sources.md)，发布日期前复核政策，不能照抄旧截止日。

## 签名与证据

签名只在受保护的 Actions Environment 执行。证据记录 applicationId、versionName、versionCode、Git SHA、Desktop SHA、toolchain、ABI、证书指纹、APK/AAB SHA-256、workflow run/attempt/job/artifact 和相关测试工件。私钥、密码、完整 tokens 不进入证据。

先验证 unsigned assembly 的可构建性，再验 signed artifact；两者不能互相替代。UI 测试在 ciAcceptance 成功不代表经过 R8/resource shrink 的 release 成功，必须对用户最终包执行关键功能 smoke 与安装/升级验收。

## 安装与升级矩阵

至少验证干净安装、同签名升级、数据库历史版本迁移、仍有活动 run 时升级、空间不足、网络中断、旧 schema 恢复失败和错误签名拒绝。升级后账户隔离、历史消息、草稿、插件权限、附件引用与未结束运行的结果可核对；不能通过清数据规避迁移。

Play 发布要取得最新政策、target API、计费、Data safety、账号删除和权限声明的证据。当前源码声明 compileSdk/targetSdk 37、minSdk 26，不等于 SDK 依赖组合已可构建，也不等于商店已批准。最终版本以 Actions 和发布日官方要求核实。

GitHub 侧更新必须验证预期包名、证书、摘要和单调版本，并经用户系统安装确认，不使用后台静默更新。回滚优先暂停分发、服务端兼容和前向修复；不得强迫旧二进制读取已经不兼容的数据库。

## 放行

发布需要独立审批和用户明确发布授权。本轮文档任务不授权公开发布、上传商店、轮换密钥或更改支付配置。所有检查 completed/success、真实关键旅程通过且未关闭重大问题清零后，才可声明候选可发布。