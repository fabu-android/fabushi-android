# F01 账户、登录与会话

## Current authority

bhrumom/fabushi-desktop@main = `12a4e29819592e3a7e32f7435cb0cf2e3cc53848` (root tree `72fc5f777012f6e7bf5f5f471b6f121fd3612bb2`). Any older Desktop SHA mentioned below is predecessor/historical provenance only; it is not current authority or exact-HEAD verification evidence.

## 范围与 owner

历史 source fact `371b72f1cd20de03dc4dff9a15ed761054968093` 已失效；当前 Desktop authority 固定为上述 `12a4e298.../72fc5f77...`，本文件中的具体 source anchor 必须以该 exact tree 重新核对。认证主入口是 `source/electron-main/account/cursor-auth.ts`（blob `c2e4c95306f8822adb22176d594b488e9717b28f`）及其真实生产调用者，不再把历史 `source/product/fabushi/fabushi-account-service.ts` 路径当作当前 authority。

Desktop 的 `SandCursorAuthService` / `SandBackendLoginManager` 负责登录尝试、凭据状态、刷新、资料 hydration、退出与账户拒绝撤销。源码事实包括：认证操作 epoch 防旧回调、credential mutation 串行化、同 refresh token 的并发 refresh 合并、refresh rotation rescue、登录取消、受保护存储 settle、失败/恢复/退出 telemetry，以及退出删除失败时禁止继续使用已撤销凭据。网络或响应解析失败与明确 session revoke 是不同状态，不能把“刷新暂时失败”直接等同于“durable session 已撤销”。

Android Platform Auth Adapter 只管系统 Browser/Custom Tabs 与受保护存储；Coordinator 管 account epoch 和跨 owner cleanup；Rust Host `AndroidAccountService` 管 canonical account/session 业务真相；Compose 只消费 projection，不维护第二套登录真相。生产路径固定为 Compose/Presentation → AndroidCoordinatorRuntime → `MahayanaHost` → JNI → Coordinator → Rust Host。后端字段按 [兼容合同](../contracts/backend-compatibility.md) 从固定源提取。

Android 对 Desktop 并发 auth/refresh 的平台适配是单一 process-owned Host：`MahayanaHost.SharedHost.lock` 串行所有普通 `nativeDispatch`，因此 `AndroidAccountService` 本身就是唯一 refresh owner，不额外创建 Kotlin refresh registry。若将来把 account I/O 移出该串行边界，必须恢复显式 request/operation epoch 与 single-flight refresh，并在迁移前补合同。

## 实施合同

账户状态为 anonymous → authenticating → hydrating → ready；过期进入 reauth-required，退出经过 revoking。身份已显示不等于 runtime ready。移植主线实际支持的登录、注册、找回、资料、设备会话与账户删除流程；未找到的入口不能假定存在。

系统 Browser/Custom Tabs 处理网页认证，回跳必须关联同一次认证事务与账户，并校验协议要求的 state、PKCE、nonce 和目标来源。当前 Android browser-attempt 使用 Host 持久化的 attempt identity + poll secret，Custom Tabs 只接收服务端返回且通过 exact-origin allowlist 的 URL；这些字段不能进入 Compose canonical state。重复回跳/轮询只消费对应尝试，取消不改变原账户。

凭据由平台安全存储持有，不进入 UI 状态快照、网页存储或通用日志。Rust Host 返回私有 `_accountSessionMutation`，Kotlin `MahayanaHost` 在 Presentation 不可见的边界将 Save/Clear 应用到 `AndroidAccountSessionStore`。该 store 使用 AndroidKeyStore AES/GCM + no-backup ciphertext；Clear 在物理删除之前持久化无账号数据的 revocation fence，使删除/key cleanup 部分失败时旧 session 也不能在 process death 后复活；新会话只有在加密 replacement fsync/rename 成功后才最后清除 revocation fence。

刷新语义必须区分可恢复故障与真正撤销：
- transport failure、服务临时不可用或无法可信解析 refresh response 时，不得仅因为一次 `auth.status` 读取就删除仍可恢复的 durable refresh credential；需要保留同 session identity、记录 refresh failure，并让需要 bearer 的业务调用 fail closed，随后由同一 Host owner 重试/恢复；
- 服务明确拒绝 refresh credential、策略拒绝、缺失 durable refresh credential、账户/设备 identity 改变时，进入 logged-out/reauth-required 并产生 Clear；
- 成功 rotation 必须原子替换 session；服务省略新的 refresh token 时，只能按当前协议允许的 rotation-rescue 规则保留已经拥有的 refresh credential；
- account switch/logout 先 fence 旧 account epoch 与运行中副作用，再清理旧账户 owner；旧 callback 不能重建旧 session。

退出先提升 account epoch，阻止旧异步结果，关闭相应连接与订阅，处理进行中的任务，再清除按政策需删除的凭据/缓存和界面。多账户能力以主线为准；支持时所有记录和附件都必须分区。Passkey 使用平台支持的凭据流程；缺少设备支持时提供主线允许的其他方式，不能返回模拟成功。

## 当前生产映射（不是 verified 声明）

| Desktop source responsibility | Android owner / production wiring | 当前证据状态 |
| --- | --- | --- |
| `SandCursorAuthService` credential/status owner | `source/host/src/account_service.rs::AndroidAccountService` → `source/host/src/android_json_runtime.rs` `feature.auth.*` routes | implemented；需同 HEAD Rust/packaged evidence |
| auth operation/refresh serialization | `source/android-main/.../core/MahayanaHost.kt::dispatch` 的 `SharedHost.lock` + 单一 mutable Rust Host | implemented；并发合同仍需 exact-HEAD tests |
| browser login/open/reopen/cancel/poll | Rust `browser_* ` → `AndroidCoordinatorRuntime.authBrowser*` → `AndroidAccountOAuthAdapter` Custom Tabs | implemented；protected login/device evidence blocked separately |
| protected credential persistence | `AndroidAccountSessionStore` + `AccountSessionRevocationFence` | implemented；unit/instrumentation and packaged evidence required |
| logout/account switch fencing | Host `account_logout` / account refusal → Coordinator account epoch → `AccountTransitionCleanup` → private Clear mutation | implemented；failure/process-death evidence required |
| refresh failure/recovery/rotation | `AndroidAccountService::refresh_if_needed` + account telemetry | under active parity review；must not be marked verified until transient/terminal semantics pass exact-HEAD CI |

## 故障与验证

测试正常登录到真实 Host ready、取消认证、重复回跳、刷新中切换账号、离线冷启动、设备锁定、进程重建、会话被服务端撤销。断言旧账户回调不能改变新账户、退出后旧授权不能继续执行。账户删除需明确确认；远端失败时不能只在本机假装删除。

Focused Rust/contract coverage至少包含：refresh rotation rescue；network/5xx/invalid-response 不错误清除 recoverable durable session；明确 revoked/invalid credential 进入 Clear；刷新后 identity/device fence 不变；logout/account-refusal remote failure 仍本地 fail-closed；browser attempt 冷启动恢复/过期/取消；同一 Host 串行边界下无第二 refresh owner。Android instrumentation 覆盖受保护 session Save/Clear、clear failure + process reopen revocation fence、新登录替换后 fence 清除。

受保护验收使用专用、限权测试账户。测试会话导入与公开发行包隔离；当前 githubRelease 配置需按 [发现记录](../authority/discovery.md) 审计，不能因测试通过就视为生产安全。CI-AUTH、release signing 等外部卡点只阻塞对应 protected/device/release 证据，不得阻塞本地无凭据的 Rust/architecture/source-closure 工作。
