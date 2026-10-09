# Kotlin/Rust ABI 与 Mini App Bridge

## 原生 ABI

现有 `source/android-host-jni` 为唯一入口。同步 JNI 只做轻量验证/入队/取句柄，不在 UI thread 阻塞网络、DB 或推理。采用有限 method enum/DTO，未知 method/version 拒绝。错误显式返回，不让 Rust panic unwind 跨 FFI；JNI exception 检查、对象引用与线程 attach/detach 按官方 JNI 规则实现。

句柄包含 generation；create/open 返回 owned runtime，close 幂等，close 后所有 callback 禁止触达已销毁对象。每次 subscribe 返回可取消 subscription；ViewModel 清理不会关闭 Application-owned runtime。回调从 native worker 转发到合适 dispatcher，不能直接操作 Compose。使用有界队列、批量增量和 snapshot；大 payload 走文件描述符或受限资源 URI。R8 keep rules 由实际 JNI 名称/registration 决定并在 minified 包验证。

## Mini App 私有会话

实际 Desktop `frontend/packages/mcp-app-sdk/src/bridge.ts` 要求 bridgeVersion 2.0、pluginInstanceId、nonce、grants 和私有 port。Android 必须保留这些语义，而不是仅传 requestId/name/input。

平台 owner 为每次 load 生成不可预测 nonce 和唯一 instance，记录 accountEpoch、来源 origin、manifest/version、显式 grants、pending IDs。调用前后都校验 exact session；禁止 JS 自报身份成为真相。重载、导航到非允许来源、账户切换、卸载、dispose/pagehide 时撤销 native registration、取消 pending、向原 session reject pending，并关闭 port。旧 session 的结果不能进入新页面。重复 request ID 在同 session 拒绝。

WebView message listener 必须校验 origin 与 frame/main-frame，使用明确 allowlist；URL scheme/host 比较不能用 contains/前缀字符串。敏感桥不能对任意 iframe 暴露。关闭不需要的 file access、mixed content、debugging；不使用 wildcard 全信任。nonce 不是可信 origin 与权限校验的替代。

## WebMCP 与 fallback

native registration 的取消/AbortSignal 传播到 Host；registration 失效后 unregister。系统/WebView 没有原生 WebMCP 时可用同一 owner 管理的 local registry，不增加第二套权限和 pending 表。UI 必须区分功能不可用、未授权、已取消与失败。不要把仅存在 window 对象当 native bridge 可用。

## ABI/桥接验收

旧 callback、句柄复用、double close、pending close、跨 account、恶意 origin/iframe、无 grant、nonce 错误、重复 ID、unregister 后调用、R8 后 JNI 解析、native crash 恢复和背压压力全部包含正反向测试。实际源 anchor 在 [来源表](../sources.md)，平台风险见官方 WebView/JNI 文档。
