# Fabushi Android (全球法布施 · GPUI-Telegram Android + Mika On-Device Phone Agent)

Canonical native Android repository (**`v2.3.0`**, `versionCode = 20300`) fusing:

1. **[bhrumom/gpui-telegram](https://github.com/bhrumom/gpui-telegram) `@6ca754c` (`v1.6.6`) Parity — Apple iOS Messages + Muse UI/UX & Zero-Fake-Data Real Backend**:
   - **Apple iOS Messages & Desktop `v1.6.6` Muse UX in Jetpack Compose**:
     - **Single Unified Apple iOS Shell**: Persistent 5-tab translucent vector bottom bar (`信息` / `联系人` / `智能体` / `远程` / `设置`) and **Apple Day / Night / Tinted** design system with verified WCAG-safe contrast.
     - **Apple iOS Messages Home**: Large title bar with `编辑` and top-right compose action, single inline pill search, 3-column circular Pinned Chats grid, collapsible archived chats, folder filters, and clean `68dp` conversation rows with indented hairlines.
     - **Apple iOS Messages Chat Stage & Capsule Composer**: Centered `76dp` frosted glass header (`Title ›` opening the Muse Inspector), `46dp` pinned message banner, `18dp` gradient pill bubbles with date separators, UTF-16 rich entities & deduplicated fenced code blocks, floating Tapbacks, multi-message edit/delete/forward/reactions, root-overlay attachment sheet, `23dp` floating capsule composer (`+`, `#007AFF` `↑` send, real `RECORD_AUDIO` permission guard), and full-screen `AndroidMediaViewer`.
     - **Desktop `v1.6.6` Muse 4-Tab Peer/Bot Inspector & iOS Grouped Settings**: `80dp` hero avatar inspector (9 bot avatar shapes, 11 color palettes), iOS inset-grouped Settings with real Sign Out, server-backed active sessions (`/api/v1/auth/sessions*`), profile editing, live `zh-CN`/`en-US` language switching, real `cacheDir` size & clear, and **Rust Argon2id Passcode Lock** (zero plaintext PIN storage).
   - **Zero-Fake-Data Real Backend (`gpui_telegram_server` `/api/v1/*` + `/api/v1/ws`)**:
     - Lock-free Rust JNI host (`mahayana_android_host`) with SQLite WAL write-through caching (zero seeded fake chats/nodes/cards/stories on cold start), fail-closed JNI RPC propagation (`MahayanaHost.requestResult`), server-persisted chat & message pins with `v1.6.5` anti-flicker reconciliation, full `UpdateEvent` WebSocket dispatch with PTS gap recovery, URL-encoded token auth, `401` stop + jittered exponential backoff, polls/drafts/folders/contacts/media CAS, Signal-grade **X3DH + Double Ratchet + Sender Keys E2EE Secret Chats**, and Global FTS5 search. Unbacked services (such as VoIP transport) report honest unavailable states (`CALL_TRANSPORT_UNAVAILABLE`) rather than simulated sessions; the Rust VAD engine remains available for audio capture.
   - **GPUI-Telegram & Fabushi Ecosystem**:
     - **Real Server-Backed Bots & Coding Agents**: Hydrated live from `/api/v1/bots/my` and `/api/v1/fabushi-bots`, supporting `/api/v1/botfather/nl-create`, `/api/v1/coding-agents/probe`, `/api/v1/bots/{id}/computer`, live `execution_state` & `BotActivityChanged` WebSocket updates, and inline callback queries.
     - **全球法布施 (`fabushi_miniapp`)**: Authentic `concap.csv` country list (zero fabricated islands/entries), Zen Room spinning prayer wheel, sutras, practice sessions, merit leaderboard & achievements, fail-closed Dharma AI (`PROVIDER_NOT_CONFIGURED` when no live Magpie seat is configured), and preserved **`MiniAppWebMcpSurface` (`AND-GD-01..05`)** security contract.
     - **TON Wallet**: 24-word BIP39 mnemonic, real Ed25519 `v4r2` address derivation, BOC/Cell builder, `wallet.status` & `wallet.sendTon`, and pure-Kotlin ISO/IEC 18004 **`QrCodeEncoder`** QR rendering (no ZXing dependency in `:app`), with honest unavailable states for unbacked DEX swap, staking, or TonConnect flows.
     - **Magpie No-Subscription AI Gateway**: 48 provider presets all starting honestly unconfigured (`configured = false`, `healthy = false`), Android Keystore-backed API key persistence synced with `MikaApp.providers`, `magpie://import` & `.env` import, smart routing (`Order/Rotate/Cheapest/Drain/Usage/Pace`), Rosetta OpenAI/Responses/Anthropic/Gemini translation, PII/secret masking, and fail-closed routing without fictional fallbacks.
     - **Hermes Multi-Provider AI Studio**: Zero-fake-data cold start (no seeded Kanban cards, approvals, skills, fake memory/terminal, or canned `/moa` echo), backed by real Rust `hermes.*` RPCs and fail-closed unconfigured feedback.
     - **Unified Remote Control Hub (`远程` tab)**: 4 server-backed panels (`My Devices / Local Android Mika`, `Remote Assist` via `/api/v1/remote/assist/*`, `Cloud Computers` via `/api/v1/computers*` & `RemoteComputerOp`, `AI MCP Hub` via `/api/v1/mcp/grants*`), live **`RemoteComputerSurface`** WebRTC viewer, `FabushiRemoteDeviceGateway` bound to Telegram auth, canonical `FabushiAppAgentSurface` actions (`"invoke"`, `"setValue"`), and `local-android-mika` wired directly to `MikaApp.mcp`, `TaskService`, and `EmbeddedMikaWorkspace`.
2. **[wangzan101/Mika](https://github.com/wangzan101/Mika) Full On-Device Android Phone Control Fusion (`:mika-core` + `:mika-agent`)**:
   - **Multi-Display Accessibility Engine (`MikaAccessibilityService`)**: UI tree extraction, bundled offline **MLKit Chinese + Latin OCR** fusion, element actions, per-app speed pacing (`快 Fast / 标准 Balanced / 像人 HumanLike` + `carefulApps`), and self-protection.
   - **On-Device Wireless ADB + Standalone `UID 2000` `IMikaShell` Binder Daemon + Root Shell**: SPAKE2/TLS 1.3 Wireless Debugging pairing (`NsdAdbServiceFinder` + `PairingCoordinator`), offscreen **`VirtualDisplay` (`AgentDisplay`)** background app execution without hijacking the main screen, silent clipboard, call audio PCM bridge, and `BootRestoreReceiver` self-healing.
   - **Floating Capsule & Live `AgentPip`**: Zero-`SYSTEM_ALERT_WINDOW` magnetic `TYPE_ACCESSIBILITY_OVERLAY` Floating Capsule + Quick Settings `CapsuleTileService` + spring-snapping in-app Picture-in-Picture viewport.
   - **Safety Guardrails & Human Handover**: One-shot `ConfirmationGate` (`IrreversibleDetector`) + `TakeoverActivity` / `WebTakeoverActivity` for payments, logins, and 2FA.
   - **Teach/Record/Replay, Workflows, Checklists, Schedules, Web CDP & Linux Sandbox**: User demonstration recording & zero-LLM `Replayer`, fill-in task templates, 2–8 step sequential `Workflows` with result reuse, updatable `SavedChecklists`, `AlarmManager` global schedules with quiet hours, multi-tab headless WebView + Chrome DevTools Protocol (`WebDevtools` + `PageKeeper`), bundled `arm64-v8a` Alpine Linux `proot` sandbox (`rootfs.tar.xz` + `TerminalActivity`), 5-backend Hold-to-Talk ASR, `MikaInCallService` AI call assistant, `NoticeListenerService` proactive digests, 12-point `SelfCheck`, and WeChat/MCP/Relay connectors.
   - **Native Telegram <-> Mika HostChannel (`TelegramMikaChannel` + `TelegramMikaChannelBridge`)**: Registered via `MikaApp.registerHostChannel`, enabling real-time task dispatch, steering, and inline **`ConfirmationGate` Approve (`批准`) / Decline (`拒绝`)** buttons inside chat cards, alongside `/mika <goal>` in chats, the `远程` tab (`local-android-mika` / `EmbeddedMikaWorkspace`), and the Floating Capsule.

## Repository Layout

- `docs/specs/desktop-v1-6-6-apple-ios-refactor-and-real-backend.md` — Authoritative master specification for `v2.3.0` (`bhrumom/gpui-telegram@6ca754c` `v1.6.6` parity, Apple iOS Messages + Muse UI/UX, zero-fake-data real backend; `No Spec, No Code`)
- `docs/specs/global-dandelion-miniapp-contract.md` — Active companion contract for Global Dharma (`MiniAppWebMcpSurface`, `AND-GD-01..05`)
- `mobile/android/rust/mahayana_android_host` — Self-contained Rust JNI & C FFI runtime (`libmahayana_app_host.so`, 16KB page-aligned)
- `mobile/android/mika-core` — Pure-JVM Mika agent core, multi-provider LLM clients, OTLP trace schemas & bundled skills/knowledge
- `mobile/android/mika-agent` — Android library housing Mika Accessibility, `IMikaShell` AIDL, VirtualDisplay, Capsule, OCR, Web CDP, `proot` sandbox & Compose screens
- `mobile/android/app` — `com.ombhrum.fabushi` unified Apple iOS Messages + Muse + Fabushi + Mika Android application (`minSdk 29`, `compileSdk 37`, `targetSdk 37`)
- `scripts/build_rust_android.sh` — Cross-compiles 16KB-aligned `libmahayana_app_host.so` for `arm64-v8a`, `x86_64`, and host JVM tests
- `scripts/package_android_release.py` — Signs, verifies 16KB ELF alignment & `ZIP_STORED` `rootfs.tar.xz`, and emits `fabushi-android-update.json` + `SHA256SUMS`

## Building & Testing Locally

Follow `AGENTS.md` bounded concurrency, sequential R8 release builds, and immediate post-test JVM daemon/artifact cleanup:

```bash
# 1. Run full Rust workspace tests and cross-compile 16KB-aligned native host libraries
CARGO_BUILD_JOBS=2 cargo test --manifest-path mobile/android/rust/Cargo.toml --workspace -- --nocapture
ANDROID_API_LEVEL=29 CARGO_BUILD_JOBS=2 bash scripts/build_rust_android.sh

# 2. Run Mika & App JVM/JNI unit tests, AndroidTest compilation, and Debug APK (bounded worker)
./mobile/android/gradlew -p mobile/android --max-workers=1 \
  :mika-core:test :mika-core:knowledgeIndex :mika-agent:testDebugUnitTest \
  :app:testDebugUnitTest :app:compileDebugAndroidTestKotlin :app:assembleDebug --stacktrace

# 3. Assemble R8-minified release APKs sequentially (never in parallel on memory-bounded hosts)
./mobile/android/gradlew -p mobile/android --max-workers=1 :app:assembleRelease --stacktrace
./mobile/android/gradlew -p mobile/android --max-workers=1 :app:assembleGithubRelease --stacktrace

# 4. Mandatory immediate post-test daemon & artifact cleanup (AGENTS.md)
(cd mobile/android && ./gradlew --stop) || true
jps -l | awk '/KotlinCompileDaemon|GradleDaemon/ {print $1}' | xargs -r kill || true
rm -rf mobile/android/rust/mahayana_android_host/target \
       mobile/android/rust/target \
       mobile/android/rust/host-lib/libmahayana_app_host.so \
       mobile/android/app/src/main/jniLibs/arm64-v8a/libmahayana_app_host.so \
       mobile/android/app/src/main/jniLibs/x86_64/libmahayana_app_host.so \
       dist release-signing
jps -l
free -h
```

## License

Apache License 2.0 (see [LICENSE](LICENSE), [NOTICE](NOTICE), and [licenses/THIRD_PARTY_NOTICES.md](licenses/THIRD_PARTY_NOTICES.md)).
