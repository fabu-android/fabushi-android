# F01 账户、登录与会话

## 范围与 owner

源入口：Desktop `source/product/fabushi/fabushi-account-service.ts`、`fabushi-account-policy.ts` 及其生产调用者。Android Platform Auth Adapter 管系统认证资源；Coordinator 管账户 epoch；Host account service 管账户业务。Compose 只消费统一 projection，不维护另一套登录真相。后端字段按 [兼容合同](../contracts/backend-compatibility.md) 从固定源提取。

## 实施合同

账户状态为 anonymous → authenticating → hydrating → ready；过期进入 reauth-required，退出经过 revoking。身份已显示不等于 runtime ready。移植主线实际支持的登录、注册、找回、资料、设备会话与账户删除流程；未找到的入口不能假定存在。

系统 Browser/Custom Tabs 处理网页认证，回跳必须关联同一次认证事务与账户，并校验协议要求的 state、PKCE、nonce 和目标来源。重复回跳只消费一次，取消不改变原账户。凭据由平台安全存储持有，不进入 UI 状态快照、网页存储或通用日志。并发请求共用一个刷新 owner；刷新失败展示明确登录状态，不无限重试。

退出先提升 account epoch，阻止旧异步结果，关闭相应连接与订阅，处理进行中的任务，再清除按政策需删除的凭据/缓存和界面。多账户能力以主线为准；支持时所有记录和附件都必须分区。Passkey 使用平台支持的凭据流程；缺少设备支持时提供主线允许的其他方式，不能返回模拟成功。

## 故障与验证

测试正常登录到真实 Host ready、取消认证、重复回跳、刷新中切换账号、离线冷启动、设备锁定、进程重建、会话被服务端撤销。断言旧账户回调不能改变新账户、退出后旧授权不能继续执行。账户删除需明确确认；远端失败时不能只在本机假装删除。

受保护验收使用专用、限权测试账户。测试会话导入与公开发行包隔离；当前 githubRelease 配置需按 [发现记录](../authority/discovery.md) 审计，不能因测试通过就视为生产安全。
