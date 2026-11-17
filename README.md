# Fabushi Android (全球法布施 · GPUI-Telegram Android + Mika On-Device Phone Agent)

Canonical native Android repository fusing:

1. **[bhrum/gpui-telegram](https://github.com/bhrum/gpui-telegram) 1:1 Telegram Android Experience & Native Rust Engine (`v2.0.0`)**:
   - **Full Telegram Android UX in Jetpack Compose**: Slide-out Navigation Drawer, Multi-Account Switcher (`GenerationFence`), Expandable Stories Strip & Viewer, Horizontal Folder Filter Tabs, Collapsible Archived Chats, Swipe-to-Reply/Archive/Pin/Mute, Grouped Chat Bubbles with UTF-16 Rich Markdown/Entities & Spoiler Shimmer, Selection Formatting Bar, Multi-Message Forward/Edit/Delete/Revoke, Pinned Message Banner, Silent & Scheduled Send, Auto-Delete TTL, Inline Bot Keyboards & `/command` Autocomplete, Stickers/GIFs/Reactions, 5-Bit 100-Bar Scrubbable Voice Messages + Floating Speed-Control Audio Bar, Round Video Notes, Pinch-to-Zoom Lightbox Media Viewer, Polls & Quizzes, Forum Topics, Broadcast Channels with Discussion Comments & Analytics, Signal-Grade **X3DH + Double Ratchet + Sender Keys E2EE Secret Chats** (4-emoji & 333-emoji fingerprints), **1:1 & Group VoIP/Video Calls + Screen Share + `<60ms` VAD Barge-In Talk Mode**, Passcode & Biometric Lock, Global FTS5 Search, Custom Themes/Wallpapers (`Day Blue`, `Night`, `Night Tinted`, `Classic`), and 11 Telegram Settings pages.
   - **GPUI-Telegram & Fabushi Ecosystem**:
     - `@BotFather` & Companion AI Bots (`SandAgentProfile`)
     - **全球法布施 (`fabushi_miniapp`)**: Interactive 2D/3D 240-Country Earth Dharma Offering Globe + Zen Room Spinning Prayer Wheel, Sutras, Practice Sessions, Merit Leaderboard, Achievements & WebMCP Bridge
     - **TON Wallet & Telegram Stars**: 24-Word BIP39 Mnemonic, Ed25519 Keypair, BOC/Cell Builder, Send/Receive/QR, Swap, Staking, NFT Collectibles & TonConnect Emulation
     - **Hermes Multi-Provider 6-Tab AI Studio**: Sessions & Providers, Memory & Soul, Skills Hub, Toolsets & Terminal, Cron & Heartbeat, Kanban & Approvals, `/moa` Mixture-of-Agents
     - **Magpie No-Subscription AI Gateway**: 47+ Provider Presets, `magpie://import` & `.env` Import, Smart Routing (`Order/Rotate/Cheapest/Drain/Usage/Pace`), Rosetta OpenAI/Responses/Anthropic/Gemini Translator & PII/Secret Masking
     - **Cloud Computer Control & Schema-First Action Bus**: Multi-Device Nodes (`LocalDevice / CloudServer / CloudSandbox`), Exclusive CUA Lease & Emergency Human Takeover, Global Tasks 7-State FSM
2. **[wangzan101/Mika](https://github.com/wangzan101/Mika) Full On-Device Android Phone Control Fusion (`:mika-core` + `:mika-agent`)**:
   - **Multi-Display Accessibility Engine (`MikaAccessibilityService`)**: UI tree extraction, bundled offline **MLKit Chinese + Latin OCR** fusion, element actions, per-app speed pacing (`快 Fast / 标准 Balanced / 像人 HumanLike` + `carefulApps`), and self-protection.
   - **On-Device Wireless ADB + Standalone `UID 2000` `IMikaShell` Binder Daemon + Root Shell**: SPAKE2/TLS 1.3 Wireless Debugging pairing (`NsdAdbServiceFinder` + `PairingCoordinator`), offscreen **`VirtualDisplay` (`AgentDisplay`)** background app execution without hijacking the main screen, silent clipboard, call audio PCM bridge, and `BootRestoreReceiver` self-healing.
   - **Floating Capsule & Live `AgentPip`**: Zero-`SYSTEM_ALERT_WINDOW` magnetic `TYPE_ACCESSIBILITY_OVERLAY` Floating Capsule + Quick Settings `CapsuleTileService` + spring-snapping in-app Picture-in-Picture viewport.
   - **Safety Guardrails & Human Handover**: One-shot `ConfirmationGate` (`IrreversibleDetector`) + `TakeoverActivity` / `WebTakeoverActivity` for payments, logins, and 2FA.
   - **Teach/Record/Replay, Workflows, Checklists, Schedules, Web CDP & Linux Sandbox**: User demonstration recording & zero-LLM `Replayer`, fill-in task templates, 2–8 step sequential `Workflows` with result reuse, updatable `SavedChecklists`, `AlarmManager` global schedules with quiet hours, multi-tab headless WebView + Chrome DevTools Protocol (`WebDevtools` + `PageKeeper`), bundled `arm64-v8a` Alpine Linux `proot` sandbox (`rootfs.tar.xz` + `TerminalActivity`), 5-backend Hold-to-Talk ASR, `MikaInCallService` AI call assistant, `NoticeListenerService` proactive digests, 12-point `SelfCheck`, and WeChat/MCP/Relay connectors.
   - **Native Telegram <-> Mika Bridge (`TelegramMikaChannelBridge`)**: Trigger, monitor, steer, and approve Mika phone-control runs directly from `@MikaPhoneBot`, `Saved Messages`, `/mika <goal>` in any chat, the Telegram Drawer, or the Floating Capsule.

## Repository Layout

- `docs/specs/gpui-telegram-android-mika-fusion.md` — Authoritative feature & architecture specification (`No Spec, No Code`)
- `mobile/android/rust/mahayana_android_host` — Self-contained Rust JNI & C FFI runtime (`libmahayana_app_host.so`, 16KB page-aligned)
- `mobile/android/mika-core` — Pure-JVM Mika agent core, multi-provider LLM clients, OTLP trace schemas & bundled skills/knowledge
- `mobile/android/mika-agent` — Android library housing Mika Accessibility, `IMikaShell` AIDL, VirtualDisplay, Capsule, OCR, Web CDP, `proot` sandbox & Compose screens
- `mobile/android/app` — `com.ombhrum.fabushi` unified Telegram Android + Fabushi + Mika application
- `scripts/build_rust_android.sh` — Cross-compiles `libmahayana_app_host.so` for `arm64-v8a`, `x86_64`, and host JVM tests
- `scripts/package_android_release.py` — Signs, verifies 16KB ELF alignment & APK contents, and emits `fabushi-android-update.json` + `SHA256SUMS`

## Building & Testing Locally

```bash
# 1. Test and cross-compile Rust native host (arm64-v8a, x86_64, and host JVM)
cargo test --manifest-path mobile/android/rust/mahayana_android_host/Cargo.toml
bash scripts/build_rust_android.sh

# 2. Run unit tests, JNI integration tests, and assemble APKs
cd mobile/android
./gradlew :mika-core:test :mika-agent:testDebugUnitTest :app:testDebugUnitTest :app:assembleDebug :app:assembleRelease :app:assembleGithubRelease
```

## License

Apache License 2.0 (see [LICENSE](LICENSE), [NOTICE](NOTICE), and [licenses/THIRD_PARTY_NOTICES.md](licenses/THIRD_PARTY_NOTICES.md)).
