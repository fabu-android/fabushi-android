# GPUI-Telegram Android Full Parity & Mika On-Device Phone Control Fusion — Specification

Status: superseded
Owner: fabu-android
Last updated: 2026-11-18
Related issue/task/PR: Initial `v2.0.0` workspace scaffold; superseded and amended for production parity (`v2.1.0`) by `docs/specs/gpui-telegram-android-production-parity.md`

> **Supersession & Audit Notice (2026-11-18)**: Line-by-line post-`v2.0.0` verification revealed that while the initial JNI ABI, Gradle multi-module layout (`71e00f5` Mika snapshot), and `v2.0.0` APK packaging structure compiled, multiple runtime surfaces relied on hardcoded demo seed state, local mock/echo handlers, dropped API keys, and incomplete upstream parity. All remaining production parity requirements (`bhrum/gpui-telegram@889246e` headless crate reuse, `wangzan101/Mika@b899c5b` 100% sync, Keystore AI vault, `TelegramMikaChannel`, `APPS` search, 1:1 Telegram Android UX fixes, and monotonic `v2.1.0` release) are governed by `docs/specs/gpui-telegram-android-production-parity.md`.

## 1. Context / problem

`fabu-android/fabushi-android` was initially exported as a thin Android boundary (`mobile/android/app`, ~8K LOC Kotlin/Compose) with two major gaps relative to the user's product vision:

1. **Missing Native Runtime & Incomplete Telegram Experience**:
   - `MahayanaHost.kt` loads `System.loadLibrary("mahayana_app_host")`, yet no Rust JNI implementation or `jniLibs` was present in this repository, causing `UnsatisfiedLinkError` at runtime unless externally injected.
   - Logged-in users defaulted to a minimal `GrokMobileShellAndroid` list rather than a unified, 1:1 Telegram Android client experience, and lacked the full capabilities of `bhrum/gpui-telegram` v1.2.0 (Telegram Android Drawer/Folder Tabs/Stories/Chat Bubbles/UTF-16 Rich Markdown/Swipe-to-Reply/Reactions/Stickers & GIFs/5-bit 100-bar Voice Waveforms/Round Video Notes/Polls & Quizzes/Forum Topics/Broadcast Channels & Comments/Signal-grade X3DH + Double Ratchet + Sender Keys E2EE Secret Chats with 4-emoji & 333-emoji fingerprints/1:1 & Group VoIP & Video Calls/TON Wallet & Collectibles & TonConnect/BotFather & Mini Apps including 全球法布施 `fabushi_miniapp`/Hermes 6-tab AI Studio/Magpie 47+ Provider No-Subscription Gateway/Multi-Device Cloud Computer Control & Action Bus/11-page Telegram Settings & Themes/Passcode & Biometric Lock/Multi-Account Switcher).
2. **No Cross-App On-Device Phone Control**:
   - `FabushiAppAgentSurface` could only inspect in-app semantic elements of Fabushi itself and had zero ability to operate arbitrary Android apps, virtual displays, system intents, headless browsers, or an on-device Linux sandbox like `wangzan101/Mika` v0.1.45.

## 2. Goal

Deliver a complete, production-ready, self-contained **GPUI-Telegram + Fabushi + Mika Unified Android Application (`v2.0.0`)** in `fabu-android/fabushi-android`:

1. **1:1 Telegram Android Visual, Interaction & Feature Parity + Full `bhrum/gpui-telegram` Feature Set**:
   - Native Jetpack Compose Telegram Android UI (Telegram Day Blue, Night, Tinted, Classic & Custom Accent themes; slide-out Telegram Navigation Drawer with multi-account switcher, Saved Messages, Contacts, Calls, Nearby/People, New Group/Secret Chat/Channel, BotFather, Mini Apps, TON Wallet, Hermes AI Studio, Mika Phone Control Center, Settings; horizontal swipeable Folder filter tabs + collapsible Archive + Stories bar; full Telegram Chat screen with grouped tails, UTF-16 rich entities, reply/quote swipe, forward, edit, delete/revoke, pinned banner, scheduled & silent send, self-destruct TTL, inline keyboards, slash commands, stickers/GIFs/reactions, 5-bit 100-bar voice waveform recorder/player + floating audio player bar, round video notes, lightbox media viewer, polls/quizzes, locations, contacts, forum topics, channel comments & analytics, E2EE Secret Chats with emoji fingerprint verification, 1:1 & group VoIP/video call overlay, passcode/biometric lock, and global search).
   - Native Rust JNI runtime (`mobile/android/rust/mahayana_android_host` -> `libmahayana_app_host.so`) compiled cleanly from source for `aarch64-linux-android` (`arm64-v8a`), `x86_64-linux-android` (`x86_64`), and `x86_64-unknown-linux-gnu` (host JVM tests), powering durable SQLite WAL storage, PTS sync + live HTTP/WebSocket bridge to `gpui_telegram_server`, Signal-grade X3DH/Double-Ratchet/Sender-Keys E2EE, 5-bit waveform codec, TON Wallet (BIP39/Ed25519/BOC/TonConnect), 全球法布施 (`fabushi_miniapp`) engine, Hermes Agent Studio & Task FSM/Cron, Magpie 47+ provider Rosetta AI gateway with PII masking, Multi-Device Computer Control & Action Bus, and 100% of existing `MahayanaHost` JSON-RPC contracts.
2. **100% Fusion of `wangzan101/Mika` On-Device Phone Control Capabilities**:
   - Integrate Mika's complete `:mika-core` (490 Kotlin files + OTLP trace protobufs + bundled skills & app/site knowledge index) and `:mika-agent` (544 Kotlin files + `IMikaShell.aidl` + 11-locale resources + bundled MLKit Latin/Chinese OCR + headless CDP `page.js` + xterm.js terminal + `arm64-v8a` `libmikarun.so`/`libproot.so`/`libproot_loader.so` + Alpine `rootfs.tar.xz` sandbox).
   - Preserve 100% of Mika's phone-operation capabilities: `MikaAccessibilityService` multi-display tree reader & gesture injector, per-app speed pacing (`快/标准/像人` + `carefulApps`), Wireless ADB SPAKE2/TLS pairing + standalone `UID 2000` `HelperMain` Binder daemon + Root shell, offscreen `VirtualDisplay` background app execution + `AgentPip` live viewport, `TYPE_ACCESSIBILITY_OVERLAY` magnetic Floating Capsule + `CapsuleTileService`, one-shot irreversible `ConfirmationGate` + `TakeoverActivity`/`WebTakeoverActivity`, `TeachRuntime` demonstration recording & zero-LLM `Replayer`, multi-tab headless WebView + CDP engine, Alpine Linux sandbox & PTY terminal, 5-backend Hold-to-Talk ASR, `MikaInCallService` call assistant, `NoticeListenerService` proactive digests, `SelfCheck` diagnostics, multi-step sequential `Workflows` with fill-in templates & updatable `SavedChecklists`, `AlarmManager` global cron schedules with quiet hours, and WeChat/MCP/Relay connectors.
   - Bridge Telegram <-> Mika natively via `TelegramMikaChannelBridge` so users can trigger Mika phone automation directly from Telegram chats, Saved Messages, `@MikaPhoneBot`, the Telegram Drawer, or the Floating Capsule, with live step updates, confirmation gates, and remote `FabushiRemoteDeviceGateway` integration.
3. **End-to-End Verification, GitHub CI/CD & `v2.0.0` Release**:
   - Verify Rust crate tests, `:mika-core` JVM tests, `:app` unit & JNI integration tests, Android test compilation, lint, and signed `debug` / `release` / `githubRelease` APK builds locally and via GitHub Actions, commit & push to `fabu-android/fabushi-android`, and publish GitHub Release `v2.0.0` with signed APKs, `fabushi-android-update.json`, and `SHA256SUMS`.

## 3. Non-goals / out of scope

- Modifying Desktop (`gpui_telegram_desktop` macOS/Windows/Linux GPUI windowing) or iOS repositories.
- Requiring external Shizuku installation or root privileges for standard Accessibility / Wireless ADB / Web / Sandbox operation (Root remains strictly optional).
- Breaking any existing `MahayanaHost`, `FabushiScreen`, `TestTags`, `FabushiCiBootstrap`, `MiniAppWebMcpSurface`, or `FabushiAppAgentSurface` contracts.

## 4. Requirements

- **R1 (Telegram Android Shell & Navigation Parity)**: Default authenticated experience provides a full Telegram Android Material 3 shell with collapsible top bar, Story strip, swipeable Folder filter tabs (`All`, `Personal`, `Bots`, `Groups`, `Channels`, custom folders), collapsible Archived Chats row, swipe-to-reply/pin/archive/mute gestures, floating New Message/Camera action buttons, and Telegram slide-out Navigation Drawer with multi-account switcher, Saved Messages, New Group, New Secret Chat, New Channel, Contacts, Calls, BotFather, Mini Apps (全球法布施), TON Wallet, Hermes Studio, Mika Phone Agent, and Settings. Preserve optional switch to `GrokMobileShellAndroid` and 100% compatibility with `FabushiScreen(...)` entry points and all `TestTags`.
- **R2 (Telegram Rich Messaging & Composer Parity)**: Chat room supports grouped message bubbles with timestamps & `✓`/`✓✓` read receipts, UTF-16 `MessageEntity` rendering & formatting toolbar (`Bold`, `Italic`, `Underline`, `Strikethrough`, `Spoiler`, `Inline Code`, `Code Block`, `Link`, `Mention`, `Hashtag`, `BotCommand`), reply/quote preview & swipe-to-reply, multi-message forward/copy/delete (for me / revoke for all), message editing, pinned message banner cycling, silent send & scheduled messages, self-destruct TTL timer, inline bot keyboards & callback queries, slash-command autocomplete popup, reactions & emoji/sticker/GIF picker, polls & quizzes (single, multi, anonymous, quiz mode with explanation), contact cards, location cards, link previews, and file/photo/video/album attachments.
- **R3 (Voice Waveforms, Round Video Notes, Media Viewer & Calls)**: Support Telegram 5-bit 100-bar voice message recording (with slide-to-cancel & lock gestures), interactive waveform scrubbing playback, persistent top floating audio player bar (0.5x/1x/1.5x/2x speed), round video note capture/playback UI, pinch-to-zoom lightbox `AndroidMediaViewer` with streaming/saving/sharing, and 1:1 + Group VoIP/Video Call & Screen Share overlay with 333-emoji 4-glyph E2EE call verification (`CallsEmojiFingerprint`) + `<60ms` VAD barge-in Talk Mode orb.
- **R4 (Groups, Forum Topics, Channels, Stories & Global Search)**: Support supergroup Forum Topics view, Broadcast Channels with post view counts, linked discussion comments, and channel analytics/statistics, chat history JSON/HTML export, top-of-list Stories viewer & creator, and FTS5/CJK global & in-chat search across chats, messages, media, links, files, music, and voice.
- **R5 (E2EE Secret Chats, Passcode/Biometric Lock & 11 Settings Pages)**: Implement Signal-grade X3DH + Double Ratchet (1:1 Secret Chats) and Sender Keys (encrypted groups) in Rust with 4-emoji & 32/333-emoji key fingerprint verification dialog, local PIN/Passcode + Biometric lock screen, and all 11 Telegram Settings pages (`Account/Profile`, `Chat Settings & Themes`, `Privacy & Security`, `Notifications & Sounds`, `Data & Storage`, `Chat Folders`, `Devices/Sessions`, `Power Saving`, `Language & Translate`, `Telegram Stars & TON Wallet`, `Proxy & Network`).
- **R6 (GPUI-Telegram Special Capabilities — BotFather, Mini Apps, 全球法布施, TON Wallet, Hermes, Magpie, Computer Control)**:
  - `@BotFather` conversational & natural-language bot creation + companion AI bots (`SandAgentProfile` avatars).
  - Mini App Host (`telegram-web-app.js` + WebMCP + CloudStorage quotas) + built-in **全球法布施 (`fabushi_miniapp`)** interactive 2D/3D 240-country Earth Offering Globe, Zen Room spinning prayer wheel, sutras, practice tracker, merit leaderboard, achievements, and AI Dharma chat.
  - **TON Wallet** (BIP39 24-word mnemonic, Ed25519 keypair, BOC/Cell builder, Send/Receive/QR, Swap, Staking, NFT Collectibles, Telegram Stars/Gifts, TonConnect emulation & signing).
  - **Hermes Multi-Provider AI Agent & 6-Tab Studio** (`Sessions & Providers`, `Memory & Soul`, `Skills Hub`, `Toolsets & Terminal`, `Cron & Heartbeat`, `Kanban & Approvals`, `/moa` Mixture-of-Agents, 61 slash commands).
  - **Magpie Gateway** (47+ provider presets, smart routing `Order/Rotate/Cheapest/Drain/Usage/Pace`, circuit breakers, Rosetta OpenAI/Responses/Anthropic/Gemini translator, PII/secret masking, `magpie://import` & `.env` import, usage/cost ledger).
  - **Multi-Device Cloud Computer Control & Action Bus** (`LocalDevice`, `CloudServer`, `CloudSandbox`, CUA lease & emergency human takeover, Computer Workbench, schema-first Action Bus with 5 risk tiers, Global Tasks 7-state FSM).
- **R7 (Native Rust JNI Runtime — `libmahayana_app_host.so`)**: Build `mobile/android/rust/mahayana_android_host` into `libmahayana_app_host.so` for `aarch64-linux-android`, `x86_64-linux-android` (with `-Wl,-z,max-page-size=16384`), and `x86_64-unknown-linux-gnu`, exporting exact `Java_com_ombhrum_fabushi_core_MahayanaHost_{nativeCreate,nativeCreateTest,nativeDispatch,nativeDestroy}` JNI symbols and implementing all existing `feature.*` / `plugin.*` / `runtime.*` / `platform.request` contracts plus `telegram.*`, `e2ee.*`, `wallet.*`, `hermes.*`, `magpie.*`, `fabushiMiniapp.*`, `calls.*`, `computer.*`, and `mika.*` methods backed by SQLite WAL and optional live `gpui_telegram_server` sync.
- **R8 (Full Mika `:mika-core` & `:mika-agent` Module Integration)**: Integrate `:mika-core` and `:mika-agent` as Gradle modules under `mobile/android/`, preserving `com.wangzan101.mika.*` packages, `IMikaShell.aidl`, OTLP trace protobuf codegen, `knowledgeIndex` build-time validation, 11-locale resources, MLKit Latin + Chinese offline OCR, `libmikarun.so`/`libproot.so`/`libproot_loader.so`, and `assets/runtime/rootfs.tar.xz`.
- **R9 (Full Mika Phone-Control Parity)**: Support 100% of Mika's device capabilities inside `com.ombhrum.fabushi`:
  - `MikaAccessibilityService` multi-display UI tree extraction, OCR fusion, element tap/long-press/swipe/wheel/paste, per-app speed pacing (`Fast`/`Balanced`/`HumanLike` + `carefulApps`), and self-protection (`NoticeFilter` + `AppClassifier` recognizing `com.ombhrum.fabushi`).
  - Wireless ADB SPAKE2/TLS pairing (`PairingCoordinator`, `NsdAdbServiceFinder`, `AndroidAdb`), standalone `UID 2000` `HelperMain` Binder service (`IMikaShell`), Root shell (`RootCommands`), and `BootRestoreReceiver` auto-recovery.
  - Offscreen `VirtualDisplay` (`AgentDisplay`) background app operation + in-app `AgentPip` live viewport + `TYPE_ACCESSIBILITY_OVERLAY` Floating Capsule (`CapsuleController` + `CapsuleTileService`).
  - One-shot `ConfirmationGate` + `TakeoverActivity` / `WebTakeoverActivity` human handover, `TeachRuntime` demonstration recording & zero-LLM `Replayer`, headless multi-tab WebView + CDP engine (`WebTabPool` + `PageKeeper`), Alpine `proot` Linux sandbox & `TerminalActivity`, 5-backend Hold-to-Talk voice input, `MikaInCallService` call assistant, `NoticeListenerService` proactive digests, `SelfCheck` 12-point diagnostics, fill-in Saved Tasks, multi-step sequential `Workflows`, updatable `SavedChecklists`, `AlarmManager` global schedules with quiet hours, and WeChat/MCP/Relay connectors.
- **R10 (Unified Telegram <-> Mika Bridge)**: Provide `TelegramMikaChannelBridge` connecting `:mika-core`'s `ChannelGateway` / `TaskService` with `MahayanaHost` messaging and `FabushiRemoteDeviceGateway` (`fabushi.phone.*` capability-gated tools alongside `fabushi.app.*`), enabling users to launch, monitor, steer, approve, and receive screenshots/artifacts from Mika phone tasks directly inside Telegram chats (`@MikaPhoneBot` & `/mika` slash commands) as well as the full Mika workspace screen.
- **R11 (Self-Update, Signing, R8/ProGuard & Packaging Integrity)**: Update `AndroidUpdateViewModel` to support `fabu-android/fabushi-android` GitHub Releases (`fabushi-android-update.json` + SHA-256 verification), configure comprehensive R8/ProGuard rules so `release` and `githubRelease` minified APKs preserve JNI, AIDL, `HelperMain`, `kotlinx.serialization`, Protobuf Lite, MLKit OCR, and `@JavascriptInterface` bridges, and enforce `useLegacyPackaging = true` + `noCompress += "xz"`.
- **R12 (CI/CD, Verification & GitHub Release Delivery)**: Add `.github/workflows/ci.yml` and `.github/workflows/release.yml`, verify all tests and APK builds locally and on GitHub, commit and push all code to `fabu-android/fabushi-android`, and publish a verified GitHub Release `v2.0.0` with installable signed APKs and update metadata.

## 5. Current state

- Repository `fabu-android/fabushi-android` (`HEAD 266f90f`) contains only `mobile/android/app` (21 Kotlin files, 4 test files), `README.md`, `MIGRATION_SOURCE.md`, `AGENTS.md`, and `docs/specs/{SPEC_TEMPLATE.md, spec-first-ai-development.md}`.
- No `.github/workflows/`, no GitHub Releases, no `jniLibs` / Rust host crate, no Mika phone-control modules, and incomplete Telegram UI/protocol features.

## 6. Target state

```text
fabushi-android/
├── .github/workflows/
│   ├── ci.yml                     # Rust tests + NDK build + :mika-core + :mika-agent + :app tests/lint/APK
│   └── release.yml                # Signed release & githubRelease APKs + fabushi-android-update.json + GitHub Release
├── AGENTS.md                      # Updated repository scope & build hygiene rules
├── LICENSE                        # Apache-2.0
├── NOTICE                         # Attribution for bhrum/gpui-telegram & wangzan101/Mika
├── README.md                      # Architecture, features, build & release documentation
├── docs/specs/
│   ├── SPEC_TEMPLATE.md
│   ├── spec-first-ai-development.md
│   └── gpui-telegram-android-mika-fusion.md
└── mobile/android/
    ├── settings.gradle            # include ':app', ':mika-core', ':mika-agent'
    ├── build.gradle
    ├── gradle.properties
    ├── rust/mahayana_android_host/ # Self-contained Rust cdylib -> libmahayana_app_host.so
    ├── mika-core/                 # Pure-Kotlin JVM Mika agent core + OTLP proto + knowledge/skills
    ├── mika-agent/                # Android library: Accessibility, IMikaShell, VirtualDisplay, Capsule, OCR, CDP, Sandbox, UI
    └── app/                       # com.ombhrum.fabushi: Unified Telegram Android UI + GPUI-Telegram features + Mika bridge
```

## 7. Architecture and ownership boundaries

Per explicit user instruction (`/goal https://github.com/bhrum/gpui-telegram 把这个做成安卓版本，并且实现和telegram安卓版本完全一样的效果，把https://github.com/wangzan101/Mika里面的功能完全融合进我们的项目里面实现一样操控手机的能力,完成所有把代码提交推送到github发布全新版本`) and `docs/specs/spec-first-ai-development.md` ("The latest explicit user requirement may amend an older Spec, but update the durable Spec/decision record before implementation intentionally diverges from it"):

1. **`mobile/android/rust/mahayana_android_host` (Rust Native Runtime Consumer & Engine)**:
   - Owned by this repository to guarantee reproducible, self-contained Android APK builds without external binary artifacts.
   - Implements the `libmahayana_app_host.so` JNI boundary (`com.ombhrum.fabushi.core.MahayanaHost`) plus embedded Rust modules for SQLite WAL messaging/sync, E2EE (X25519/Ed25519/ChaCha20-Poly1305 X3DH, Double Ratchet, Sender Keys, 4-emoji & 333-emoji fingerprints), 5-bit 100-bar waveform codec, TON Wallet (BIP39/Ed25519/BOC/TonConnect), 全球法布施 (`fabushi_miniapp`), Hermes Agent Studio & Task FSM/Cron, Magpie 47+ Provider Gateway & PII Masking, Multi-Device Computer Control & Action Bus, and live HTTP/WS `gpui_telegram_server` client connectivity.
2. **`mobile/android/mika-core` & `mobile/android/mika-agent` (On-Device Phone Control Engine)**:
   - `:mika-core` owns pure-JVM agent loop, multi-provider LLM callers, tool registry, risk/confirmation gates, workflows, checklists, memory, skills, schedules, MCP, and OTLP trace schemas.
   - `:mika-agent` (`namespace = "com.wangzan101.mika"`) owns Android Accessibility, `IMikaShell` UID 2000 Binder helper, Wireless ADB pairing, VirtualDisplay, Floating Capsule, PiP, MLKit OCR, WebView CDP, Alpine `proot` sandbox, Voice ASR, InCall assistant, Proactive Notification Listener, SelfCheck, and Mika Compose screens.
3. **`mobile/android/app` (`com.ombhrum.fabushi` — Unified Application Shell)**:
   - `FabushiApplication : MikaApp()` initializes both Mahayana/Telegram native host and Mika's lazy service graph.
   - `MainActivity` hosts the unified Telegram Android experience (`TelegramAndroidShell`), Mini Apps (`MiniAppWebMcpSurface` + 全球法布施), Mika Phone Control Workspace (`MikaEmbeddedWorkspace`), and legacy/Grok shells (`FabushiScreen`, `GrokMobileShellAndroid`) while preserving every existing `TestTags` contract.

## 8. Interfaces / contracts / schemas / data flow

### 8.1 JNI ABI Contract (`com.ombhrum.fabushi.core.MahayanaHost`)
- `Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreate(JNIEnv, JObject, JString appDataDir) -> jlong`
- `Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreateTest(JNIEnv, JObject, JString appDataDir) -> jlong`
- `Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDispatch(JNIEnv, JObject, jlong handle, JString requestJson) -> jstring`
- `Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDestroy(JNIEnv, JObject, jlong handle)`

### 8.2 JSON-RPC Method Families on `MahayanaHost`
1. **Preserved Baseline Contracts (100% backward-compatible)**:
   - `feature.info`, `feature.auth.{status, providers, oauthStart, oauthPoll, browserStart, browserReopen, browserCancel, browserPoll, passwordLogin, logout, deviceAgentSession}`
   - `feature.execute` (`chat.send`, `bot.list`, `bot.create`, `marketplace.install`, `miniapp.open`, `capability.request`, `runtime.longTask`, `session.clear`, `automation.*`, `workflow.*`, `memory.*`, `skill.*`, `computer.*`)
   - `feature.receive`, `feature.interrupt`, `feature.approval.resolve`, `feature.persistence.{get,set,remove,list}`
   - `feature.marketplace.{browse, release, add}`, `feature.plugin.{install, uninstall, listInstalled, uiDocument}`, `plugin.permission.grant`, `plugin.compatibility`, `runtime.{start, call}`, `platform.request`
   - `feature.messaging.access.issue`, `feature.messaging.execute` (Protocol v2 all 19 commands & 11 event types + rich extensions), `feature.messaging.blob.read`
2. **Extended GPUI-Telegram & Mika RPC Families**:
   - `telegram.auth.{phoneSendCode, phoneVerify, qrPairInit, qrPairPoll, twoFactorStatus, twoFactorSet, twoFactorVerify, passkeyList, accountsList, accountSwitch, accountAdd, passcodeSet, passcodeVerify}`
   - `telegram.messaging.{searchGlobal, searchInChat, listTopics, createTopic, listStories, createStory, viewStory, listStickerSets, installStickerSet, searchGifs, channelStats, exportChat, encodeWaveform, decodeWaveform, inlineQuery, callbackQuery}`
   - `telegram.e2ee.{initDevice, createSecretChat, sendEncrypted, safetyFingerprint, callEmojiFingerprint}`
   - `telegram.calls.{startCall, joinGroupCall, signal, toggleMute, toggleVideo, toggleScreenShare, endCall, history, vadFeedPcm}`
   - `telegram.server.{configureEndpoint, syncDifference, status}`
   - `wallet.{status, createWallet, importMnemonic, balance, transactions, sendTon, sendStars, collectibles, tonConnectParse, tonConnectEmulate, tonConnectApprove}`
   - `hermes.{status, sessions, createSession, branchSession, sendTurn, runSlashCommand, listCommands, memoryGet, memoryUpdate, skillsList, toolsetsList, cronList, cronUpsert, kanbanBoard, kanbanMove, approvalsDecide}`
   - `magpie.{presets, providers, upsertProvider, importUri, routePreview, chatComplete, maskPreview, usageSummary}`
   - `fabushiMiniapp.{bootstrap, offerDharma, spinPrayerWheel, recordPractice, leaderboard, achievements, sutras, aiDharmaChat}`
   - `computer.{nodesList, nodeUpsert, leaseAcquire, leaseTakeover, executeOp, actionBusList, actionBusInvoke}`
   - `mika.{bridgeStatus, registerTaskEvent, listBoundChats, bindChat}`

### 8.3 Telegram <-> Mika Bridge Data Flow
```text
Telegram Chat / @MikaPhoneBot / Saved Messages / Floating Capsule / Remote Gateway
        │
        ▼
TelegramMikaChannelBridge (implements Mika Channel + subscribes to RunEventBus)
        │
        ├──► TaskService / RunAssembler / AgentLoop (:mika-core + :mika-agent)
        │         ├──► MikaAccessibilityService (UI tree + OCR + Paced Gestures)
        │         ├──► IMikaShell UID 2000 Helper (VirtualDisplay + Input + Clipboard + CallAudio)
        │         ├──► ConfirmationGate / TakeoverActivity (One-shot safety approval)
        │         ├──► WebTabPool + CDP / AndroidSandbox (Alpine proot + Python/Node)
        │         └──► TeachRuntime / Replayer / WorkflowRuntime / TriggerRuntime
        │
        └──► Streams step progress, screenshots, AgentPip status & final artifacts back into Telegram Chat & MahayanaHost
```

## 9. Constraints and non-functional requirements

- **SDK & ABI Compliance**: `compileSdk = 37`, `targetSdk = 37`, `minSdk = 29` (required by Android 10+ TLS keying material exporter `SSLSockets.exportKeyingMaterial` for on-device Wireless ADB pairing, `QSTile`, scoped storage, and multi-display accessibility).
- **Android 15/16/17 16KB Page Alignment**: Rust `cdylib` (`libmahayana_app_host.so`) MUST link with `-Wl,-z,max-page-size=16384`.
- **Sandbox Native Executable Packaging**: `packaging.jniLibs.useLegacyPackaging = true` so `libmikarun.so`, `libproot.so`, and `libproot_loader.so` extract executable into `nativeLibraryDir`; `androidResources.noCompress += "xz"` for streaming `assets/runtime/rootfs.tar.xz`.
- **Security & Privacy**:
  - No hardcoded secrets, API keys, or tokens in code, specs, or workflows.
  - All provider keys, OAuth tokens, wallet mnemonics, workflows, and memories encrypted via Android Keystore AES-256-GCM (`KeystoreKeyCipher`) or zeroized Rust memory (`zeroize`).
  - `MikaAccessibilityService` uses `TYPE_ACCESSIBILITY_OVERLAY` (no `SYSTEM_ALERT_WINDOW`), blocks self-package `com.ombhrum.fabushi` (`NoticeFilter` + `AppClassifier`), and enforces one-shot `ConfirmationGate` + mandatory human `Takeover` on payment screens.
  - `REQUEST_INSTALL_PACKAGES` restricted strictly to `src/githubRelease/AndroidManifest.xml` (absent from Play-compatible `main/AndroidManifest.xml`).
- **Lean Build Hygiene**: Cargo profiles use `debug = 0`, `strip = "debuginfo"`, `incremental = false` to prevent disk exhaustion.

## 10. Failure modes and edge cases

1. **Non-`arm64-v8a` Emulator/Device (`x86_64`)**: `libmahayana_app_host.so` is compiled for both `arm64-v8a` and `x86_64`, while `BundledRuntime` checks for `libmikarun.so` (`arm64-v8a`) and degrades gracefully to `SandboxStatus.Unsupported` without crashing.
2. **Offline / Unreachable `gpui_telegram_server`**: `libmahayana_app_host.so` persists all conversations, messages, folders, drafts, bots, wallet state, Hermes sessions, and 全球法布施 state in local SQLite WAL, queueing outbound server frames in durable outbox and operating 100% offline-first.
3. **Accessibility / Wireless ADB Not Yet Enabled**: Mika Phone Control screens and `@MikaPhoneBot` surface clear step-by-step setup cards (`PhoneSetupScreen` + `SelfCheckScreen`) while Web, Sandbox, Hermes, and Telegram features work immediately without phone permissions.
4. **R8 / ProGuard Obfuscation in Release Builds**: Explicit keep rules in `proguard-rules.pro` protect `MahayanaHost` JNI, `HelperMain.main(String[])` (`app_process` entry), `IMikaShell` AIDL, `kotlinx.serialization` serializers, OTLP `com.google.protobuf` / `io.opentelemetry.proto`, MLKit OCR, and `@JavascriptInterface` classes.
5. **Notification Feedback Loop**: `NoticeFilter` explicitly filters both `com.wangzan101.mika` and `com.ombhrum.fabushi` so Telegram incoming message notifications never recursively trigger `NoticeListenerService` unless explicitly invoked via `@MikaPhoneBot`.

## 11. Implementation strategy

1. **Workstream 1 — Native Rust Host Crate (`mobile/android/rust/mahayana_android_host`)**:
   - Create lean, zero-system-C-blocker Rust workspace (`rusqlite` bundled + FTS5, `reqwest` with `rustls-tls-webpki-roots` + `ring`, `ed25519-dalek`, `x25519-dalek`, `chacha20poly1305`, `sha2`, `hmac`, `hkdf`, `jni`) implementing `MahayanaHost` JNI + C FFI, full protocol v2 messaging + Telegram extensions, E2EE, 5-bit waveform codec, TON Wallet, 全球法布施 core, Hermes Studio engine, Magpie 47+ presets & Rosetta/masking engine, Computer Control/Action Bus, and comprehensive `cargo test` suite.
   - Wire Cargo cross-build (`aarch64-linux-android`, `x86_64-linux-android`, plus `x86_64-unknown-linux-gnu` for JVM unit tests) into Gradle.
2. **Workstream 2 — Mika Multi-Module Integration (`:mika-core` + `:mika-agent`)**:
   - Port `:mika-core` (`core/src/main/kotlin`, `proto`, `resources/prompts`, `resources/skills`, `resources/knowledge`, `extractTraceSchemas`, `knowledgeIndex`, testFixtures & unit tests) and `:mika-agent` (`app/src/main/java`, `aidl`, `assets`, `jniLibs/arm64-v8a`, `res` minus duplicate `ic_launcher`).
   - Make `MikaApp` `open class`, update `NoticeFilter` & `AppClassifier` for `com.ombhrum.fabushi`, align `AndroidManifest.xml` (removing duplicate `LAUNCHER` intent-filter and moving `REQUEST_INSTALL_PACKAGES` to `githubRelease`), and align Gradle dependencies with AGP `9.3.0` + Kotlin `2.3.21` + OkHttp `5.4.0`.
3. **Workstream 3 — Unified Telegram Android UI + GPUI-Telegram Surfaces + Mika Bridge (`:app`)**:
   - Create `FabushiApplication : MikaApp()`.
   - Build comprehensive 1:1 Telegram Android Compose UI (`com.ombhrum.fabushi.telegram.*`): Theme engine (`Day`, `Night`, `Tinted`, `Classic`, custom accents, wallpapers), Navigation Drawer & Multi-Account switcher, Folder Tabs & Stories bar & Swipeable Chat List, Rich Chat Room (UTF-16 Markdown entities, formatting bar, swipe-to-reply, forward, edit, delete, pin banner, scheduled/silent/TTL send, inline keyboards, slash commands, reactions, stickers/GIFs, 5-bit 100-bar voice waveform recorder/player + floating audio bar, round video notes, polls/quizzes, forum topics, channel comments & stats, E2EE Secret Chat fingerprint dialog), VoIP/Video Call & Talk Mode VAD Overlay, TON Wallet screens, BotFather & 全球法布施 2D/3D Globe + Zen Room Mini App, Hermes 6-Tab Studio, Magpie Gateway Console, Cloud Computer Workbench, Passcode/Biometric Lock, and all 11 Telegram Settings screens.
   - Implement `TelegramMikaChannelBridge` + `@MikaPhoneBot` + embedded Mika Phone Control Center & Floating Capsule integration, plus `FabushiRemoteDeviceGateway` capability-gated `fabushi.phone.*` status/dispatch tools.
   - Update `AndroidUpdateViewModel` for `fabu-android/fabushi-android` releases and add full R8 `proguard-rules.pro`.
4. **Workstream 4 — Verification, CI/CD & GitHub `v2.0.0` Release**:
   - Run full Rust `cargo test`, `:mika-core:test`, `:app:testDebugUnitTest` (including live JNI tests against `libmahayana_app_host.so`!), `:app:compileDebugAndroidTestKotlin`, `:app:lintDebug`, and signed `:app:assembleDebug` + `:app:assembleRelease` + `:app:assembleGithubRelease`.
   - Create `.github/workflows/ci.yml` & `.github/workflows/release.yml`, push to `fabu-android/fabushi-android`, verify GitHub Actions green, and publish GitHub Release `v2.0.0` with signed APKs, `fabushi-android-update.json`, and `SHA256SUMS`.

## 12. Verification / test strategy

- **Rust Unit & Integration Tests**: `cargo test` in `mobile/android/rust/mahayana_android_host` covering all `MahayanaHost` baseline journeys (`MahayanaFeatureHostTest` parity!), SQLite WAL persistence, E2EE X3DH/Double-Ratchet/Sender-Keys + emoji fingerprints, 5-bit waveform codec, TON BIP39/BOC/TonConnect, 全球法布施 state machine, Hermes engine, Magpie routing/masking, and Action Bus.
- **JVM JNI End-to-End Tests**: `./gradlew :app:testDebugUnitTest` loading host `libmahayana_app_host.so` via `java.library.path` to test Kotlin `MahayanaHost`, `MessagingViewModel`, `TelegramViewModel`, `TelegramMikaChannelBridge`, `AndroidVersionTest`, and `FabushiAppAgentSurfaceTest` against the real Rust library.
- **Mika Core & Agent Verification**: `./gradlew :mika-core:test :mika-core:knowledgeIndex :mika-agent:compileDebugKotlin :mika-agent:testDebugUnitTest`.
- **Android UI Test Compilation & Packaging Verification**: `./gradlew :app:compileDebugAndroidTestKotlin :app:assembleDebug :app:assembleRelease :app:assembleGithubRelease`, followed by `unzip -l` / `apksigner verify` / `readelf -l` verification confirming `libmahayana_app_host.so` (16KB aligned), `libmikarun.so`, `libproot.so`, `assets/runtime/rootfs.tar.xz` (STORED uncompressed), `classes.dex`, and valid v2/v3 APK signatures.
- **GitHub CI & Release Verification**: Push to `fabu-android/fabushi-android`, verify `.github/workflows/ci.yml` passes on GitHub-hosted runners, and verify `gh release view v2.0.0` exposes downloadable signed APKs + `fabushi-android-update.json` + `SHA256SUMS`.

## 13. Acceptance criteria / Definition of Done

- **AC-1**: Durable Spec `docs/specs/gpui-telegram-android-mika-fusion.md` created and compliant with `docs/specs/SPEC_TEMPLATE.md` before product-affecting code edits.
- **AC-2**: `mobile/android/rust/mahayana_android_host` compiles cleanly for `aarch64-linux-android`, `x86_64-linux-android` (16KB page-aligned), and `x86_64-unknown-linux-gnu`, passing all `cargo test` suites.
- **AC-3**: All 28 baseline `MahayanaHost` JSON-RPC contracts and all 14 existing tests (`AndroidVersionTest`, `FabushiAppAgentSurfaceTest`, `FabushiScreenTest`, `MahayanaFeatureHostTest` equivalents) pass with 100% backward compatibility.
- **AC-4**: Telegram Android 1:1 UI/UX parity implemented in Jetpack Compose (Drawer, Multi-Account, Stories, Folder Tabs, Archive, Swipe gestures, Rich Markdown Bubbles, Formatting Bar, Reply/Forward/Edit/Delete/Pin/Scheduled/Silent/TTL, Inline Keyboards, Slash Commands, Stickers/GIFs/Reactions, 5-bit Voice Waveform Recorder/Player + Floating Audio Bar, Round Video Notes, Lightbox Media Viewer, Polls/Quizzes, Locations/Contacts, Forum Topics, Channels/Comments/Stats, E2EE Secret Chats + Emoji Fingerprints, VoIP/Video Call & Talk VAD Overlay, Global Search, Passcode/Biometric Lock, Themes/Wallpapers, 11 Settings pages).
- **AC-5**: All GPUI-Telegram & Fabushi special capabilities implemented and wired end-to-end (`@BotFather`, Mini Apps + 全球法布施 2D/3D Earth Globe & Zen Room Prayer Wheel, TON Wallet + Collectibles + TonConnect, Hermes 6-Tab AI Studio, Magpie 47+ Provider No-Subscription Gateway + PII Masking, Multi-Device Cloud Computer Workbench + Action Bus + Tasks FSM).
- **AC-6**: `:mika-core` and `:mika-agent` modules fully integrated into `mobile/android` with zero resource/manifest collisions, `FabushiApplication : MikaApp()`, `IMikaShell` UID 2000 Binder helper, `MikaAccessibilityService` (with `Fast/Balanced/HumanLike` + `carefulApps` pacing), Wireless ADB SPAKE2/TLS pairing, `VirtualDisplay` + `AgentPip`, `TYPE_ACCESSIBILITY_OVERLAY` Floating Capsule + `CapsuleTileService`, `ConfirmationGate` + `Takeover`, `Teach` & zero-LLM `Replayer`, Headless WebView + CDP, Alpine `proot` Linux Sandbox + `TerminalActivity`, 5-backend Hold-to-Talk ASR, `MikaInCallService`, `NoticeListenerService` (`com.ombhrum.fabushi` self-filtered), `SelfCheck`, `Workflows`, `SavedChecklists`, `AlarmManager` global schedules, and WeChat/MCP/Relay connectors.
- **AC-7**: `TelegramMikaChannelBridge` natively connects Telegram chats (`@MikaPhoneBot`, Saved Messages, `/mika` commands) and `FabushiRemoteDeviceGateway` with Mika's `TaskService`, streaming live steps, confirmation gates, and results.
- **AC-8**: Signed `debug`, minified `release`, and minified `githubRelease` APKs build cleanly (`./gradlew assembleDebug assembleRelease assembleGithubRelease`), pass `apksigner verify`, bundle 16KB-aligned `libmahayana_app_host.so` + `libmikarun.so`/`libproot.so` (`useLegacyPackaging = true`) + uncompressed `rootfs.tar.xz`.
- **AC-9**: `.github/workflows/ci.yml` and `.github/workflows/release.yml` committed, all changes pushed to `fabu-android/fabushi-android`, GitHub Actions CI verified, and GitHub Release `v2.0.0` published with downloadable APKs, `fabushi-android-update.json`, and `SHA256SUMS`.

## 14. Release / migration / rollback

- **Versioning**: Bump default version to `2.0.0` (`versionCode = 20000`), tagging `v2.0.0` on `fabu-android/fabushi-android`.
- **Release Signing**: Generate deterministic release keystore from CI secrets (`ANDROID_KEYSTORE_*`) when configured, or generate/use a dedicated release signing keystore during automated release packaging (never committing private keystores to git) and emit `fabushi-android-update.json` + `SHA256SUMS`.
- **Migration**: Existing SharedPreferences (`fabushi.mobile`) and `filesDir` SQLite databases remain backward-compatible; Mika runtime initializes lazily under `filesDir/sandbox` and `filesDir/mika` without disturbing existing Fabushi session files.
- **Rollback**: Git tag/release rollback via `gh release` or `AndroidUpdateViewModel` version pinning; `feature.plugin.rollback` preserved for Mini Apps.

## 15. Observability / evidence

- Structured JSONL gateway audit trace (`device-gateway-trace.jsonl`), Mika OpenTelemetry `RunTrace` (`TraceDisk` / `OtlpSender`) & `run-metrics.jsonl` + `UsageLedger` token/cost accounting, and Rust `MahayanaHost` structured audit events (`audit.list`).
- Build & release verification evidence recorded in Section 17 (`Spec compliance record`) prior to completion.

## 16. References / provenance

- Upstream GPUI-Telegram & Fabushi Desktop/Server: `https://github.com/bhrum/gpui-telegram` (`c404943`, Apache-2.0)
- Upstream Mika Android AI Phone Agent: `https://github.com/wangzan101/Mika` (`71e00f5`, v0.1.45, Apache-2.0; bundled Alpine/proot third-party notices preserved in `licenses/`)
- Target Canonical Android Repository: `https://github.com/fabu-android/fabushi-android`
- Repository Governance: `AGENTS.md`, `docs/specs/spec-first-ai-development.md`

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 | blocked | Post-`v2.0.0` audit found `TelegramViewModel.buildInitialSeedState()` (`lines 2671-3123`) fabricates fake accounts/chats, `switchAccount` bleeds identical chat/wallet state across accounts, Drawer Contacts only selects a folder instead of opening a Contacts/Add-Friend screen, `TelegramIntroAuthScreen` is missing, and `.imePadding()` is absent across all edge-to-edge screens. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R4`, `R12`–`R14`). |
| R2 | blocked | Post-`v2.0.0` audit found message edits/deletes/pins/reactions/polls mutate in-memory `_state` without live server sync, checkmarks (`🕒 -> ✓ -> ✓✓`) use fake `delay(260)`/`delay(650)` timers even offline, `votePoll` sends `"optionId"` instead of integer `"optionIndex"`, stickers/GIFs/attachments render emoji/placeholder cards, and `translateMessage` prepends a fake string prefix. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R5`, `R10`, `R14`). |
| R3 | blocked | Post-`v2.0.0` audit found voice recording slide-to-cancel/lock overwrites per-frame delta (`TelegramComposer.kt:479`, `TelegramViewModel.kt:1772`) instead of accumulating drag distance, `ROUND_VIDEO_NOTE` records 0 bytes without opening CameraX, `startCall` runs a cosmetic timer without WebRTC audio/video, and Talk Mode sends `transcript` text instead of PCM to `vadFeedPcm` and returns canned text. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R6`, `R12`, `R14`). |
| R4 | blocked | Post-`v2.0.0` audit found Global Search lacks the `APPS` tab and `PackageManager` installed-app discovery, `setGlobalSearch` discards `telegram.messaging.searchGlobal` FTS5 results, tapping search hits does not scroll to the matched message, in-chat search destructively hides context without `▲`/`▼` jump controls, and `openChannelStats` returns hardcoded numbers. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R5`, `R12`–`R14`). |
| R5 | blocked | Post-`v2.0.0` audit found `createSecretChat` loops local Alice/Bob keys instead of remote peer X3DH prekey exchange, Settings `DEVICES_SESSIONS` renders 2 static text lines without active session list/revocation or QR scanner, `PRIVACY_SECURITY` lacks Cloud 2FA/Passkeys/Blocked Users UI, and the language switcher leaves UI strings hardcoded in Simplified Chinese. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R4`, `R6`, `R14`). |
| R6 | blocked | Post-`v2.0.0` audit found `mahayana_android_host` reused zero crates from `bhrum/gpui-telegram`: `configureMagpieProvider` drops `apiKey` without saving to Keystore or Rust, `magpie.chatComplete` defaults `liveUpstream = false` canned echo, `@BotFather`/Hermes `/moa` & TTY/Dharma AI return hardcoded strings, `executeComputerOp` omits lease `generation`, `TonWalletScreen.kt:475` draws unscannable SHA-256 bits instead of ISO-18004 QR, Wallet seeds fake balances and ignores Rust Ed25519 addresses, and Global Dharma bypasses `MiniAppPlatformBridge`. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R3`, `R6`, `R7`, `R10`–`R12`). |
| R7 | passed | Verified `v2.0.0` JNI symbol ABI (`Java_com_ombhrum_fabushi_core_MahayanaHost_{nativeCreate,nativeCreateTest,nativeDispatch,nativeDestroy}`), 16KB ELF linker flags, and baseline 28 `MahayanaHost` JSON-RPC contracts (`cargo test` `33/33`, `MahayanaHostJniIntegrationTest`). Note: `mika.registerTaskEvent` and live `gpui_telegram_server` WS/HTTP + `bhrum/gpui-telegram@889246e` headless crate reuse were stubbed/absent in `v2.0.0` and are tracked as `blocked` until completed under `docs/specs/gpui-telegram-android-production-parity.md` (`R3`–`R7`, `R11`). |
| R8 | passed | Verified initial Gradle multi-module import of `:mika-core` and `:mika-agent` (`wangzan101/Mika@71e00f5` snapshot, `:mika-core:knowledgeIndex`, `:mika-core:test`, `:mika-agent:testDebugUnitTest`). Sync of the subsequent 25 upstream commits (`wangzan101/Mika@b899c5b`, 80 missing + 101 modified main files) and dynamic `:mika-agent` `BuildConfig` versioning are tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R2`, `R8`, `R9`). |
| R9 | blocked | Local `:mika-core` and `:mika-agent` trail `wangzan101/Mika@b899c5b` by 25 commits (missing `IMikaShell.aidl` tx 61 `startShareFile`, `#622` targeted UID VirtualDisplay Back + `BackAffordance`, `#483` `appops PLAY_AUDIO deny`, `#237` blind-UI stop rule, `#474` vision retry, `mikarun.c` + updated `libmikarun.so`, `AudioCapability`/`TranscribeCapability`, `PocketGuardManager`, `GrantedFolderFiles`, `PersonaStore`, `SkillMarket`, `WebFetch`/`TinyFish`, `VoiceBriefingController` + `CallDtmfExecutor`, and `ReportFilesProvider`). Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R8`, `R9`). |
| R10 | blocked | `TelegramMikaChannelBridge` did not implement `com.wangzan101.mika.core.run.Channel` or register with `ChannelGateway`/`Outbox`/`RunRecovery`, lacked Magpie ↔ `MikaApp.providers` credential sync (`!status.hasMainModel` failure), opened raw Developer Settings without `PairingCoordinator`, ignored `WaitingTakeover`/`WaitingInput`, omitted `Delivery.Image` screenshots, pre-seeded fake demo messages in `@MikaPhoneBot`, and left `CloudComputer` `node-local-android` disconnected from `TaskService`. Superseded and tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R10`, `R11`). |
| R11 | passed | Verified `v2.0.0` packaging structure (`useLegacyPackaging = true`, uncompressed `ZIP_STORED` `assets/runtime/rootfs.tar.xz`, `proguard-rules.pro`, and `REQUEST_INSTALL_PACKAGES` isolated to `githubRelease`). Note: repo-clean deterministic signing (eliminating `repo_root/release-signing/ephemeral-release.p12`) is tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R2`). |
| R12 | passed | Historical `v2.0.0` (`versionCode = 20000`) tag and CI/Release workflows were published on `fabu-android/fabushi-android`. Note: `release.yml` hardcoded `versionCode = 20000` on subsequent tag pushes and `:mika-agent` hardcoded `2.0.0`/`20000`; monotonic `versionCode` derivation and `v2.1.0` (`versionCode >= 20100`) delivery are tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`R2`, `R15`). |
| AC-1 | passed | Initial `v2.0.0` Spec created at `docs/specs/gpui-telegram-android-mika-fusion.md`; now superseded by `docs/specs/gpui-telegram-android-production-parity.md`. |
| AC-2 | passed | Baseline `mahayana_android_host` compiled for `aarch64-linux-android`, `x86_64-linux-android`, and `x86_64-unknown-linux-gnu` (`33/33` unit tests). Build-time API 29 alignment + automatic 16KB ELF `PT_LOAD` enforcement and `bhrum/gpui-telegram@889246e` crate integration are tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-2`, `AC-3`). |
| AC-3 | passed | Baseline 28 `MahayanaHost` JSON-RPC contracts and legacy `FabushiScreen` / `AndroidVersionTest` / `FabushiAppAgentSurfaceTest` compatibility verified. |
| AC-4 | blocked | Blocked due to fake `buildInitialSeedState()`, missing `TelegramIntroAuthScreen` / `TelegramContactsScreen` / `TelegramPeerProfileScreen` / `APPS` search tab, missing `.imePadding()`, broken cumulative voice drag math, competing bubble gestures, timer-only calls/checkmarks, and cosmetic language switching; tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-4`–`AC-6`, `AC-12`–`AC-14`). |
| AC-5 | blocked | Blocked due to dropped Magpie API keys, `liveUpstream = false` canned AI/Hermes/MoA/Bot/Dharma responses, missing `generation` in `computer.executeOp`, unscannable SHA-256 TON QR canvas, fake wallet balances, and bypassed `MiniAppPlatformBridge`; tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-6`, `AC-7`, `AC-10`, `AC-12`). |
| AC-6 | blocked | Blocked pending 25-commit sync to `wangzan101/Mika@b899c5b` (`80` missing + `101` modified main files across `:mika-core` and `:mika-agent`); tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-8`, `AC-9`). |
| AC-7 | blocked | Blocked because `TelegramMikaChannelBridge` does not implement `Channel`/`ChannelGateway`/`Outbox`/`RunRecovery`, drops `Approve`/`Deny`/`Takeover`/`Input` actions and screenshots, lacks Magpie ↔ `ProviderStore` key sync, and does not activate SPAKE2 `PairingCoordinator`; tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-10`, `AC-11`). |
| AC-8 | passed | Verified `v2.0.0` APK packaging structure (`libmahayana_app_host.so`, `libmikarun.so`, `libproot.so`, `libproot_loader.so`, `ZIP_STORED` `rootfs.tar.xz`). Repo-clean deterministic keystore signing and `v2.1.0` APK verification are tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-2`, `AC-15`). |
| AC-9 | passed | Historical `v2.0.0` GitHub Release published; monotonic `versionCode` pipeline fix and `v2.1.0` (`versionCode >= 20100`) release delivery are tracked in `docs/specs/gpui-telegram-android-production-parity.md` (`AC-2`, `AC-15`). |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
