# 安全边界、威胁模型与发布阻断

状态：实施要求，不是已完成的安全审计。责任人：Android 安全维护者；执行者：各模块 owner；验收独立于实现者。

## 资产与信任分区

资产包括账户令牌、用户消息与附件、工具权限、远程设备授权、付款权益、持久化密钥、更新签名、工作流凭据和用户设备资源。Compose 是呈现层而非权限权威；第三方 Mini App、模型文本、连接器返回、导入文件、二维码和网络事件全部属于不可信输入。Provider/MCP 的 readOnlyHint 等声明不是授权证明。

唯一授权入口在 Android-local CapabilityBroker/Host policy。规则至少包含 accountId、sessionEpoch、capability、resourceScope、operation、expiry、decisionId。UI 上的审批只有经 owner 原子消费后才能触发副作用；账户切换、撤销、超时或设备解绑必须立即失效。

## 边界及验收

| 边界 | 威胁 | 必需控制 | 必须实际失败的反例 |
| --- | --- | --- | --- |
| 登录与 Intent | 回调劫持、重复回调、账户混淆 | state/PKCE、issuer/redirect 校验、单次消费、epoch 隔离 | 旧 state、错误 host、跨账户 callback |
| Kotlin/JNI | 释放后回调、错误句柄、无界 payload | typed schema、长度上限、generation、明确 buffer 所有权、关闭幂等 | close 后 callback、超大事件、重复 release |
| Mini App | 非授权工具、旧页面调用、任意导航 | instance/nonce/grants、origin 校验、session-bound pending、dispose 取消 | 旧 nonce、缺失 grant、hosted 页面继承本地权限 |
| 文件 | 越界路径、URI 越权、解压耗尽 | 系统选择器、URI grant、归一化、大小/展开比配额 | `../`、符号链接逃逸、撤销后的 URI |
| 本地执行 | 任意权限提升、绕开审批 | app sandbox、操作 allowlist、资源/时间配额 | 无批准执行、未声明 capability |
| 远程设备 | 重放、撤销后调用、串号 | 明确配对、目标标识、消息关联、会话撤销 | 旧授权、另一设备 response |
| 更新 | 篡改、降级、渠道交叉污染 | 签名校验、versionCode、渠道策略、用户确认 | 错误证书、哈希不符、非预期包名 |
| 证据与日志 | 令牌/用户内容泄露 | 采集前脱敏、最小范围、保留期、日志负例测试 | 含 bearer、cookie、支付令牌的日志 |

## 密钥、数据与遥测

Android Keystore 保存或保护应用密钥；业务数据可按风险模型加密。强制区分凭据撤销与仅本地退出。备份规则排除不可迁移的密钥材料，恢复后重新认证而非伪造有效会话。崩溃报告和性能报告只记录必要的技术标识，不默认附带完整对话、附件、屏幕或账号信息。

生产包不得含测试账户导入入口、调试调度器、未受保护的导出组件或万能工具开关。当前 discovery 中 githubRelease 开启 CI_ACCOUNT_SESSION_IMPORT_ENABLED，属于待核实/整改的发布风险，不能仅因历史 CI 通过就放行；需检查实际代码路径与 release manifest，发布前证明测试入口已关闭或移除。

## 交付要求

每个风险绑定功能职责、滥用用例、真实 Android 测试和处置结果。重大风险未闭合即阻断发布。秘密只进入受保护的 GitHub Actions Environment，不写进仓库、文档、工件或聊天。不得为通过测试降低 TLS、授权、商店规则或系统权限控制。

Android 官方资料见 [来源](../sources.md) 中 WebView、Keystore、应用链接和后台执行条目。