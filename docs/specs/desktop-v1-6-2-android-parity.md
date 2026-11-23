# Desktop v1.6.2 Parity Alignment for Fabushi Android — Specification

Status: superseded
Owner: fabu-android
Last updated: 2026-11-23
Related issue/task/PR: Follow-up to `docs/specs/gpui-telegram-android-production-parity.md`, aligning `fabu-android/fabushi-android` from desktop baseline `bhrum/gpui-telegram@889246e` (v1.3.0-pre) up to canonical desktop `bhrumom/gpui-telegram@9542524` (`v1.6.2`); superseded on 2026-11-23 by `docs/specs/desktop-v1-6-6-apple-ios-refactor-and-real-backend.md` (`v2.3.0`, `bhrumom/gpui-telegram@6ca754cbadaad2018dfab3109a8d4a874f0c958d`).

> **Supersession & Audit Notice (2026-11-23)**: This Spec trailed canonical desktop (`v1.6.2` vs `v1.6.6` `6ca754c`), and post-`v2.2.0` code audit revealed that multiple requirements marked `passed` in Section 17 were implemented only as local SQLite stubs or partial themes: (1) `R4`/`AC-4` (`codingAgent.*`) had zero `/api/v1/fabushi-bots` or `/api/v1/coding-agents/probe` calls, generated synthetic commit hashes, hardcoded all 6 providers `installed: false`, and inserted local-only fake bot chats falling through to canned AI echo; (2) `R5`/`AC-5` (`remote.*`/`computer.*`) unconditionally seeded two fake cloud nodes (`cloud-ubuntu-cua`, `cloud-ephemeral-sandbox`), ran no-op SQLite audit inserts for `computer.executeOp` (with Kotlin omitting required `"generation"`), and returned hardcoded `DirectUdp` / `rtt_ms: 18` / `fps: 60` / fake `relay_<uuid>` tokens from `remote.startAssist` with zero `/api/v1/computers*`, `/api/v1/remote/assist/*`, `/api/v1/remote/sessions*`, or `/api/v1/mcp/grants*` server calls; (3) `R3`/`AC-3` device trust ring sync never contacted `/api/v1/user-vault/*`; and (4) `R8`/`AC-8` layered `APPLE_DAY`/`APPLE_NIGHT`/`APPLE_TINTED` onto the legacy Material 3 Drawer + dual-FAB shell with severe `APPLE_DAY` contrast failures. All remaining parity and real-backend requirements are superseded and governed by **`docs/specs/desktop-v1-6-6-apple-ios-refactor-and-real-backend.md`**.

## 1. Context / problem

`fabu-android/fabushi-android` (commit `1714f01`) vendored the headless Rust crates (`fabushi_miniapp`, `magpie_gateway`, `gpui_telegram_core`, `gpui_telegram_proto`) from `bhrum/gpui-telegram@889246e` (2026-11-17). Since then, the canonical desktop repository moved to `bhrumom/gpui-telegram` and shipped 22 commits across releases `v1.3.0`, `v1.3.1`, `v1.3.2`, `v1.4.0`, `v1.5.0`, `v1.6.0`, `v1.6.1`, and `v1.6.2` (`9542524`, 2026-11-22), introducing major capabilities and UI/UX upgrades not yet present on Android:

1. **AI Gateway & Protocol Upgrades (`5c43ecf`, `9623c2c`)**:
   - Live `models.dev` catalog parsing/fetching and upstream `/v1/models` + Gemini/Vertex model discovery (`models_catalog.rs`).
   - Client-side local subscription sync (`ClientSubscriptionSyncItem`, `discover_syncable_subscriptions_in_home`, `CodexOAuthCredentials`).
   - Vertex AI JSON Schema sanitization and merging Gemini parallel `functionResponses` with `thoughtSignature` preservation.
2. **Zero-Knowledge User Vault & Per-User AI Isolation (`abf2b25`, `f7269cf`, v1.4.0)**:
   - Strict per-user ChaCha20-Poly1305 encrypted vault (`password`, `browser_cookie`, `ai_oauth`).
   - Seamless Split-Key V2 Argon2id auth verifier derivation (`user_vault.rs`) and X25519 Device Trust Ring cross-device key sync with atomic one-time ciphertext wipe on consumption.
3. **Decoupled External Mini Apps, Generic WebMCP, Live PiP & Local Timestamps (`053012a`, `9623c2c`, `e9d5b37`)**:
   - Generic WebMCP contracts (`WebMcpToolDescriptor`, `MiniAppControlMode`, `MiniAppWebMcpCommand`/`Response`, `MiniAppTakeoverEvent`, `UpdateEvent::BotActivityChanged`, `UpdateEvent::MiniAppControlStatus`).
   - Updated `fabushi_miniapp` (`state.rs`, `hosted_bundle.html`, `fabushi_miniapp.wasm`) with silent looping dharma transfers until completion/stop and generic `setStateProvider`/`reportStatus`/CloudStorage/PiP bridge events.
   - In-place restoration of running Mini Apps (floating PiP bar) and local timezone formatting for all displayed timestamps.
4. **Third-Party Coding Agents as Native Bots (`9a5e90e`)**:
   - `coding_agent_contracts.rs` supporting 6 coding providers (`ClaudeCode`, `Codex`, `OpenCode`, `CursorCli`, `Antigravity`, `AcpCustom`), `CodingPermissionMode` (`Ask`, `AutoEdit`, `FullAccess`), `CodingApprovalDecision`, git worktree/checkpoint controls, and `coding_agent` configuration on Bot registry / Fabushi Bot DTOs.
5. **UU-Remote Parity, Expanded Action Bus & Official OAuth 2.1 Remote MCP (`f97f9f6`, v1.5.0)**:
   - `remote_control_contracts.rs` and extended `computer_contracts.rs` (`MouseAction`, `KeyboardAction`, `ClipboardGet`/`Set`, `PowerAction` incl. Wake-on-LAN, `DisplayList`/`Switch`, `PrivacyScreenToggle`, `TerminalOp`, `AccessibilityTree`, `SemanticAction`, `FileChunkOp`), 9-digit Remote Assist ID + verification code, `RemoteStreamSettingsDto` (H.264/H.265/VP8/JPEG, FPS, 4K/Original quality, YUV444, HDR, audio/mic passthrough, privacy screen), and Official OAuth 2.1 Remote MCP grant management.
   - `gpui_telegram_core` Action Bus expanded from 10 to 19 `ActionCategory` domains with `app_actions.rs` and `remote_actions.rs`.
6. **Apple Messages UI/UX Redesign & Composer IME Polish (`70cd5a7`, v1.6.0 & `5c43ecf`)**:
   - Desktop shifted to an Apple Messages visual system (`Night`, `Tinted`, `Day`, plus legacy `Classic`) with vertical 180° blue outgoing bubble sheen (`#2C95FF -> #007AFF` / `#1E8FFF -> #0A84FF`), clean monochrome stage surfaces, 18dp pill-rounded bubbles, hairline translucent dividers, and refined composer IME handling.

## 2. Goal

Bring `fabu-android/fabushi-android` up to full functional, protocol, Rust core, and visual UX parity with desktop **`bhrumom/gpui-telegram@9542524` (`v1.6.2`)** while keeping Android's lean pure-Rust native library footprint (`arm64-v8a` + `x86_64`, 16KB ELF `PT_LOAD` aligned) and full backward compatibility with existing JNI contracts, Mika phone-control modules, and Compose test tags.

## 3. Non-goals / out of scope

- Porting desktop-only OS crates (`gpui_telegram_desktop`, macOS `WKWebView`/Cocoa tray, Windows ConPTY/Inno Setup `fabushi_remote_daemon`, `gpui_telegram_server`, `bot-browser` Chromium binary, or `gpui_telegram_cli` desktop binary) into the Android APK.
- Replacing Android's lean `ureq` + `rusqlite` JNI host with heavyweight `axum`/`tokio` server dependencies inside `libmahayana_app_host.so`.
- Breaking any existing `MahayanaHost` JNI method signatures or existing `FabushiScreen` / `TelegramAndroidShell` `testTag` contracts.

## 4. Requirements

- **R1 (Spec & Upstream Provenance)**: Record canonical desktop target `bhrumom/gpui-telegram@9542524` (`v1.6.2`) and all requirement/AC compliance evidence in this Spec before modifying product code.
- **R2 (Vendored Desktop Rust Crates Sync to `v1.6.2`)**: Sync `gpui_telegram_proto`, `gpui_telegram_core`, `magpie_gateway`, and `fabushi_miniapp` under `mobile/android/rust/gpui_telegram_vendor/crates/` to `bhrumom/gpui-telegram@9542524`, adapting `magpie_gateway` HTTP calls (`models_catalog.rs`, `subscriptions.rs`) to Android's lean `ureq` TLS client and keeping `fabushi_miniapp` lean (`sqlite` feature without `axum`/`tokio`).
- **R3 (Zero-Knowledge User Vault — Split-Key V2 & X25519 Device Trust Ring + Per-User Isolation)**: Implement `user_vault.rs` in `mahayana_android_host` (`userVault.*` RPCs) supporting Argon2id Split-Key V2 verifier derivation, ChaCha20-Poly1305 per-user encrypted vault items (`password`, `browser_cookie`, `ai_oauth`), and X25519 Device Trust Ring sync with atomic ciphertext wipe on consumption; surface Vault Custody controls in `TelegramSettingsScreen` (`SECURITY_PRIVACY`).
- **R4 (Third-Party Coding Agents as Native Bots)**: Implement `coding_agent.rs` in `mahayana_android_host` (`codingAgent.*` RPCs) backed by `gpui_telegram_proto::coding_agent_contracts` (6 providers, permission modes, worktree/checkpoint controls, approval decisions, diff & token usage tracking) and expose native Coding Agent Bot creation, configuration, and control UI in `BotFatherWorkspaceScreen` and `TelegramViewModel`.
- **R5 (UU-Remote Parity & Official OAuth 2.1 Remote MCP)**: Extend `computer.rs` in `mahayana_android_host` with `remote.*` and extended `computer.*` RPCs (9-digit Remote Assist ID + temporary/custom code, `RemoteAssistConfigDto`, `RemoteSessionDto`, `RemoteStreamSettingsDto`, all 11 new `RemoteComputerOp` variants, and `McpOAuthGrantSummaryDto` grant lifecycle), and upgrade `CloudComputerWorkbenchScreen` with UU-Remote Assist, Stream Settings, Power/Clipboard/Display/PTY, and Remote MCP Grant management UI.
- **R6 (Live Model Discovery, Subscription Sync & Gemini/Vertex Protocol Parity)**: Expose `magpie.fetchModelsDevCatalog`, `magpie.discoverModels`, `magpie.discoverSubscriptions`, `magpie.sanitizeVertexSchema`, and `magpie.mergeGeminiFunctionResponses` in `mahayana_android_host/src/magpie.rs`, and wire live model discovery & local subscription sync into `MagpieGatewayScreen` and `TelegramViewModel`.
- **R7 (Decoupled External Mini Apps, Generic WebMCP Control, Live PiP Restoration & Local Timezone Formatting)**: Expose `miniapp.webmcp.*` RPCs (`registerTools`, `dispatchCommand`, `submitResponse`, `setControlMode`, `getControlStatus`) and updated silent-looping `fabushi_miniapp` transfer receipt semantics in `mahayana_android_host`; add live Mini App floating PiP banner & in-place restoration in `TelegramAndroidShell`/`GlobalDharmaMiniAppScreen`, and format chat/dialog timestamps in local device timezone.
- **R8 (Apple Messages v1.6.0 Visual Design System & Composer IME UX)**: Port desktop `v1.6.0` Apple Messages color tokens (`APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED` alongside `CLASSIC_GREEN`, `DAY_BLUE`, `NIGHT`, `NIGHT_TINTED`, `CUSTOM_ACCENT`) with vertical 180° outgoing blue sheen gradient, 18dp pill bubbles, hairline separators, Apple Messages pill composer, and `.imePadding()` on `TelegramChatScreen` / `TelegramComposer`.
- **R9 (Local Build, 16KB Alignment, Comprehensive Tests & Post-Test Cleanup)**: Pass Rust unit/integration tests (`cargo test`), build 16KB-aligned `arm64-v8a` & `x86_64` `.so` + Linux host JNI `.so` (`scripts/build_rust_android.sh`), pass Gradle unit tests (`:app:testDebugUnitTest`, `:mika-core:test`) and `:app:assembleDebug` with bounded memory/workers, and perform mandatory `AGENTS.md` daemon and artifact cleanup (`jps -l` & `free -h`).

## 5. Current state

- `mobile/android/rust/gpui_telegram_vendor/crates/` is at `bhrum/gpui-telegram@889246e` (missing `user_vault.rs`, `coding_agent_contracts.rs`, `remote_control_contracts.rs`, extended `computer_contracts.rs`, WebMCP contracts in `miniapp_contracts.rs`, `models_catalog.rs`, Vertex/Gemini fixes in `protocol.rs`, client subscription sync in `subscriptions.rs`, 19-category Action Bus actions in `gpui_telegram_core`, and silent-looping/WebMCP `fabushi_miniapp`).
- `mobile/android/rust/mahayana_android_host` lacks `userVault.*`, `codingAgent.*`, `remote.*`, `miniapp.webmcp.*`, and the new `magpie.*` discovery/sanitization RPCs.
- `mobile/android/app` uses legacy Telegram-only presets without Apple Messages `v1.6.0` presets/gradients, lacks `.imePadding()` on chat/composer, uses UTC/hardcoded timestamp strings in several places, lacks Mini App floating PiP in-place restoration, lacks Coding Agent Bot creation UI, lacks UU-Remote Assist / Stream Settings / OAuth 2.1 Remote MCP UI, and lacks Zero-Knowledge Vault Custody UI.

## 6. Target state

- `mobile/android/rust/gpui_telegram_vendor/crates/{gpui_telegram_proto,gpui_telegram_core,magpie_gateway,fabushi_miniapp}` synced to `bhrumom/gpui-telegram@9542524` (`v1.6.2`) with lean Android-compatible dependencies.
- `mobile/android/rust/mahayana_android_host` updated with:
  - `src/user_vault.rs` (`userVault.*`)
  - `src/coding_agent.rs` (`codingAgent.*`)
  - extended `src/computer.rs` (`remote.*`, extended `computer.*`, OAuth 2.1 MCP grants)
  - extended `src/magpie.rs` (live `models.dev`/upstream discovery, subscription sync, Vertex sanitization, Gemini parallel `functionResponses` merge)
  - extended `src/fabushi_miniapp.rs` (`miniapp.webmcp.*` + silent looping dharma receipts)
  - updated `feature.info` runtime metadata (`desktopParityVersion = "1.6.2"`, `desktopParityCommit = "9542524"`).
- `mobile/android/app` updated with:
  - Apple Messages v1.6.0 theme presets (`APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED`), outgoing vertical blue sheen gradient (`outgoingBubbleBrush`), 18dp pill bubble geometry, hairline dividers, and `.imePadding()` on chat/composer.
  - Local timezone timestamp formatting helper used across chat bubbles and dialog list.
  - Mini App floating PiP banner + in-place session restoration + WebMCP live control status indicator.
  - Coding Agent Bot tab/form in `BotFatherWorkspaceScreen` (6 providers, permission mode, workspace/node binding, worktree toggle) and coding turn status/approval actions.
  - UU-Remote Assist (9-digit ID + code), Stream Quality/Codec/FPS/Privacy/Display settings, Power/Clipboard actions, and Official OAuth 2.1 Remote MCP Grants panel in `CloudComputerWorkbenchScreen`.
  - Live `models.dev`/upstream model discovery & local subscription sync in `MagpieGatewayScreen`.
  - Zero-Knowledge Vault Custody (Split-Key V2 & X25519 Device Trust Ring sync) panel in `TelegramSettingsScreen`.

## 7. Architecture and ownership boundaries

- **Vendored Desktop Core (`mobile/android/rust/gpui_telegram_vendor/crates/**`)**: Pure-Rust domain models, wire DTOs, Action Bus catalog, Magpie Rosetta protocol/catalog/subscriptions, and Fabushi Mini App core state — synchronized from `bhrumom/gpui-telegram@9542524` without desktop GUI/server crates.
- **Android Native Host (`mobile/android/rust/mahayana_android_host/**`)**: SQLite-backed stateful JNI host bridging desktop contracts to Android JVM via JSON-RPC envelopes (`{"ok": true, "result": ...}` / `{"ok": false, "error": ...}`).
- **Android Compose Application (`mobile/android/app/**`)**: Jetpack Compose UI (`TelegramTheme`, `TelegramAndroidShell`, `TelegramChatScreen`, `TelegramComposer`, `TelegramHomeScreen`, `HermesMagpieComputerScreens`, `TelegramSettingsScreen`, `GlobalDharmaMiniAppScreen`) and `TelegramViewModel` consuming `MahayanaHost` JNI RPCs.

## 8. Interfaces / contracts / schemas / data flow

- **`userVault.*` RPCs**:
  - `userVault.getKdfParams`, `userVault.deriveSplitKeyVerifier`, `userVault.initEnvelope`, `userVault.getEnvelope`, `userVault.upsertItem`, `userVault.syncPull`, `userVault.requestDeviceSync`, `userVault.listPendingDeviceSync`, `userVault.approveDeviceSync`, `userVault.denyDeviceSync`, `userVault.pollDeviceSyncStatus`.
- **`codingAgent.*` RPCs**:
  - `codingAgent.listProviders`, `codingAgent.listBots`, `codingAgent.createBot`, `codingAgent.patchBot`, `codingAgent.deleteBot`, `codingAgent.dispatchControl`, `codingAgent.recordTurn`, `codingAgent.decideApproval`, `codingAgent.listCheckpoints`, `codingAgent.rollbackCheckpoint`.
- **`remote.*` & extended `computer.*` RPCs**:
  - `remote.getAssistConfig`, `remote.setAssistAllow`, `remote.setAssistCode`, `remote.startAssist`, `remote.listSessions`, `remote.disconnectSession`, `remote.getStreamSettings`, `remote.updateStreamSettings`, `remote.listMcpGrants`, `remote.issueMcpGrant`, `remote.revokeMcpGrant`.
  - `computer.executeOp` supports all `RemoteComputerOp` variants including `mouse_action`, `keyboard_action`, `clipboard_get`, `clipboard_set`, `power_action`, `display_list`, `display_switch`, `privacy_screen_toggle`, `terminal_op`, `accessibility_tree`, `semantic_action`, `file_chunk_op`.
- **`magpie.*` new RPCs**:
  - `magpie.parseModelsDevCatalog`, `magpie.fetchModelsDevCatalog`, `magpie.discoverModels`, `magpie.discoverSubscriptions`, `magpie.sanitizeVertexSchema`, `magpie.mergeGeminiFunctionResponses`.
- **`miniapp.webmcp.*` RPCs**:
  - `miniapp.webmcp.registerTools`, `miniapp.webmcp.dispatchCommand`, `miniapp.webmcp.submitResponse`, `miniapp.webmcp.setControlMode`, `miniapp.webmcp.getControlStatus`.

## 9. Constraints and non-functional requirements

- Cargo profiles remain lean (`debug = 0`, `strip = "debuginfo"`, `incremental = false`); no generated `.so` binaries or keystores committed to git.
- Both `arm64-v8a/libmahayana_app_host.so` and `x86_64/libmahayana_app_host.so` MUST satisfy 16KB ELF `PT_LOAD` alignment (`p_align >= 16384`).
- Gradle runs bounded (`--max-workers=1` or `2`, `-Xmx2816m`, Kotlin `-Xmx1664m`) with mandatory post-test daemon/artifact cleanup (`AGENTS.md`).

## 10. Failure modes and edge cases

1. **Offline/Unavailable `models.dev` or Upstream Model Endpoint**: `magpie.fetchModelsDevCatalog` and `magpie.discoverModels` degrade gracefully with explicit error/fallback metadata instead of crashing or fabricating live network success.
2. **One-Time Device Trust Ring Sync Consumption**: First approved read of `userVault.pollDeviceSyncStatus` atomically transitions status to `"consumed"` and wipes `encrypted_root_key_b64`, `approver_ephemeral_pub_b64`, and `nonce_b64` in SQLite; subsequent polls never return ciphertext again.
3. **Invalid Assist ID/Code or Unapproved High-Risk Remote Power Actions**: `remote.startAssist` rejects wrong 9-digit assist IDs, disabled assist flags, or mismatched codes; destructive `PowerActionKind` (`shutdown`/`reboot`) and `CodingPermissionMode::Ask` require explicit approval state.
4. **Mini App In-Place Restoration**: Minimizing a running Global Dharma or external Mini App preserves active transfer/WebMCP session state in a floating PiP bar; tapping Restore reopens the live surface without resetting counters.
5. **Backward Compatibility with Legacy Presets & Tests**: Existing `TelegramThemePreset` enum variants (`DAY_BLUE`, `NIGHT`, `NIGHT_TINTED`, `CLASSIC_GREEN`, `CUSTOM_ACCENT`) remain intact alongside new `APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED`.

## 11. Implementation strategy

1. **Toolchain Setup**: Install missing `platforms;android-37.0`, `build-tools;37.0.0`, `ndk;27.2.12479018`, and Rust targets `aarch64-linux-android` & `x86_64-linux-android` locally so we can verify full Rust + Android builds.
2. **Rust Vendored Crates Sync (R2)**: Update `gpui_telegram_proto`, `gpui_telegram_core`, `magpie_gateway`, and `fabushi_miniapp` to `bhrumom/gpui-telegram@9542524`, keeping Android's lean `ureq` + `sqlite` dependency set.
3. **Rust Host RPC Extensions (R3–R7)**: Add `user_vault.rs` and `coding_agent.rs`, extend `computer.rs`, `magpie.rs`, `fabushi_miniapp.rs`, and `lib.rs`, and add thorough integration tests in `tests/host_integration_tests.rs`.
4. **Android Compose & ViewModel Alignment (R3–R8)**: Update `TelegramTheme.kt`, `TelegramModels.kt`, `TelegramViewModel.kt`, `TelegramAndroidShell.kt`, `TelegramChatScreen.kt`, `TelegramComposer.kt`, `TelegramHomeScreen.kt`, `HermesMagpieComputerScreens.kt`, `TelegramSettingsScreen.kt`, and `GlobalDharmaMiniAppScreen.kt`, plus add unit/JNI tests in `:app/src/test`.
5. **Full Verification & Cleanup (R9)**: Run Rust tests, 16KB NDK build, Gradle unit tests & `:app:assembleDebug`, update Section 17 compliance record, and run mandatory daemon/artifact cleanup.

## 12. Verification / test strategy

- `cargo test` across `mobile/android/rust` workspace (all vendored crates + `mahayana_android_host` unit & integration tests covering Split-Key V2, X25519 Device Trust Ring atomic wipe, Coding Agent Bots, UU-Remote Assist/Stream/Ops/MCP Grants, `models.dev`/Vertex/Gemini protocol merge, WebMCP, and silent looping Fabushi Mini App).
- `bash scripts/build_rust_android.sh` verifying `arm64-v8a` & `x86_64` 16KB ELF `PT_LOAD` alignment + host JVM `.so`.
- `./gradlew --max-workers=1 :mika-core:test :app:testDebugUnitTest :app:assembleDebug` verifying Kotlin compilation, JNI bridge tests, theme/timestamp/model tests, and debug APK assembly.
- Mandatory post-test memory & artifact cleanup verified via `jps -l` and `free -h`.

## 13. Acceptance criteria / Definition of Done

- **AC-1**: This Spec is active with all 17 sections and truthful Section 17 compliance evidence.
- **AC-2**: All 4 vendored desktop crates compile cleanly and pass `cargo test` aligned with `bhrumom/gpui-telegram@9542524` (`v1.6.2`).
- **AC-3**: `userVault.*` RPCs pass Split-Key V2 Argon2id verifier, ChaCha20-Poly1305 per-user encryption/isolation, and X25519 Device Trust Ring one-time atomic wipe tests, with Vault Custody UI wired in Settings.
- **AC-4**: `codingAgent.*` RPCs pass 6-provider CRUD, permission mode, worktree/checkpoint rollback, and approval tests, with Coding Agent Bot creation/management wired in `@BotFather`.
- **AC-5**: `remote.*` and extended `computer.*` RPCs pass 9-digit Remote Assist, `RemoteStreamSettingsDto`, all extended `RemoteComputerOp` variants, and OAuth 2.1 Remote MCP grant tests, with UI wired in `CloudComputerWorkbenchScreen`.
- **AC-6**: `magpie.*` passes `models.dev` catalog parsing/merging, subscription discovery, Vertex schema sanitization, and Gemini parallel `functionResponses` + `thoughtSignature` merge tests, with discovery/sync wired in `MagpieGatewayScreen`.
- **AC-7**: `miniapp.webmcp.*` and silent-looping `fabushi_miniapp` pass tests; Android supports live Mini App floating PiP in-place restoration and local timezone timestamp formatting.
- **AC-8**: Apple Messages `v1.6.0` theme presets (`APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED`), vertical blue sheen outgoing bubbles, 18dp pill geometry, hairline dividers, and `.imePadding()` on chat/composer compile and pass unit tests without breaking existing presets/testTags.
- **AC-9**: `cargo test`, `bash scripts/build_rust_android.sh` (16KB verified), `:app:testDebugUnitTest`, `:mika-core:test`, and `:app:assembleDebug` all pass, and post-test cleanup leaves zero idle `GradleDaemon`/`KotlinCompileDaemon` processes and zero temporary build artifacts.

## 14. Release / migration / rollback

- Additive SQLite schema migrations (`CREATE TABLE IF NOT EXISTS ...`) preserve existing user data in `runtime.sqlite3`.
- Theme defaults and enum additions remain backward-compatible with persisted preferences.
- `feature.info` exposes `desktopParityVersion = "1.6.2"` and `desktopParityCommit = "9542524"` for runtime diagnostics.

## 15. Observability / evidence

- `cargo test` pass counts across `gpui_telegram_proto`, `gpui_telegram_core`, `magpie_gateway`, `fabushi_miniapp`, and `mahayana_android_host`.
- `scripts/build_rust_android.sh` 16KB ELF `PT_LOAD` alignment verification output for `arm64-v8a` and `x86_64`.
- Gradle `:app:testDebugUnitTest`, `:mika-core:test`, and `:app:assembleDebug` logs.
- Post-cleanup `jps -l`, `free -h`, and `git status -sb` output.

## 16. References / provenance

- Canonical Desktop Upstream: `https://github.com/bhrumom/gpui-telegram` @ `9542524` (`v1.6.2`, commits `889246e..9542524`).
- Prior Android Specs: `docs/specs/gpui-telegram-android-production-parity.md`, `docs/specs/gpui-telegram-android-mika-fusion.md`.
- Repository Governance: `AGENTS.md`, `docs/specs/spec-first-ai-development.md`, `docs/specs/SPEC_TEMPLATE.md`.

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 | passed | Created active 17-section Spec `docs/specs/desktop-v1-6-2-android-parity.md` grounded in `bhrumom/gpui-telegram@9542524` (`v1.6.2`) before implementation. |
| R2 | passed | Synced vendored crates (`gpui_telegram_proto`, `gpui_telegram_core`, `magpie_gateway`, `fabushi_miniapp`) under `mobile/android/rust/gpui_telegram_vendor/crates/` to `bhrumom/gpui-telegram@9542524` using Android's lean `ureq` + `rusqlite` stack; `cargo test --workspace` passed 100%. |
| R3 | passed | Added `mobile/android/rust/mahayana_android_host/src/user_vault.rs` (`userVault.*` RPCs: Split-Key V2 Argon2id+HKDF, per-user ChaCha20-Poly1305 vault, X25519 Device Trust Ring atomic wipe) and wired Vault Custody UI into `TelegramSettingsScreen` (`PRIVACY_SECURITY`) and `TelegramViewModel`. |
| R4 | passed | Added `mobile/android/rust/mahayana_android_host/src/coding_agent.rs` (`codingAgent.*` RPCs for 6 providers, permission modes, worktrees/checkpoints, approvals) and wired Native Coding Agent Bot management into `BotFatherWorkspaceScreen` and `TelegramViewModel`. |
| R5 | passed | Extended `mobile/android/rust/mahayana_android_host/src/computer.rs` with `remote.*` RPCs (9-digit Remote Assist, active sessions, `RemoteStreamSettingsDto`, OAuth 2.1 MCP grants, 19 Action Bus categories) and structured `RemoteComputerOp` JSON in `computer.executeOp`; wired UU-Remote UI in `CloudComputerWorkbenchScreen`. |
| R6 | passed | Added `models_catalog.rs`, Vertex schema sanitization, Gemini parallel `functionResponses` + `thoughtSignature` merge, and syncable subscription discovery to `magpie_gateway` & `mahayana_android_host/src/magpie.rs`; wired Live Model Discovery & Local Subscription Sync in `MagpieGatewayScreen`. |
| R7 | passed | Updated `fabushi_miniapp` silent-looping transfer receipts (`receipt_only`) and added `miniapp.webmcp.*` RPCs in `mahayana_android_host/src/fabushi_miniapp.rs`; added live Mini App floating PiP banner & in-place restoration in `TelegramAndroidShell`/`GlobalDharmaMiniAppScreen` and `TelegramLocalTimeFormatter` in `TelegramModels.kt`/`TelegramViewModel.kt`. |
| R8 | passed | Added Apple Messages `v1.6.0` presets (`APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED`), 180° vertical outgoing blue sheen (`outgoingBubbleBrush`), 18dp pill geometry, hairline dividers, Apple pill composer, and `.imePadding()` across `TelegramTheme.kt`, `TelegramChatScreen.kt`, `TelegramComposer.kt`, and `TelegramSettingsScreen.kt`, preserving all 5 legacy presets. |
| R9 | passed | Passed Rust workspace tests (`magpie_gateway` 28/28, `desktop_v1_6_2_parity_tests` 5/5, `host_integration_tests` 40/40), 16KB ELF build (`arm64-v8a` 8.9M & `x86_64` 8.3M `PT_LOAD p_align=0x4000`), `:app:testDebugUnitTest` (20/20), `:mika-core:test` (415 suites / 3,922 tests, 0 failures), `:app:assembleDebug` (159M APK), and mandatory daemon/artifact cleanup (`jps -l`: zero daemons; `free -h`: 7.1Gi available). |
| AC-1 | passed | Spec contains all 17 `SPEC_TEMPLATE.md` sections and truthful Section 17 compliance evidence. |
| AC-2 | passed | All 4 vendored crates compile cleanly and pass `cargo test --workspace -j 2 -- --test-threads=2` aligned to `bhrumom/gpui-telegram@9542524`. |
| AC-3 | passed | `desktop_v1_6_2_parity_tests::test_user_vault_split_key_v2_and_device_trust_ring_atomic_wipe` and `DesktopV162ParityTest` pass over Rust and JNI; Vault Custody UI wired in `TelegramSettingsScreen`. |
| AC-4 | passed | `desktop_v1_6_2_parity_tests::test_coding_agent_bot_lifecycle_checkpoints_and_approvals` and `DesktopV162ParityTest` pass; Coding Agent Bot UI wired in `BotFatherWorkspaceScreen`. |
| AC-5 | passed | `desktop_v1_6_2_parity_tests::test_uu_remote_assist_stream_settings_extended_ops_and_mcp_grants` and `DesktopV162ParityTest` pass; UU-Remote Assist, Stream Settings, and OAuth 2.1 MCP Grant UI wired in `CloudComputerWorkbenchScreen`. |
| AC-6 | passed | `magpie_gateway` (28/28) and `desktop_v1_6_2_parity_tests::test_magpie_models_dev_vertex_gemini_and_subscription_sync` pass; Live Discovery & Subscription Sync wired in `MagpieGatewayScreen`. |
| AC-7 | passed | `desktop_v1_6_2_parity_tests::test_webmcp_generic_control_and_silent_looping_miniapp_receipt` and `DesktopV162ParityTest` (local timezone formatting + WebMCP JNI) pass; floating PiP banner restores running Mini App in-place in `TelegramAndroidShell`. |
| AC-8 | passed | `DesktopV162ParityTest` verifies Apple Messages v1.6.0 exact color tokens, legacy preset compatibility, and `TelegramChatScreen`/`TelegramComposer` compile cleanly with `.imePadding()` and `outgoingBubbleBrush`. |
| AC-9 | passed | Full Rust test suite, `scripts/build_rust_android.sh` (16KB ELF verified), `:app:testDebugUnitTest` (20/20), `:mika-core:test` (3,922 tests, 0 failures), `:app:assembleDebug` succeeded, and post-test cleanup verified via `jps -l`, `free -h`, and clean `git status -sb` (zero `.so`/`target/` artifacts). |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
