# 可追溯来源

读取日期：2026-10-09。源码事实固定到完整 commit；Android 政策/API 页面在实现及发布日重新核验。本页是入口清单，不表示所有列出的源码已逐行审计。

## 源仓库

Desktop 权威：[main 快照](https://github.com/bhrumom/fabushi-desktop/tree/3bc92400826cc4ca7ac665b467708e22261edc61)。根 [Rust 闭包](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/RUST_RUNTIME_SOURCE.md) 与 [前端闭包](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/DESKTOP_SOURCE_CLOSURE.md) 是必须继续追踪的真实路径。

已读取关键实现：[MCP App bridge](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/bridge.ts)、[WebMCP registry](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/mcp-app-sdk/src/webmcp.ts)、[共享 journey 特征](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/shared/src/mahayana-host-features.ts)。其中 private port、instance/nonce/grants、dispose pending 和 registry abort/fallback 的语义必须保留。

Android 既有实现：[迁移前 HEAD](https://github.com/fabu-android/fabushi-android/tree/5d049353e167d9b91b5c765e1686f1ac8670f2ff)、[现有 app 构建配置](https://github.com/fabu-android/fabushi-android/blob/5d049353e167d9b91b5c765e1686f1ac8670f2ff/mobile/android/app/build.gradle)、[自有 Rust workspace](https://github.com/fabu-android/fabushi-android/blob/5d049353e167d9b91b5c765e1686f1ac8670f2ff/Cargo.toml)。已存在不等于当前 Desktop parity 已验收。

## Android 官方依据

- [后台启动前台服务限制](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)：服务资格、while-in-use 权限和前台状态必须一起判断。
- [16 KB 内存页支持](https://developer.android.com/guide/practices/page-sizes)：检查全部 native .so 及实际安装包；不只 Rust 主库。
- [WebView native bridge 风险](https://developer.android.com/privacy-and-security/risks/insecure-webview-native-bridges)：不可信内容不得获得应用权限。
- [SpeechRecognizer API](https://developer.android.com/reference/android/speech/SpeechRecognizer)：on-device 可用性与创建接口需 API/设备检查，不能把 prefer-offline 当成保证。
- [JNI 性能与所有权](https://developer.android.com/training/articles/perf-jni)：线程、引用和跨边界开销。
- [Credential Manager](https://developer.android.com/identity/sign-in/credential-manager)：原生凭据入口。
- [Android Keystore](https://developer.android.com/privacy-and-security/keystore)：密钥保护。
- [Photo Picker](https://developer.android.com/training/data-storage/shared/photopicker) 与 [Storage Access Framework](https://developer.android.com/guide/topics/providers/document-provider)：媒体/文件授权。
- [Play target API](https://developer.android.com/google/play/requirements/target-sdk)：发布时复核，不按旧记忆声明最新值。
- [GitHub Git trees API](https://docs.github.com/en/rest/git/trees)：recursive 可能 truncated，必须处理；gitlink 不是普通文件。

本轮已在线核对前四项；其余为实施时必须继续核对的官方入口，不宣称本轮已审阅其所有当前要求。
