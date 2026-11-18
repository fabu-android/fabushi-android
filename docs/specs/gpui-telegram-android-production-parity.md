# GPUI-Telegram Android Production Parity, 1:1 Telegram Android UX & Full Mika Fusion — Specification

Status: active
Owner: fabu-android
Last updated: 2026-11-18
Related issue/task/PR: Production parity overhaul superseding `docs/specs/gpui-telegram-android-mika-fusion.md` (`v2.0.0` -> `v2.1.0`, `bhrum/gpui-telegram@889246e4621e3e8e58022895a887c63ea3130bdd` + `wangzan101/Mika@b899c5bb01acd80cc29f0cc25cad189a8cc20b46`)

## 1. Context / problem

Although `docs/specs/gpui-telegram-android-mika-fusion.md` (`v2.0.0`) established the initial multi-module Android workspace (`:app`, `:mika-core`, `:mika-agent`, `mobile/android/rust/mahayana_android_host`), a comprehensive line-by-line audit triggered by the latest explicit user mandate (`ORIGINAL_REQUEST.md`) identified systemic gaps between the shipped `v2.0.0` build and genuine production parity:

1. **Zero Headless Crate Reuse from `bhrum/gpui-telegram` & Pervasive Mock Handlers**:
   - `mobile/android/rust/mahayana_android_host/Cargo.toml` imported zero crates from upstream `bhrum/gpui-telegram@889246e4621e3e8e58022895a887c63ea3130bdd`, even though 11 of its 13 workspace crates (`gpui_telegram_core`, `gpui_telegram_proto`, `gpui_telegram_client`, `gpui_telegram_media`, `gpui_telegram_webrtc`, `gpui_telegram_agent`, `magpie_gateway`, `fabushi_miniapp`, and `inherited/{mahayana-messaging,host,node-agent-coordinator,mahayana-rs/*}`) have zero dependency on desktop `gpui = 0.2.2` and zero OpenSSL dependency.
   - Instead, `mahayana_android_host` lacked an async `tokio` runtime, had zero live WebSocket/HTTP transport to `gpui_telegram_server`, defaulted `magpie.chatComplete` to `liveUpstream = false` canned echo text, returned static canned strings from `lib.rs` `build_agent_chat_reply`, and omitted `mika.registerTaskEvent`.
2. **Fake Seeded State, Broken Auth/Sync & Missing App Search in `:app`**:
   - `TelegramViewModel.buildInitialSeedState()` (`TelegramViewModel.kt:2671-3123`) and `messaging.rs:148-259` fabricated 2 fake logged-in accounts, 8+ fake chats with mismatched IDs, fake `25.00`/`128.64 TON` balances, fake stories, and fake remote computer nodes, while omitting any real `TelegramIntroAuthScreen` (Phone OTP, Email OTP, Password, QR Pairing, 2FA, WebAuthn Passkeys).
   - Multi-account switching swapped only header metadata while bleeding identical chat/wallet state across accounts; checkmarks (`🕒 -> ✓ -> ✓✓`) advanced via hardcoded `delay(260)` / `delay(650)` timers even offline; `votePoll` sent `"optionId"` instead of integer `"optionIndex"`; `executeComputerOp` omitted `"generation"`.
   - Unified Global Search lacked an **`APPS` (`应用`)** tab, never queried Android `PackageManager` for installed launchable apps or Mika clone `userId` copies/aliases, never searched Marketplace Mini Apps or `@username` public directory, discarded `telegram.messaging.searchGlobal` FTS5 results, and failed to scroll to matched messages.
3. **Dropped AI Credentials & Disconnected Mika Phone Control (`wangzan101/Mika@b899c5b` Delta)**:
   - `TelegramViewModel.configureMagpieProvider` discarded `apiKey` without persisting it to Android Keystore or syncing with `MikaApp.providers` (`ProviderStore`), causing `@MikaPhoneBot` (`TelegramMikaChannelBridge.submitPhoneTask`) to fail `!status.hasMainModel` after rendering fake step logs, while Translate, Dharma AI, `/moa`, Hermes TTY, Custom Bots, and Talk Mode returned hardcoded demo strings.
   - `TelegramMikaChannelBridge` did not implement `com.wangzan101.mika.core.run.Channel` or register with `ChannelGateway`, `Outbox`, and `RunRecovery`, could not deliver `Delivery.Image` screenshots or inline `Approve`/`Deny`/`Takeover`/`Stop` actions, and launched raw Developer Settings without activating Mika's SPAKE2 `PairingCoordinator`.
   - Local `:mika-core` and `:mika-agent` were forked at `71e00f5` (25 commits behind `wangzan101/Mika@b899c5bb01acd80cc29f0cc25cad189a8cc20b46`), missing 80 main files and 101 modified main files including `IMikaShell.aidl` tx 61 `startShareFile`, `#622` targeted UID `VirtualDisplay` Back + `BackAffordance`, `#483` `appops PLAY_AUDIO deny`, `#237` blind-UI stop rule, `#474` vision retry, `mikarun.c` + updated `libmikarun.so` (`--pty`), `AudioCapability` (`OpenAiSpeechModel`/`GeminiTtsModel`), `TranscribeCapability`, `PocketGuardManager` (`#679`), `GrantedFolderFiles` SAF (`#689`), `PersonaStore` (`#292`), GitHub `SkillMarket` (`#472`), `WebFetch`/`TinyFish` (`#671`), `VoiceBriefingController` + `CallDtmfExecutor` (`#670`), `ReportFilesProvider` (`#697`), and `ViewerFind`/`ScrollToBottom`.
4. **1:1 Telegram Android UX Bugs & CI/Release Pipeline Defects**:
   - `MainActivity` enabled `enableEdgeToEdge()`, yet zero Compose screens applied `.imePadding()`, causing the software keyboard to cover chat composers; voice recording slide-to-cancel/lock overwrote per-frame `dragAmount` instead of accumulating drag distance; chat bubbles had competing gesture detectors, squished 5-icon TopBars, no-op Peer Profile headers, placeholder media/GIF/sticker cards, unscannable SHA-256 QR canvases (`TonWalletScreen.kt:475`), 0-byte round video notes, and cosmetic-only language switching.
   - Cloud environment lacked Android SDK/NDK (`compileSdk = 37`, `ndk;27.2.12479018`) and Rust Android targets; `scripts/build_rust_android.sh` defaulted `ANDROID_API_LEVEL=26` without 16KB ELF `PT_LOAD` verification; `release.yml` hardcoded `versionCode = 20000` on tag pushes; `:mika-agent` hardcoded `2.0.0`/`20000`; and `scripts/package_android_release.py` generated ephemeral keystores inside `repo_root/release-signing`.

## 2. Goal

Deliver a 100% genuine, zero-fake-state **Fabushi Android (`v2.1.0`, `versionCode >= 20100`)** production application in `fabu-android/fabushi-android`:

1. **Direct Desktop Rust Core Reuse (`bhrum/gpui-telegram@889246e4621e3e8e58022895a887c63ea3130bdd`)**:
   - Vendor and wire all 11 portable headless Rust crates (`gpui_telegram_core`, `gpui_telegram_proto`, `gpui_telegram_client` with `rustls-tls-webpki-roots`, `gpui_telegram_media`, `gpui_telegram_webrtc`, `gpui_telegram_agent`, `magpie_gateway`, `fabushi_miniapp`, and `inherited/{mahayana-messaging,host,node-agent-coordinator,mahayana-rs/*}`) into `mobile/android/rust/` with an embedded multi-thread `tokio` runtime and `telegram.events.drain` event pump inside 16KB-aligned `libmahayana_app_host.so`.
   - Provide real authentication, per-account SQLite isolation + `GenerationFence`, live WebSocket + PTS `SyncGetDifference` + `client_nonce` durable outbox, 512KB CAS media transfer, 5-bit 100-bar waveform codec, pure-Rust `webrtc = 0.11` VoIP/video calls, remote peer X3DH/Double-Ratchet E2EE, real v4R2 Ed25519 TON Wallet + ISO-18004 QR matrix + live TON HTTP RPC, real 48+ provider Rosetta SSE `magpie_gateway` (`liveUpstream = true`), real 61-command `HermesEngine`/`AcpRunner`/`MoaCoordinator`/`TalkCoordinator`/`ComputerLeaseBroker`/`TaskFsm`, and 240+ country `fabushi_miniapp`.
2. **100% `wangzan101/Mika@b899c5bb01acd80cc29f0cc25cad189a8cc20b46` Parity & Native `TelegramMikaChannel`**:
   - Sync all 80 missing and 101 modified main files across `:mika-core` and `:mika-agent` to `wangzan101/Mika@b899c5b`.
   - Unify AI credentials via Android Keystore (`KeystoreKeyCipher`) with bidirectional sync between Magpie/Telegram and `MikaApp.providers` (`ProviderStore`), and implement `TelegramMikaChannel` (`implements com.wangzan101.mika.core.run.Channel`) registered in `ChannelGateway`, `Outbox`, and `RunRecovery`, plus Hermes TTY -> `AndroidSandbox` and `CloudComputer` local node + Cron -> Mika `TaskService`/`TriggerRuntime`.
3. **1:1 Telegram Android UX, Unified App Search & Deterministic Signed Release (`v2.1.0`)**:
   - Eliminate fake seeded state; add `TelegramIntroAuthScreen`, `TelegramContactsScreen`, `TelegramPeerProfileScreen`, and Global Search `APPS` tab (`InstalledAppSearchIndex`: `PackageManager` launchable apps + Mika clone `userId` copies + 36 app knowledge aliases + Marketplace MiniApps + built-in surfaces + FTS5 + `@username` directory, with 1-tap Open & 1-tap Mika Control).
   - Fix `.imePadding()`, cumulative voice drag-to-cancel/lock math, bubble gesture deduplication, real `NativeVoiceRecorder`/`NativeVoicePlayer`, `CameraX` 240dp round video notes, `<60ms` VAD + Mika ASR/LLM/TTS Talk Mode, WebRTC audio/camera, ZXing ISO-18004 QR + live TON RPC, `MiniAppPlatformBridge` + `GlobalDharmaCommercePanel`, real Settings & i18n, and publish signed GitHub Release `v2.1.0` (`versionCode = 20100`).

## 3. Non-goals / out of scope

- Implementing macOS/Windows/Linux desktop GPUI windowing (`crates/gpui_telegram_desktop` or `gpui = 0.2.2`) or iOS/Web client code in this repository (`AGENTS.md` repository ownership rule).
- Embedding the server-side `crates/gpui_telegram_server` binary inside the Android APK (`gpui_telegram_server` runs remotely/self-hosted and is accessed via `gpui_telegram_client`).
- Requiring root privileges or external third-party ADB apps for standard Mika Accessibility, Wireless ADB SPAKE2 pairing, WebView CDP, or Alpine PRoot sandbox execution.
- Breaking backward-compatible baseline `MahayanaHost` JNI symbols (`Java_com_ombhrum_fabushi_core_MahayanaHost_*`) or existing `FabushiScreen` `TestTags`.

## 4. Requirements

- **R1 (Durable Production Parity Spec & Git Hygiene — F1, M1)**: Create `docs/specs/gpui-telegram-android-production-parity.md` compliant with `docs/specs/SPEC_TEMPLATE.md`; repair `docs/specs/gpui-telegram-android-mika-fusion.md` (`Status: superseded`, zero false `passed` claims) and `docs/specs/spec-first-ai-development.md` (canonical `fabu-android/fabushi-android` & full `AGENTS.md` scope); configure `.git/info/exclude` to exclude `.agents/`, `PROJECT.md`, `TEST_INFRA.md`, `TEST_READY.md`, `mobile/android/local.properties`, generated `libmahayana_app_host.so` / `host-lib/`, keystores, `release-signing/`, and `dist/`.
- **R2 (Cloud Build Toolchain & CI/Release Pipeline Fixes — F2, M1)**: Provide idempotent, non-interactive `/home/antigravity/env-setup.sh` and `~/.bashrc` configuration installing Android SDK (`cmdline-tools/latest`, `platform-tools`, `platforms;android-37.0` / `android-36`, `build-tools;37.0.0` / `36.0.0` / `35.0.0`, `ndk;27.2.12479018`), `mobile/android/local.properties`, and Rust cross-targets (`aarch64-linux-android`, `x86_64-linux-android`) using `TMPDIR=/home/antigravity/.gemini/jetski/scratch/teamwork_tmp`; update `scripts/build_rust_android.sh` (`ANDROID_API_LEVEL=29`, `local.properties` NDK discovery, 16KB ELF `PT_LOAD` `p_align >= 16384` verification); fix monotonic semver `versionCode` derivation in `.github/workflows/release.yml` and `scripts/package_android_release.py`; dynamically read `-PversionName`/`-PversionCode`/`-PgitHash` in `mobile/android/mika-agent/build.gradle.kts`; and keep `repo_root` 100% free of signing keystores while preserving deterministic local/CI signing and `zipalign -P 16 -f 4`.
- **R3 (`bhrum/gpui-telegram@889246e` Headless Crate Integration & `tokio` JNI Bridge — F3, M2)**: Vendor/wire all 11 portable headless crates from `bhrum/gpui-telegram@889246e4621e3e8e58022895a887c63ea3130bdd` (`gpui_telegram_core`, `gpui_telegram_proto`, `gpui_telegram_client` with `rustls-tls-webpki-roots`, `gpui_telegram_media`, `gpui_telegram_webrtc`, `gpui_telegram_agent`, `magpie_gateway`, `fabushi_miniapp`, `inherited/{mahayana-messaging,host,node-agent-coordinator,mahayana-rs/*}`) into `mobile/android/rust/`, embed a multi-thread `tokio` runtime + `telegram.events.drain` subscription queue in `mahayana_android_host`, and eliminate all mock/echo Rust handlers.
- **R4 (Real Rust Auth, Multi-Account Session Persistence & Security RPCs — F4, M2)**: Wire `gpui_telegram_client` Phone OTP (`phone_send_code`/`phone_verify`), Email OTP (`email_send_code`/`email_verify`), Username/Password, QR Pairing (`qr_pair_init`/`qr_pair_poll`/`qr_pair_accept`), Cloud 2FA (`get_2fa_status`/`set_2fa`/`verify_2fa`), WebAuthn Passkeys (`passkey_challenge`/`passkey_verify`/`register_passkey`), `list_sessions`/`revoke_session`, and strict per-account SQLite isolation with `GenerationFence`.
- **R5 (Live WebSocket Hub, PTS `SyncGetDifference` & Durable Outbox — F5, M2)**: Replace fake local sync and timer-driven checkmarks with `gpui_telegram_client` `WsTransport`, atomic PTS gap recovery (`sync_chat_difference`/`sync_all_gaps`), `client_nonce` durable SQLite outbox with jittered exponential backoff, cloud drafts, server-synced folders, reactions, polls (`optionIndex`), forum topics, channel statistics, and JSON/HTML export.
- **R6 (Rust Media CAS, Waveform, Pure-Rust WebRTC, E2EE, TON Wallet & Dharma Core — F6, M2)**: Integrate resumable 512KB chunked SHA-256 CAS media upload/download + HTTP 206 range streaming, 5-bit 100-bar waveform codec (`gpui_telegram_media`), `gpui_telegram_webrtc` (`webrtc = 0.11` ICE/STUN/TURN/DTLS/SRTP), remote peer X3DH + Double Ratchet + Sender Keys E2EE (`send_encrypted_dm`), real v4R2 Ed25519 TON Wallet + ISO/IEC 18004 QR matrix (`wallet.generateQrMatrix`) + live TON HTTP RPC, and `crates/fabushi_miniapp` (240+ countries).
- **R7 (Rust Magpie Rosetta Gateway, Hermes 61-Command Engine & Computer Lease Broker — F7, M2)**: Replace canned `lib.rs`/`magpie.rs`/`hermes.rs`/`computer.rs` stubs with real `magpie_gateway` (`liveUpstream = true` by default across `openai_compat`, `openai_responses`, `anthropic`, `gemini` SSE streaming + PII masking + explicit error envelopes on missing/invalid keys) and `gpui_telegram_agent` (`AcpRunner`, 61-command `HermesEngine`, `MoaCoordinator`, `TalkCoordinator`, `TaskFsm`, `ComputerLeaseBroker` with `generation` fencing).
- **R8 (Upstream `wangzan101/Mika@b899c5b` Core & Shell/VirtualDisplay Sync — F8, M3)**: Sync all 37 missing + 37 modified `:mika-core` main files and 43 missing + 64 modified `:mika-agent` main files to `wangzan101/Mika@b899c5bb01acd80cc29f0cc25cad189a8cc20b46`, including `IMikaShell.aidl` tx 61 `startShareFile`, `#622` targeted top-task UID `VirtualDisplay` Back key injection + `BackAffordance` tap fallback, `#483` `appops PLAY_AUDIO deny`, `#237` blind-UI circuit breaker (`"该应用不响应自动化"`), `#474` `VISION_ATTEMPTS=2` transient retry, and `mikarun.c` + synced `jniLibs/arm64-v8a/libmikarun.so` (`--pty`).
- **R9 (Upstream Mika New Capabilities Integration — F9, M3)**: Integrate `AudioCapability` (`OpenAiSpeechModel`, `GeminiTtsModel`, `audio_generate`, `audio_save`, `speak`, `phone_share_audio`), `TranscribeCapability` (`audio_transcribe`), `PocketGuardManager` (`#679`), `GrantedFolderFiles` SAF read/write (`#689`), `PersonaStore` (`#292`), GitHub `SkillMarket` (`#472`), `WebFetch`/`TinyFish` (`#671`), `VoiceBriefingController` + `CallDtmfExecutor` (`#670`), `ReportFilesProvider` (`#697`) in `AndroidManifest.xml`, and `ViewerFind`/`ScrollToBottom`/code-block copy (`#691`, `#692`, `#694`).
- **R10 (Unified Keystore AI Credential Vault & Bidirectional Magpie ↔ `MikaApp.providers` Sync — F10, M4)**: Encrypt and persist Magpie/Telegram provider keys in Android Keystore (`KeystoreKeyCipher`), fix `configureMagpieProvider`/`importUri`/`importEnv` key-drop bug, synchronize active `main`/`vision`/`light`/ASR/TTS models bi-directionally with `MikaApp.providers` (`ProviderStore`) so `status.hasMainModel == true` immediately after configuring either surface, and route Translate, Dharma AI, Companion Bots, Hermes, and `/moa` to live streaming LLMs.
- **R11 (Genuine `TelegramMikaChannel` & Native Sandbox/Computer/Cron Execution — F11, M4)**: Implement `TelegramMikaChannel` (`implements com.wangzan101.mika.core.run.Channel`, `id = "telegram"`, `supportsImages = true`, `confirms = true`) registered in `MikaApp.channelGateway`, `Outbox`, and `RunRecovery`, mapping `Delivery.Progress`, `Delivery.Ask(Input | Confirm | Takeover)`, `Delivery.Image`, and `Delivery.Result` into persistent Telegram messages with working inline `Confirm`/`Deny`/`Takeover`/`Stop` callbacks, `PhoneSetupScreen` SPAKE2 Wireless ADB pairing (`openPhoneSetupInitially = true`), Hermes TTY -> `AndroidSandbox`, and `CloudComputer` local node + Cron -> Mika `TaskService`/`TriggerRuntime`.
- **R12 (Truthful State, Unified Installed App + MiniApp Search Engine & Real Media/Wallet/Dharma Wiring — F12, M4)**: Remove `buildInitialSeedState()` and `seed_starter_conversations_if_empty()` fake demo state; build `InstalledAppSearchIndex` (`InstalledAppRepository.searchApps`) merging `PackageManager` launchable activities + Mika `appCopies()` clone `userId`s + 36 Mika app knowledge aliases/tags + Marketplace MiniApps + built-in surfaces + SQLite FTS5 + `@username` peer lookup; wire real `NativeVoiceRecorder`/`NativeVoicePlayer`, `CameraX` 240dp round video notes, `<60ms` VAD + Mika ASR/LLM/TTS Talk Mode, WebRTC audio/camera routing, ZXing ISO-18004 QR + live TON RPC, and `MiniAppPlatformBridge` + `GlobalDharmaCommercePanel`.
- **R13 (1:1 Telegram Android Onboarding, Auth, Contacts, Add Bot & Unified Search UI with `APPS` Tab — F13, M5)**: Implement `TelegramIntroAuthScreen` (Phone/Email/QR/2FA/Passkey + server/proxy setup), leak-free multi-account switcher, `TelegramContactsScreen` (Add Friend by `@username`/phone/QR + 1:1 DM), Add Bot (`MobileBotViewModel` + `@BotFather`), `GlobalSearchResultsPane` with `APPS` (`应用`) tab exposing 1-tap `launchDirectly(packageName, userId)` and 1-tap `automateWithMika(packageName, label, goal)`, multi-select chat action bar, and spring swipe physics.
- **R14 (1:1 Telegram Android Chat, Composer, Peer Profile & Settings UX Polish — F14, M5)**: Apply `.imePadding()` across all edge-to-edge screens; fix cumulative slide-to-cancel/lock voice recording drag math and deduplicate bubble gesture detectors; add slim Chat TopBar + `TelegramPeerProfileScreen` for all chat types; implement non-destructive in-chat search with `▲`/`▼` match jump & search-hit scroll, sticky date headers & scroll-to-bottom FAB, grouped bubble tails & real photo/video/sticker/GIF rendering, real Settings (2FA, Passkeys, Active Sessions + QR scan, Proxy ping, Storage), and real multi-language string table.
- **R15 (Full E2E Verification Tiers 1–5, Signed Release Packaging & GitHub Release `v2.1.0` — F15, M6)**: Pass 100% unit, JNI, Compose, and opaque-box E2E tests (Tiers 1–4) plus Tier 5 adversarial hardening; build 16KB-aligned JNI + sequential R8 `assembleRelease` and `assembleGithubRelease`; perform mandatory `AGENTS.md` daemon/artifact cleanup; push to `fabu-android/fabushi-android`, verify green GitHub Actions CI, and publish signed GitHub Release `v2.1.0` (`versionCode >= 20100`).

## 5. Current state

At `fabu-android/fabushi-android@026971c` (`v2.0.0` baseline):
- `docs/specs/gpui-telegram-android-mika-fusion.md` erroneously records `passed` across `R1`–`R12` and `AC-1`–`AC-9`; `docs/specs/spec-first-ai-development.md` cites legacy `bhrumom/fabushi-android`.
- Cloud VM has OpenJDK 17 (`17.0.21`) and `rustc 1.99.1` (`x86_64-unknown-linux-gnu` in read-only `/usr/local/rustup`), while Android SDK/NDK (`ANDROID_HOME`, `ndk;27.2.12479018`), `mobile/android/local.properties`, and Rust cross-targets (`aarch64-linux-android`, `x86_64-linux-android`) are uninstalled.
- `scripts/build_rust_android.sh` defaults `ANDROID_API_LEVEL=26` without 16KB ELF `PT_LOAD` verification; `.github/workflows/release.yml` hardcodes `versionCode = 20000` on tag pushes; `mobile/android/mika-agent/build.gradle.kts` hardcodes `2.0.0`/`20000`; `scripts/package_android_release.py` writes ephemeral keystores inside `repo_root/release-signing`.
- `mahayana_android_host` uses standalone mock/SQLite modules rather than `bhrum/gpui-telegram@889246e` headless crates; `:mika-core`/`:mika-agent` trail `wangzan101/Mika@b899c5b` by 25 commits; `:app` runs on `buildInitialSeedState()` demo data without `APPS` search, Keystore Magpie↔Mika sync, real `TelegramMikaChannel`, or `.imePadding()`.

## 6. Target state

```text
fabushi-android/
├── .git/info/exclude              # Excludes .agents/, PROJECT.md, TEST_*.md, local.properties, generated .so, keystores, dist/
├── .github/workflows/
│   ├── ci.yml                     # Rust tests + API 29 16KB NDK build + :mika-core + :mika-agent + :app tests & Debug APK
│   └── release.yml                # Deterministic monotonic semver versionCode + sequential R8 release/githubRelease + GitHub Release
├── docs/specs/
│   ├── SPEC_TEMPLATE.md
│   ├── spec-first-ai-development.md               # Canonical fabu-android/fabushi-android & full AGENTS.md scope
│   ├── gpui-telegram-android-mika-fusion.md       # Status: superseded, truthful compliance record
│   └── gpui-telegram-android-production-parity.md # Master active spec (M1–M6, F1–F15)
├── scripts/
│   ├── build_rust_android.sh      # API 29 NDK build + local.properties discovery + strict 16KB ELF PT_LOAD check
│   └── package_android_release.py # Monotonic versionCode helper + repo-clean deterministic signing + zipalign -P 16
└── mobile/android/
    ├── rust/
    │   ├── gpui_telegram_vendor/  # Vendored headless crates from bhrum/gpui-telegram@889246e (zero gpui dep)
    │   └── mahayana_android_host/ # Multi-thread tokio JNI host + telegram.events.drain + zero mock/echo handlers
    ├── mika-core/                 # 100% synced to wangzan101/Mika@b899c5b (Audio, Transcribe, WebFetch, FolderFiles, Persona, BackAffordance)
    ├── mika-agent/                # 100% synced to wangzan101/Mika@b899c5b (dynamic version props, tx 61, UID Back, PLAY_AUDIO deny, PocketGuard, SkillMarket, Call Briefing/DTMF, ReportFilesProvider)
    └── app/                       # Truthful 1:1 Telegram Android Compose client + Keystore AI Vault + TelegramMikaChannel + InstalledAppSearchIndex
```

## 7. Architecture and ownership boundaries

1. **Build, Toolchain & Release Layer (`scripts/*`, `.github/workflows/*`, `/home/antigravity/env-setup.sh`, `mika-agent/build.gradle.kts` — Milestone M1)**:
   - Owns reproducible cloud/CI Android SDK 37 + NDK 27 + Rust Android target provisioning, API 29 16KB ELF native compilation checks, dynamic Gradle version injection across `:app` and `:mika-agent`, monotonic semver `versionCode` calculation, and repo-clean APK signing/verification.
2. **Rust Native Host Layer (`mobile/android/rust/**` — Milestone M2)**:
   - `mahayana_android_host` embeds a single multi-thread `tokio::runtime::Runtime` and delegates protocol, networking, PTS sync, durable outbox, CAS media, waveform, pure-Rust WebRTC (`webrtc = 0.11`), E2EE, TON Wallet, Rosetta AI gateway (`magpie_gateway`), 61-command `HermesEngine` / `AcpRunner` / `ComputerLeaseBroker`, and 240+ country `fabushi_miniapp` directly to headless crates from `bhrum/gpui-telegram@889246e4621e3e8e58022895a887c63ea3130bdd`.
3. **On-Device Mika Agent Layer (`mobile/android/mika-core/**`, `mobile/android/mika-agent/**` — Milestone M3)**:
   - Maintains 100% upstream parity with `wangzan101/Mika@b899c5bb01acd80cc29f0cc25cad189a8cc20b46` while preserving `open class MikaApp`, `com.ombhrum.fabushi` self-protection (`NoticeFilter`, `AppClassifier`), `EmbeddedMikaWorkspace`, and `com.ombhrum.fabushi.SCHEDULE`.
4. **Android Application Bridge, Search & 1:1 Telegram Compose UI Layer (`mobile/android/app/**` — Milestones M4 & M5)**:
   - M4 owns `AiCredentialVault` / `MikaCredentialBridge`, `TelegramMikaChannel`, `InstalledAppRepository` / `InstalledAppSearchIndex`, truthful `TelegramViewModel` state/sync, and real audio/camera/WebRTC/TON/Dharma bridges.
   - M5 owns 1:1 Telegram Android Compose screens (`TelegramIntroAuthScreen`, `TelegramHomeScreen` + `GlobalSearchResultsPane` `APPS` tab, `TelegramContactsScreen`, `TelegramChatScreen`, `TelegramComposer`, `TelegramPeerProfileScreen`, `TelegramSettingsScreen`), `.imePadding()`, gesture physics, and multi-language string resources.

## 8. Interfaces / contracts / schemas / data flow

### 8.1 JNI & JSON-RPC Contract (`mahayana_android_host` ↔ `:app`)
- **JNI Symbols (`com.ombhrum.fabushi.core.MahayanaHost` / `MahayanaNativeBridge`)**:
  - Preserves `Java_com_ombhrum_fabushi_core_MahayanaHost_{nativeCreate,nativeCreateTest,nativeDispatch,nativeDestroy}` (and optional `MahayanaNativeBridge.nativeRequest(requestJson: String): String` wrapper) returning strictly `{"ok": true, "result": ...}` or `{"ok": false, "error": {"code": "...", "message": "..."}}` — **never** silent fake success on missing credentials, network failure, or contract mismatch.
- **Live Event Drain**:
  - `telegram.events.drain`: `{"accountGeneration": u64, "afterSeq": u64}` -> ordered batch of `WsServerFrame` updates, PTS diffs, streaming SSE deltas (`magpie.streamDelta`, `hermes.streamDelta`), Outbox state transitions (`Pending -> Sent -> Read -> Failed`), and WebRTC call signals.
- **Canonical Fixed Field Names**:
  - `magpie.upsertProvider`: `{"id": String, "protocol": String, "baseUrl": String, "apiKey": String, "defaultModel": String, "enabled": Boolean}` (secret material encrypted via Android Keystore before persistence or supplied per-session from Keystore vault).
  - `magpie.chatComplete`: `{"providerId": String, "model": String, "messages": [...], "liveUpstream": true}` (`liveUpstream` defaults to `true`; supports `openai_compat`, `openai_responses`, `anthropic`, `gemini`).
  - `messaging.votePoll` / `telegram.messaging.votePoll`: `{"accountId": String, "chatId": String, "messageId": String, "optionIndex": Int}`.
  - `computer.executeOp`: `{"nodeId": String, "leaseId": String, "generation": u64, "op": {...}}`.
  - `wallet.generateQrMatrix`: `{"payload": String}` -> `{"size": Int, "modules": [Boolean]}` (ISO/IEC 18004 scannable QR matrix).

### 8.2 AI Vault, `TelegramMikaChannel` & App Search Contracts (`:app` ↔ `:mika-agent`)
- **Bidirectional Credential Sync (`MikaCredentialBridge`)**:
  - `syncFromMagpie(presetId, protocol, baseUrl, apiKey, modelId, roles)` writes Keystore-encrypted credentials into `MikaApp.providers` (`ProviderStore`), and `observeMikaProviders()` mirrors Mika provider updates back to Magpie/Telegram so `status.hasMainModel == true` holds across both surfaces after single-point configuration.
- **Native Mika `Channel` (`TelegramMikaChannel`)**:
  - Implements `com.wangzan101.mika.core.run.Channel` (`id = "telegram"`, `supportsImages = true`, `confirms = true`), registered in `MikaApp.channelGateway`, `Outbox`, and `MikaApp.runRecovery`.
  - Routes `Delivery.Progress`, `Delivery.Ask(Input | Confirm | Takeover)`, `Delivery.Image(file, caption)`, and `Delivery.Result` into durable Telegram messages and dispatches inline button actions (`ChannelInput.Confirm`, `ChannelInput.Text`, `ChannelInput.Stop`) plus `TakeoverActivity` / `WebTakeoverActivity` and `EmbeddedMikaWorkspace(openPhoneSetupInitially = true)`.
- **Installed App & MiniApp Discovery (`InstalledAppRepository`)**:
  - `searchApps(query: String): List<DiscoverableAppItem>` merges `PackageManager.queryIntentActivities(ACTION_MAIN + CATEGORY_LAUNCHER)` + Mika `appCopies()` multi-user clones (`userId`) + Mika's 36 built-in app knowledge aliases/tags + Marketplace Mini Apps + built-in Fabushi surfaces, exposing `launchDirectly(packageName, userId)` and `automateWithMika(packageName, label, goal)`.

### 8.3 Monotonic Version Code & Release Signing Contract (`release.yml` & `scripts/package_android_release.py`)
- **Semver Derivation Formula**:
  - Strip optional `android-` and `v` prefixes plus `-prerelease` / `+build` suffixes from tag `TAG` -> `MAJOR.MINOR.PATCH` (`MAJOR >= 2`, `0 <= MINOR <= 99`, `0 <= PATCH <= 99`), computing `versionCode = MAJOR * 10000 + MINOR * 100 + PATCH` (`v2.0.0 -> 20000`, `v2.1.0 -> 20100`, `v2.1.3 -> 20103`), failing closed on malformed tags or `versionCode < 20000`.
- **Repo-Clean Deterministic Signing**:
  - Keystore resolution order: (1) `ANDROID_KEYSTORE_PATH` existing file; (2) `ANDROID_KEYSTORE_BASE64` decoded into a `0700` `tempfile.TemporaryDirectory()` cleaned via `try...finally`; (3) if `--require-release-keystore` is set, fail closed with `RuntimeError`; otherwise persist/reuse a `0600` keystore under `${FABUSHI_SIGNING_CACHE_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/fabushi-android}/release-signing.p12` (`0700` dir) outside `repo_root` so local upgrades keep identical X.509 certificates and `repo_root` never contains keystore files.

## 9. Constraints and non-functional requirements

- **Zero-Swap Memory & Disk Discipline (`AGENTS.md`)**:
  - Cloud VM operates with `7.8 GiB` RAM, `0B` Swap, and `19 GiB` `/home/antigravity` disk.
  - Never write temporary downloads or build scratch files to `/tmp`; always export `TMPDIR=/home/antigravity/.gemini/jetski/scratch/teamwork_tmp` and remove SDK `.zip` archives immediately after extraction.
  - Run `./gradlew` with `--max-workers=1` (or `<=2` on CI) and bounded heaps (`-Xmx2816m` Gradle, `-Xmx1664m` Kotlin), run heavy R8 tasks (`:app:assembleRelease`, `:app:assembleGithubRelease`) sequentially, keep Cargo profiles lean (`debug = 0`, `strip = "debuginfo"`, `incremental = false`), and immediately stop `GradleDaemon`/`KotlinCompileDaemon` and clean `target/`, `dist/`, `release-signing/`, and generated `:app` `jniLibs` after every local verification run.
- **SDK, NDK & 16KB Page Alignment**:
  - `compileSdk = 37`, `targetSdk = 37`, `minSdk = 29`, `ANDROID_API_LEVEL = 29`, `ndk;27.2.12479018`.
  - Both `arm64-v8a/libmahayana_app_host.so` and `x86_64/libmahayana_app_host.so` MUST have all ELF `PT_LOAD` segments aligned to `p_align >= 16384` (`0x4000`), verified at build time (`scripts/build_rust_android.sh`) and APK packaging time (`zipalign -P 16 -f 4` + `check_elf_16kb_alignment`).
- **Security & Secret Hygiene**:
  - Never commit `libmahayana_app_host.so`, `local.properties`, `.agents/`, or any `.jks`/`.keystore`/`.p12` file to git.
  - Encrypt all API keys, OAuth tokens, and wallet mnemonics via Android Keystore (`KeystoreKeyCipher`) or zeroized memory; preserve `REQUEST_INSTALL_PACKAGES` isolation to `githubRelease` only.

## 10. Failure modes and edge cases

1. **Read-Only System `RUSTUP_HOME` on Cloud VM**: Container base images with `/usr/local/rustup` owned by `nobody:nogroup` reject `rustup target add`; `env-setup.sh` seeds writable `$HOME/.rustup` and `$HOME/.cargo` and persists `RUSTUP_HOME`/`CARGO_HOME` in `~/.bashrc` so Android cross-targets install and survive `/` resets.
2. **Unconfigured AI Provider or Offline Backend**: Calling Magpie, Hermes, Translate, Dharma AI, Talk Mode, or `@MikaPhoneBot` without an API key or network connectivity MUST return structured `CONFIGURATION` / `NETWORK` error states and actionable setup prompts—never fake echoed text or fake `✓✓` read receipts.
3. **Multi-Account Switch During In-Flight Requests**: `GenerationFence` increments on account switch and discards stale WebSocket frames, PTS sync responses, and streaming deltas from the previous account generation.
4. **VirtualDisplay Back Key & Audio Focus on OEM ROMs (`#622`, `#483`)**: Targeted top-task UID key injection falls back to untargeted -> shell -> `BackAffordance` tap, and `appops PLAY_AUDIO deny` prevents background VirtualDisplay apps from stealing foreground audio focus.
5. **Custom-Drawn / Game UI & Concurrent SAF Folder Edits (`#237`, `#689`)**: Blind Accessibility trees trip `StopRules` with explicit `"该应用不响应自动化"`; SAF folder writes verify SHA-256 optimistic concurrency before overwriting external edits.
6. **Malformed Release Tags or Missing CI Keystore**: `package_android_release.py --compute-version-code` rejects non-semver tags or `MAJOR < 2` / `MINOR > 99` / `PATCH > 99` with non-zero exit; `--require-release-keystore` fails closed when CI signing secrets are absent.

## 11. Implementation strategy

Execute sequentially across 6 dependency-ordered milestones (`PROJECT.md`):
- **Milestone M1 (F1, F2)**: Establish `.git/info/exclude` hygiene; publish this Spec and repair `gpui-telegram-android-mika-fusion.md` + `spec-first-ai-development.md`; implement and verify idempotent `/home/antigravity/env-setup.sh` + `~/.bashrc` + `local.properties`; fix `scripts/build_rust_android.sh`, `.github/workflows/release.yml`, `mobile/android/mika-agent/build.gradle.kts`, and `scripts/package_android_release.py`; verify baseline Rust & Gradle compilation and clean up daemons/artifacts.
- **Milestone M2 (F3–F7)**: Vendor and integrate all 11 headless `bhrum/gpui-telegram@889246e` crates into `mahayana_android_host` with `tokio` runtime, `telegram.events.drain`, real auth/sessions, live WS/PTS/Outbox, CAS media, waveform, pure-Rust WebRTC, peer E2EE, v4R2 TON + QR + RPC, Rosetta `magpie_gateway`, and 61-command `HermesEngine`/`ComputerLeaseBroker`.
- **Milestone M3 (F8, F9)**: Sync `:mika-core` and `:mika-agent` 100% to `wangzan101/Mika@b899c5b` (80 missing + 101 modified main files, `IMikaShell` tx 61, `mikarun.c` + `libmikarun.so`, Audio/TTS, Transcribe, PocketGuard, SAF Folders, Personas, SkillMarket, WebFetch/TinyFish, Call Briefing/DTMF, `ReportFilesProvider`).
- **Milestone M4 (F10–F12)**: Implement Keystore `AiCredentialVault` + bidirectional `MikaCredentialBridge`, genuine `TelegramMikaChannel` (`Channel` + `ChannelGateway` + `Outbox` + `RunRecovery`) + `AndroidSandbox`/`TaskService` routing, truthful unseeded `TelegramViewModel`, `InstalledAppSearchIndex`, and real Voice/Round-Video/Talk/WebRTC/TON/Dharma runtime wiring.
- **Milestone M5 (F13, F14)**: Deliver 1:1 Telegram Android Compose UX (`TelegramIntroAuthScreen`, `TelegramContactsScreen`, `TelegramPeerProfileScreen`, `GlobalSearchResultsPane` `APPS` tab, `.imePadding()`, cumulative voice drag math, slim TopBar, non-destructive in-chat jump search, sticky headers, grouped tails, real media/sticker/GIF rendering, Settings & i18n).
- **Milestone M6 (F15)**: Pass all unit, JNI, Compose, and E2E Suites (Tiers 1–5), build sequential R8 release APKs, verify 16KB alignment & signatures, execute mandatory memory cleanup, push to `fabu-android/fabushi-android`, verify CI, and publish GitHub Release `v2.1.0`.

## 12. Verification / test strategy

- **M1 Toolchain & Pipeline Verification**:
  - Run `bash /home/antigravity/env-setup.sh` twice consecutively (both exit `0`, zero duplicate `~/.bashrc` blocks).
  - Run `cargo test` in `mobile/android/rust/mahayana_android_host` and `bash scripts/build_rust_android.sh` (verifying `PT_LOAD` `p_align >= 0x4000` on `arm64-v8a` and `x86_64`).
  - Run bounded Gradle verification (`./gradlew --max-workers=1 -PversionName=2.1.0 -PversionCode=20100 :mika-core:test :mika-agent:compileDebugKotlin :app:compileDebugKotlin`) and confirm `com.wangzan101.mika.BuildConfig` emits `VERSION_NAME = "2.1.0"` and `VERSION_CODE = 20100`.
  - Verify `python3 scripts/package_android_release.py --compute-version-code v2.1.0` outputs `20100` (> `20000`) and rejects invalid tags, plus verify repo-clean external keystore signing behavior.
- **M2–M6 Comprehensive Verification**:
  - `cargo test` across `mahayana_android_host` + vendored `gpui-telegram` crates (zero mock/echo strings in `rg` audit).
  - `./gradlew --max-workers=1 :mika-core:test :mika-core:knowledgeIndex :mika-agent:testDebugUnitTest :app:testDebugUnitTest :app:compileDebugAndroidTestKotlin`.
  - Sequential `:app:assembleDebug`, `:app:assembleRelease`, `:app:assembleGithubRelease` + `scripts/package_android_release.py` (`apksigner verify`, 16KB ELF check, `ZIP_STORED` `rootfs.tar.xz`).
  - Full opaque-box E2E test suite (Tiers 1–4) + Tier 5 hardening + GitHub Actions CI + `gh release view v2.1.0`, followed immediately by `AGENTS.md` daemon/artifact cleanup (`jps -l` & `free -h`).

## 13. Acceptance criteria / Definition of Done

- **AC-1 (M1 — Specs & Git Hygiene)**: `docs/specs/gpui-telegram-android-production-parity.md` is active with all 17 `SPEC_TEMPLATE.md` sections; `docs/specs/gpui-telegram-android-mika-fusion.md` (`Status: superseded`) and `docs/specs/spec-first-ai-development.md` are repaired with zero false `passed` claims; `.git/info/exclude` keeps `git status -sb` free of `.agents/`, `PROJECT.md`, `TEST_*.md`, `local.properties`, keystores, and generated `.so` outputs.
- **AC-2 (M1 — Cloud Toolchain & CI/Release Fixes)**: `/home/antigravity/env-setup.sh` exits `0` on two consecutive runs using `TMPDIR=/home/antigravity/.gemini/jetski/scratch/teamwork_tmp`; `scripts/build_rust_android.sh` builds API 29 native libs and enforces 16KB ELF `PT_LOAD` alignment; `mika-agent` generates `BuildConfig` `2.1.0`/`20100` from `-PversionName`/`-PversionCode`; `release.yml` and `package_android_release.py` compute monotonic `versionCode` (`v2.1.0 -> 20100`) and never write keystores into `repo_root`; post-test `jps -l` and `free -h` confirm zero lingering Gradle/Kotlin daemons.
- **AC-3 (M2 — Headless `gpui-telegram` Rust Host)**: All 11 headless `bhrum/gpui-telegram@889246e` crates compile into 16KB-aligned `libmahayana_app_host.so` with `tokio` runtime and `telegram.events.drain`, passing all `cargo test` and baseline 28 `MahayanaHost` JNI contracts with zero canned echo handlers.
- **AC-4 (M2 — Real Rust Auth & Multi-Account Isolation)**: Phone OTP, Email OTP, Password, QR Pairing, 2FA, Passkeys, `list_sessions`/`revoke_session`, and `GenerationFence` per-account SQLite isolation pass unit and JNI integration tests without cross-account data bleed.
- **AC-5 (M2 — Live WS Sync & Durable Outbox)**: WebSocket frame stream, PTS `SyncGetDifference` gap catch-up, `client_nonce` durable outbox retry, cloud drafts, folders, reactions, `optionIndex` poll voting, forum topics, stats, and history export operate end-to-end without fake `delay(...)` checkmarks.
- **AC-6 (M2 — Rust Media, WebRTC, E2EE, TON & Dharma Core)**: 512KB CAS media upload/range stream, 5-bit 100-bar waveform, pure-Rust `webrtc = 0.11`, remote peer X3DH/Double-Ratchet E2EE, v4R2 Ed25519 TON Wallet + ISO-18004 QR matrix + live TON RPC, and 240+ country `fabushi_miniapp` pass verification.
- **AC-7 (M2 — Real Magpie Rosetta & Hermes/Computer Engine)**: `magpie_gateway` streams live OpenAI/Responses/Anthropic/Gemini SSE (`liveUpstream = true`) with explicit error reporting when unconfigured, and `gpui_telegram_agent` executes real 61-command `HermesEngine`, `MoaCoordinator`, `TalkCoordinator`, `TaskFsm`, and `ComputerLeaseBroker` (`generation` enforced).
- **AC-8 (M3 — `wangzan101/Mika@b899c5b` Core/Shell Sync)**: All 80 missing and 101 modified main files in `:mika-core` and `:mika-agent` are synced (`IMikaShell` tx 61, `#622` UID Back + `BackAffordance`, `#483` `PLAY_AUDIO deny`, `#237` blind-UI stop rule, `#474` vision retry, `mikarun.c` + `libmikarun.so`), passing `:mika-core:test`, `:mika-core:knowledgeIndex`, and `:mika-agent:testDebugUnitTest`.
- **AC-9 (M3 — Upstream Mika New Capabilities)**: `AudioCapability` (`OpenAiSpeechModel`/`GeminiTtsModel`), `TranscribeCapability`, `PocketGuardManager`, `GrantedFolderFiles`, `PersonaStore`, `SkillMarket`, `WebFetch`/`TinyFish`, `VoiceBriefingController` + `CallDtmfExecutor`, `ReportFilesProvider` manifest registration, and `ViewerFind`/`ScrollToBottom` compile and pass tests.
- **AC-10 (M4 — Unified Keystore AI Vault)**: Configuring an API key or OAuth provider in either Magpie/Telegram or Mika encrypts secrets via `KeystoreKeyCipher` and bi-directionally updates `MikaApp.providers` (`status.hasMainModel == true`), powering live LLM responses across Magpie, Hermes, MoA, Bots, Translate, and Dharma AI.
- **AC-11 (M4 — Genuine `TelegramMikaChannel` & Sandbox/Computer Wiring)**: `TelegramMikaChannel` (`implements Channel`) delivers live progress, `Ask(Input/Confirm/Takeover)` inline buttons, `Delivery.Image` screenshots, and results via `ChannelGateway`/`Outbox`/`RunRecovery`, launches SPAKE2 `PhoneSetupScreen`, and executes Hermes TTY in `AndroidSandbox` and `CloudComputer` local tasks/Cron in `TaskService`/`TriggerRuntime`.
- **AC-12 (M4 — Truthful State, `InstalledAppSearchIndex` & Real Media/Wallet/Dharma)**: Cold start contains zero fabricated accounts/chats/balances; `InstalledAppRepository.searchApps` indexes `PackageManager` launcher apps + Mika clones/aliases + MiniApps + FTS5 + `@username`; `NativeVoiceRecorder`/`NativeVoicePlayer`, `CameraX` 240dp round video notes, `<60ms` VAD + Mika ASR/LLM/TTS Talk Mode, WebRTC audio/camera, ZXing QR + live TON RPC, and `MiniAppPlatformBridge` + `GlobalDharmaCommercePanel` operate genuinely.
- **AC-13 (M5 — 1:1 Telegram Onboarding, Contacts, Bots & `APPS` Search UI)**: `TelegramIntroAuthScreen`, multi-account switcher, `TelegramContactsScreen` (Add Friend + 1:1 DM), Add Bot (`MobileBotViewModel` + `@BotFather`), `GlobalSearchResultsPane` `APPS` tab (1-tap Open & 1-tap Mika Control), multi-select chat bar, and spring swipes work end-to-end.
- **AC-14 (M5 — 1:1 Telegram Chat, Composer, Profile & Settings UX)**: All screens apply `.imePadding()`; voice slide-to-cancel/lock accumulates drag distance accurately; bubbles use a single non-competing gesture detector with grouped tails and real photo/video/sticker/GIF rendering; slim Chat TopBar opens `TelegramPeerProfileScreen`; in-chat search preserves context with `▲`/`▼` jump; sticky date headers, scroll-to-bottom FAB, real Settings, and multi-language i18n are verified.
- **AC-15 (M6 — E2E Pass, Signed Release & GitHub Release `v2.1.0`)**: 100% E2E Tiers 1–5 pass; signed `release` and `githubRelease` APKs pass R8, 16KB ELF alignment, and `apksigner verify`; post-test `jps -l` & `free -h` are clean; commits are pushed to `fabu-android/fabushi-android` with green CI and published GitHub Release `v2.1.0` (`versionCode >= 20100`).

## 14. Release / migration / rollback

- **Versioning**: Releases use monotonic `versionCode = MAJOR * 10000 + MINOR * 100 + PATCH` (`v2.1.0 -> 20100 > 20000`), injected identically into `:app` and `:mika-agent` via `-PversionName` / `-PversionCode` so `AndroidUpdateViewModel` and Android `PackageInstaller` cleanly upgrade existing `v2.0.0` (`20000`) installations.
- **Signing Continuity**: CI uses `ANDROID_KEYSTORE_BASE64` / `ANDROID_KEYSTORE_PATH` (materialized only in temporary runner memory/dir outside `repo_root`); local developer packaging reuses `${FABUSHI_SIGNING_CACHE_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/fabushi-android}/release-signing.p12` (`0600`) so repeated local builds maintain a consistent X.509 certificate without polluting git.
- **Migration**: Existing SQLite databases (`runtime.sqlite3`, Mika stores) migrate additively while purging/omitting hardcoded demo seed rows on unauthenticated accounts; legacy `SharedPreferences` (`fabushi.mobile`) remain readable.
- **Rollback**: Users and CI can roll back to prior signed tags (`v2.0.0`) on GitHub Releases or pin update channels via `fabushi-android-update.json`.

## 15. Observability / evidence

- Rust `MahayanaAndroidHost` structured audit log (`audit.list`) and `telegram.events.drain` sequence telemetry.
- Mika OpenTelemetry traces (`TraceDisk` / `OtlpSender`), `run-metrics.jsonl`, `UsageLedger` per-provider token/cost accounting, and 12-step `SelfCheckRunner` report export (`ReportFilesProvider`).
- Build-time `check_elf_16kb_alignment` diagnostics in `scripts/build_rust_android.sh` and `scripts/package_android_release.py`, plus post-test `jps -l` / `free -h` verification records in Section 17.

## 16. References / provenance

- Explicit User Request: `.agents/teamwork/ORIGINAL_REQUEST.md` (`2026-11-18T01:20:50Z`)
- Upstream Desktop & Headless Rust Reference: `https://github.com/bhrum/gpui-telegram` @ `889246e4621e3e8e58022895a887c63ea3130bdd` (Apache-2.0)
- Upstream On-Device Phone Agent Reference: `https://github.com/wangzan101/Mika` @ `b899c5bb01acd80cc29f0cc25cad189a8cc20b46` (Apache-2.0)
- Canonical Android Repository: `https://github.com/fabu-android/fabushi-android`
- Governance & Prior Spec: `AGENTS.md`, `docs/specs/spec-first-ai-development.md`, `docs/specs/gpui-telegram-android-mika-fusion.md`

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 | passed | Created `docs/specs/gpui-telegram-android-production-parity.md` (17 `SPEC_TEMPLATE.md` sections); repaired `docs/specs/gpui-telegram-android-mika-fusion.md` (`Status: superseded`, zero false `passed` claims) and `docs/specs/spec-first-ai-development.md` (`fabu-android/fabushi-android` + full `AGENTS.md` scope); updated `.git/info/exclude` and verified `git status -sb` excludes `.agents/`, `PROJECT.md`, `TEST_*.md`, `local.properties`, generated `.so` libs, keystores, `release-signing/`, and `dist/`. |
| R2 | passed | Implemented `/home/antigravity/env-setup.sh` + `~/.bashrc` + `mobile/android/local.properties` (`TMPDIR=/home/antigravity/.gemini/jetski/scratch/teamwork_tmp`, two consecutive runs exited `0`, `env -i` proxy preservation verified); updated `scripts/build_rust_android.sh` (`ANDROID_API_LEVEL=29`, `local.properties` NDK fallback, strict 64-bit `ET_DYN` + ABI `e_machine` + power-of-two `PT_LOAD` `p_align=0x4000` check passed on `arm64-v8a` & `x86_64`), `.github/workflows/release.yml` (`--resolve-release-metadata` + `=`-joined flags eliminating 64-bit bash overflow & `$GITHUB_ENV` injection) & `.github/workflows/ci.yml`, `mobile/android/mika-agent/build.gradle.kts` (`BuildConfig` `2.1.0`/`20100`/`v2.1.0` verified), and `scripts/package_android_release.py` (strict ASCII `\Z` SemVer + `--compute-version-code` / `--resolve-release-metadata`, strict 64-bit `ET_DYN` ELF check, unconditional keystore & bidirectional symlink/repo-root guards, `virtiofs`-safe `shutil.copyfile`, and `--out-dir` safety guards). |
| R3 | blocked | Blocked pending Milestone M2 implementation (`bhrum/gpui-telegram@889246e` headless crate integration & `tokio` JNI bridge). |
| R4 | blocked | Blocked pending Milestone M2 implementation (real Rust auth, sessions & per-account SQLite isolation). |
| R5 | blocked | Blocked pending Milestone M2 implementation (live WebSocket hub, PTS `SyncGetDifference` & durable outbox). |
| R6 | blocked | Blocked pending Milestone M2 implementation (512KB CAS media, waveform, pure-Rust WebRTC, peer E2EE, v4R2 TON + QR + RPC & 240+ country `fabushi_miniapp`). |
| R7 | blocked | Blocked pending Milestone M2 implementation (`magpie_gateway` live Rosetta SSE & 61-command `HermesEngine` / `ComputerLeaseBroker`). |
| R8 | blocked | Blocked pending Milestone M3 implementation (`wangzan101/Mika@b899c5b` `:mika-core` & `:mika-agent` core/shell/VirtualDisplay sync). |
| R9 | blocked | Blocked pending Milestone M3 implementation (upstream Mika Audio/TTS, Transcribe, PocketGuard, SAF Folders, Personas, SkillMarket, WebFetch/TinyFish, Call Briefing/DTMF & `ReportFilesProvider`). |
| R10 | blocked | Blocked pending Milestone M4 implementation (Android Keystore AI credential vault & bidirectional Magpie ↔ `MikaApp.providers` sync). |
| R11 | blocked | Blocked pending Milestone M4 implementation (`TelegramMikaChannel` `Channel`/`ChannelGateway`/`Outbox`/`RunRecovery` + `AndroidSandbox` & `TaskService` wiring). |
| R12 | blocked | Blocked pending Milestone M4 implementation (removal of `buildInitialSeedState()`, `InstalledAppSearchIndex` & real media/WebRTC/TON/Dharma wiring). |
| R13 | blocked | Blocked pending Milestone M5 implementation (`TelegramIntroAuthScreen`, `TelegramContactsScreen`, Add Bot & Global Search `APPS` tab UI). |
| R14 | blocked | Blocked pending Milestone M5 implementation (`.imePadding()`, cumulative voice drag math, `TelegramPeerProfileScreen`, non-destructive in-chat jump search, grouped bubbles & real Settings/i18n). |
| R15 | blocked | Blocked pending Milestone M6 implementation (100% E2E Tiers 1–5 pass, sequential R8 release APKs, git push & GitHub Release `v2.1.0`). |
| AC-1 | passed | Verified active 17-section Spec `docs/specs/gpui-telegram-android-production-parity.md`, repaired `gpui-telegram-android-mika-fusion.md` (`Status: superseded`) and `spec-first-ai-development.md`, and confirmed clean `git status -sb` via `.git/info/exclude`. |
| AC-2 | passed | Verified two consecutive `bash /home/antigravity/env-setup.sh` runs (`exit 0`) + `env -i` `.bashrc`/`env-setup.sh` proxy preservation, `cargo test -j 2 -- --test-threads=2` (`33/33` passed), `bash scripts/build_rust_android.sh` (API 29, `0x4000` 16KB `ET_DYN` `PT_LOAD` alignment on `arm64-v8a` & `x86_64`), `./gradlew --max-workers=1 -PversionName=2.1.0 -PversionCode=20100 :mika-core:test :mika-agent:compileDebugKotlin :app:compileDebugKotlin` (`BUILD SUCCESSFUL`, `BuildConfig` `2.1.0`/`20100`/`v2.1.0`), full adversarial matrices on `package_android_release.py` & `release.yml` (ASCII SemVer, 64-bit overflow/downgrade/`$GITHUB_ENV` rejection, 29-case ELF matrix, pre-signed keystore & symlink/repo-root enforcement, `virtiofs` copy, `--out-dir` guards), and post-cleanup `jps -l` / `free -h` (`7.0Gi` available, zero lingering daemons). |
| AC-3 | blocked | Blocked pending Milestone M2 implementation. |
| AC-4 | blocked | Blocked pending Milestone M2 implementation. |
| AC-5 | blocked | Blocked pending Milestone M2 implementation. |
| AC-6 | blocked | Blocked pending Milestone M2 implementation. |
| AC-7 | blocked | Blocked pending Milestone M2 implementation. |
| AC-8 | blocked | Blocked pending Milestone M3 implementation. |
| AC-9 | blocked | Blocked pending Milestone M3 implementation. |
| AC-10 | blocked | Blocked pending Milestone M4 implementation. |
| AC-11 | blocked | Blocked pending Milestone M4 implementation. |
| AC-12 | blocked | Blocked pending Milestone M4 implementation. |
| AC-13 | blocked | Blocked pending Milestone M5 implementation. |
| AC-14 | blocked | Blocked pending Milestone M5 implementation. |
| AC-15 | blocked | Blocked pending Milestone M6 implementation. |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
