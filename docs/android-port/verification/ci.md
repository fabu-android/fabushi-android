# GitHub Actions 与证据合同

## 文档工作流

`android-port-docs.yml` 仅处理文档范围和质量：checkout 精确提交，读取 baseline，取得固定源完整树，生成库存及初始 unreviewed ledger，校验链接/必需结构/源锚点，生成报告与 checksum 工件。不得把这条绿色 check 命名为“Android 全功能验收通过”。无生产 secrets、无 Android 编译、无发布。

## 产品工作流目标

1. source-authority/architecture：当前源锁、完整库存、责任台账、依赖边界、无另仓源依赖。
2. rust-contract：锁依赖、workspace tests、DTO/状态机、取消/恢复/持久化与消息/工具合同。
3. android-build-unit：固定 JDK/SDK/NDK/Kotlin/Gradle、正式 ABI JNI、Kotlin/Compose unit、lint。
4. instrumentation：模拟器实际 app，导航/消息/模型/审批/bridge/进程重建；保存测试报告和必要截图。
5. protected-journey：受保护环境里最小权限测试账号，真实服务和 complete-state；最后清理凭据/会话。
6. packaged-acceptance：同源 minified APK/AAB、签名与 ABI 检查、真机安装升级、性能和 release surface。
7. independent-review：逐验收项核对证据，结束再查 Desktop main。

这些是需实施的工作流合同，不声称当前仓库已有同名 job。先读取现有 workflows 复用有效部分，不能建两套互不关联的 gate。

## exact HEAD

PR workflow 明确 checkout head.sha，不能把 merge ref 测试冒充 head 测试；两者若都需要分别记录。artifact 包含 Android SHA、Desktop SHA/root、工具链、命令、配置/variant、测试数量、设备/OS/ABI、开始结束时间、run/attempt/job 和 digest。未来签名包包含原生库与模型来源。job success 之外还检查步骤有执行、断言非零、未跳过和 artifact 完整性。

文档库存生成造成的新提交与测试输入提交分开标识，不能造成自引用“自身 SHA 写入自身”。生成报告注明 inputs_sha，产品验收仍绑定实际产品 HEAD；绝不从文档生成提交继承产品通过结论。

## 安全与失败

PR 只读权限；有写入的工作流仅可信分支并检查预期 HEAD。不要在 pull_request_target 执行未审代码或给 fork secrets。actions 和下载工具固定可审计 revision/checksum；签名仅 protected release。上传日志先去敏。确定性失败修正真实 source/test/workflow 后用新 HEAD 取证，不改成 continue-on-error。
