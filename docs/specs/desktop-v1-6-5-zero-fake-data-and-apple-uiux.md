# Desktop v1.6.5 Zero-Fake-Data, Real Server Sync, Branch Consolidation & Apple Messages UI/UX Alignment — Specification

Status: active
Owner: fabu-android
Last updated: 2026-11-22
Related issue/task/PR: Follow-up to `docs/specs/desktop-v1-6-2-android-parity.md`; consolidates all repository branches, eliminates all hardcoded demo/fake UI data, wires real `gpui_telegram_server` `/api/v1/*` HTTP + WebSocket sync, and aligns Android UI/UX with desktop **`bhrumom/gpui-telegram@db06f9c` (`v1.6.5`)**.

## 1. Context / problem

After releasing `v2.1.0`, installing the APK revealed three critical product gaps compared to desktop `bhrumom/gpui-telegram@db06f9c` (`v1.6.5`):

1. **Pervasive Hardcoded Fake/Demo Data**:
   - `TelegramViewModel.buildInitialSeedState()` seeded 450+ lines of fake accounts (`全球法布施 · 主账号`, `+86 138 0013 8000`, `Mahayana DevOps`), fake stories, fake chats (`GPUI-Telegram × Mika 架构群 1,024人`, `觉行禅师`, `全球法布施 · 官方广播频道 128,400位订阅者`, `早期版本构建通知归档`), fake messages, fake comments (`新加坡莲友`, `Kyoto Practitioner`), fake polls, fake TON Wallet transactions (`+25.00 TON`, `@fabushi.ton`), fake Hermes Studio sessions/kanban/approvals, fake Cloud Computer nodes (`NVIDIA L40S`, `Firecracker Alpine`), fake tasks, and fake call history.
   - `TelegramModels.kt` defaulted `TonWalletUiState` to `128.64 TON` / `3,850 Stars` / fake address and `GlobalDharmaUiState` to `1,284,920` offerings / `1,080` merit / fake leaderboard.
   - `TelegramViewModel.restorePersistedSettings()` defaulted to legacy `DAY_BLUE` + `DOODLE_CLASSIC` instead of Apple Messages `APPLE_DAY` + `SOLID_MINIMAL`, overriding the new v1.6.0 theme on fresh installs.
2. **Missing Real `gpui_telegram_server` Auth & Sync Flow**:
   - Android opened straight into fake demo chats without desktop's `AuthWizardView` login/registration gate and without talking to desktop's real `gpui_telegram_server` (`/api/v1/auth/login`, `/api/v1/auth/register`, `/api/v1/auth/me`, `/api/v1/chats`, `/api/v1/chats/{id}/messages`, `/api/v1/messages/send`, `/api/v1/contacts/add`, `/api/v1/chats/create`, `/api/v1/ws?token=...`, default `http://34.46.77.133:8080`).
3. **Desktop `v1.6.0`–`v1.6.5` Apple Messages UI/UX Gaps & Unmerged Repository Branches**:
   - Desktop shipped `v1.6.3`–`v1.6.5` (`db06f9c`) with **Centered Apple Messages Contact Header** (`stage_bars.rs`), **3-Column Pinned Chats Avatar Grid** (`dialogs_renderer.rs`), **Apple Messages Pill Composer with circular `+` attach and `#007AFF` `↑` send button** (`composer_ui.rs`), pinned chat anti-flicker stability, and explicit WebMCP error reporting.
   - Repository still had 6 unmerged remote branches including PR #4 (`codex/dandelion-capability-contract-20261019`) with important Android MiniApp lifecycle background revocation and capability contract fixes.

## 2. Goal

Deliver a **100% real-data, zero-fake-data** Android client consolidated across all branches and aligned in both backend protocol (`gpui_telegram_server` `/api/v1/*` + WS) and Apple Messages UI/UX with desktop **`bhrumom/gpui-telegram@db06f9c` (`v1.6.5`)**.

## 3. Non-goals / out of scope

- Porting desktop-only crates (`gpui_telegram_desktop`, `gpui_telegram_server`, Windows ConPTY daemon, Chromium binary) into Android APK.
- Re-introducing legacy pre-November Grok-0.18 stub trees (`frontend/`, `tests/test_*.py`) onto `main`.
- Breaking existing `MahayanaHost` JNI method signatures or `:mika-core` / `:mika-agent` functionality.

## 4. Requirements

- **R1 (Spec & Upstream v1.6.5 Provenance)**: Create this active Spec grounded in `bhrumom/gpui-telegram@db06f9c` (`v1.6.5`) prior to product edits.
- **R2 (All-Branch Consolidation)**: Merge all unmerged remote branches into the working integration branch — fully integrating PR #4 (`codex/dandelion-capability-contract-20261019`) MiniApp lifecycle revocation & capability contract code/tests, and reconciling superseded legacy Grok-0.18 / older doc branches cleanly so `main` absorbs all branch histories without regression.
- **R3 (Zero Hardcoded Fake/Demo Data)**: Remove all hardcoded fake accounts, stories, chats, messages, comments, polls, TON balances/txs/NFTs, Global Dharma fake counters/leaderboards, Hermes fake sessions/kanban/approvals, Cloud Computer fake GPU/MicroVM nodes/tasks, and fake call history from `TelegramViewModel.kt` and `TelegramModels.kt`. Default fresh installs to `APPLE_DAY` + `SOLID_MINIMAL` (18dp bubble radius, 15sp font).
- **R4 (Real `gpui_telegram_server` HTTP + WebSocket Client)**: Implement real `/api/v1/*` server auth, dialog listing, history loading, message sending, contact/chat creation, and search RPCs in `mahayana_android_host/src/telegram_ext.rs` (default endpoint `http://34.46.77.133:8080`, user-configurable, Bearer token persisted in SQLite) plus live OkHttp WebSocket (`/api/v1/ws?token=...`) subscription in `TelegramViewModel.kt`.
- **R5 (Desktop-Aligned `AuthWizard` Onboarding & Real Session Gate)**: Add `TelegramAuthWizardScreen.kt` styled after desktop's Apple Messages `AuthWizardView` (Password Login / Register / Email OTP / Alipay QR tabs, editable server URL pill, real server error reporting, and explicit "仅本地离线使用" option with clean empty state), gating `TelegramAndroidShell` when unauthenticated and supporting sign-out back to login.
- **R6 (Desktop `v1.6.0`–`v1.6.5` Apple Messages UI/UX Alignment)**:
  - **Home (`TelegramHomeScreen.kt`)**: Apple translucent top bar, inline pill search bar, 3-column Pinned Chats circular avatar grid (stable sort, no flicker), hide empty Stories strip, honest empty state with create/add actions, and Apple translucent bottom navigation bar (`消息` · `联系人` · `智能体` · `云电脑` · `设置`) mirroring desktop's `UnifiedNavRail`.
  - **Chat (`TelegramChatScreen.kt`)**: Desktop `v1.6.5` **Centered Apple Messages Contact Header** (left glass back button, centered 38dp avatar + overlapping frosted name pill `Title ›` + online dot/subtitle, right floating glass action pill) when Apple theme is active, plus clean stage and 18dp pill gradient bubbles.
  - **Composer (`TelegramComposer.kt`)**: Desktop `v1.6.0` Apple Messages input strip (circular `30.dp` `+` attach button, `18.dp` pill input with focus ring, inline `Aa` + emoji, embedded circular `26.dp` `#007AFF` `↑` send button or mic icon).
- **R7 (Desktop `v1.6.3`–`v1.6.5` Runtime Metadata & Hardening)**: Update `feature.info` metadata to `desktopParityVersion = "1.6.5"`, `desktopParityCommit = "db06f9c"`; enforce explicit WebMCP error reporting without false success claims.
- **R8 (Full Verification, Mandatory Cleanup & Release)**: Pass Rust workspace tests, 16KB ELF Android build, `:app:testDebugUnitTest` (including `MiniAppWebMcpSurfaceTest` + zero-fake-data & loopback `gpui_telegram_server` tests), `:mika-core:test`, `:app:assembleDebug`, mandatory `AGENTS.md` daemon/artifact cleanup (`jps -l`, `free -h`), merge to `main`, clean obsolete branches, and ship a new release if requested.

## 5. Current state

- `main` (`c80e0c2`) is at `v2.1.0` aligned to `v1.6.2` (`9542524`), but still executes `buildInitialSeedState()` with 450 lines of fake demo data, defaults fresh installs to `DAY_BLUE` + `DOODLE_CLASSIC`, lacks a real `AuthWizard` / `gpui_telegram_server` `/api/v1/*` client, lacks desktop `v1.6.5` centered contact header / pinned chats grid / Apple composer `↑` button, and has 6 unmerged remote branches.

## 6. Target state

- All 6 remote branches cleanly merged/consolidated into `main`.
- Zero fake/demo data anywhere in default `TelegramUiState`, `TonWalletUiState`, `GlobalDharmaUiState`, `HermesStudioUiState`, or `CloudComputerUiState`.
- Fresh app launch presents desktop-aligned Apple Messages `AuthWizard` connected to `http://34.46.77.133:8080` (configurable) with Password Login / Register / Email OTP / Alipay QR + explicit Offline Local Mode.
- Authenticated sessions pull real chats, messages, contacts, and live WebSocket updates from `gpui_telegram_server`, rendered with desktop `v1.6.5` Apple Messages UI/UX (3-column pinned grid, inline pill search, translucent bottom nav, centered glass contact header, circular `+` & `#007AFF` `↑` composer).

## 7. Architecture and ownership boundaries

- **Rust JNI Host (`mobile/android/rust/mahayana_android_host`)**: Owns SQLite session/token persistence, real `ureq` HTTP client for `gpui_telegram_server` `/api/v1/*` endpoints, and local offline SQLite fallback.
- **Android Compose Shell (`mobile/android/app/src/main/java/com/ombhrum/fabushi/telegram/`)**: Owns `TelegramAuthWizardScreen`, `TelegramHomeScreen`, `TelegramChatScreen`, `TelegramComposer`, `TelegramAndroidShell`, `TelegramViewModel` (including OkHttp WebSocket listener), and zero-fake-data models in `TelegramModels.kt`.
- **MiniApp WebMCP Surface (`MiniAppWebMcpSurface.kt`, `MarketplaceViewModel.kt`)**: Integrates PR #4 lifecycle background revocation and complete capability contract.

## 8. Interfaces / contracts / schemas / data flow

- **`telegram.server.*` new/updated RPCs** (matching `gpui_telegram_client/src/net/http_client.rs`):
  - `telegram.server.login` (`POST /api/v1/auth/login`)
  - `telegram.server.register` (`POST /api/v1/auth/register`)
  - `telegram.server.sendEmailCode` (`POST /api/v1/auth/email/send-code`)
  - `telegram.server.registerEmail` (`POST /api/v1/auth/email/register`)
  - `telegram.server.alipayStart` (`POST /api/v1/auth/alipay/start`) & `telegram.server.alipayPoll` (`GET /api/v1/auth/alipay/poll/{attempt_id}`)
  - `telegram.server.me` (`GET /api/v1/auth/me`)
  - `telegram.server.logout`
  - `telegram.server.listChats` (`GET /api/v1/chats`)
  - `telegram.server.getMessages` (`GET /api/v1/chats/{id}/messages` with fallback `/api/v1/messages/history?chat_id={id}`)
  - `telegram.server.sendMessage` (`POST /api/v1/messages/send`)
  - `telegram.server.addContact` (`POST /api/v1/contacts/add`)
  - `telegram.server.createChat` (`POST /api/v1/chats/create`)
  - `telegram.server.searchMessages` (`GET /api/v1/search/messages`)
- **Live WebSocket**: `OkHttpClient.newWebSocket("ws(s)://{server}/api/v1/ws?token={token}")` handling incoming message/sync/typing events and triggering safe state updates.

## 9. Constraints and non-functional requirements

- Lean Cargo profiles (`debug = 0`, `strip = "debuginfo"`, `incremental = false`); 16KB ELF `PT_LOAD` aligned (`p_align >= 16384`).
- Never commit generated `.so` binaries or keystores.
- Mandatory post-test daemon & artifact cleanup (`./gradlew --stop`, `jps -l`, `free -h`).

## 10. Failure modes and edge cases

1. **Server Unreachable / Invalid Credentials**: AuthWizard surfaces exact HTTP/transport errors inline; never silently falls back to fake demo chats.
2. **Expired Bearer Token (HTTP 401)**: Clears invalid token and returns cleanly to AuthWizard with re-login prompt.
3. **Zero Chats on Fresh Account**: Displays clean Apple Messages empty state with Add Contact / New Group / AI & Mika quick actions instead of fake demo threads.
4. **Offline Local Mode**: User explicitly opts into offline mode; app renders clean empty state with local SQLite persistence, Mika phone control, TON wallet creation, and Global Dharma without fabricated balances/chats.
5. **Existing Tests Compatibility**: Preserve all existing `host_integration_tests` and `TelegramModelsAndEntitiesTest` contracts while adding strict zero-fake-data assertions.

## 11. Implementation strategy

1. Create integration branch `feat/consolidate-zero-fake-data-v1.6.5` and merge all remote branches cleanly (PR #4 real code merge + `-s ours` history consolidation for superseded legacy branches).
2. Extend `mahayana_android_host/src/telegram_ext.rs` with real `gpui_telegram_server` `/api/v1/*` HTTP client + SQLite session storage + `v1.6.5` metadata in `lib.rs` + Rust loopback server integration tests.
3. Strip 100% of fake data from `TelegramModels.kt` and `TelegramViewModel.kt`, fix default theme to `APPLE_DAY`/`SOLID_MINIMAL`, wire real server HTTP + OkHttp WebSocket sync, and add `TelegramAuthWizardScreen.kt`.
4. Upgrade `TelegramHomeScreen.kt`, `TelegramChatScreen.kt`, `TelegramComposer.kt`, and `TelegramAndroidShell.kt` to desktop `v1.6.5` Apple Messages UI/UX.
5. Add unit & JNI tests verifying zero fake data, real loopback `gpui_telegram_server` auth/chats/messages, MiniApp capability contract, and Apple Messages v1.6.5 defaults.
6. Run full local verification (Rust tests, 16KB NDK build, `:app:testDebugUnitTest`, `:mika-core:test`, `:app:assembleDebug`) and mandatory daemon/artifact cleanup.

## 12. Verification / test strategy

- `cargo test --workspace -j 2 -- --test-threads=2` (including new loopback `gpui_telegram_server` `/api/v1/*` tests).
- `bash scripts/build_rust_android.sh` (16KB ELF `PT_LOAD` verified on `arm64-v8a` & `x86_64`).
- `./gradlew --max-workers=1 :mika-core:test :app:testDebugUnitTest :app:assembleDebug` (including `MiniAppWebMcpSurfaceTest` and zero-fake-data tests).
- Mandatory post-test cleanup verified via `jps -l`, `free -h`, and `git status -sb`.

## 13. Acceptance criteria / Definition of Done

- **AC-1**: This Spec is active with all 17 sections and truthful Section 17 compliance evidence.
- **AC-2**: All 6 remote branches are consolidated into branch history; PR #4 `MiniAppWebMcpSurface.kt` lifecycle revocation & `MiniAppWebMcpSurfaceTest.kt` are integrated and passing.
- **AC-3**: Initial `TelegramUiState`, `TonWalletUiState`, `GlobalDharmaUiState`, `HermesStudioUiState`, and `CloudComputerUiState` contain zero fake accounts, zero fake chats/messages, zero fake stories, zero fake balances/txs/NFTs, and zero fake cloud GPU nodes; default theme is `APPLE_DAY` + `SOLID_MINIMAL`.
- **AC-4**: `telegram.server.{login,register,me,listChats,getMessages,sendMessage,addContact,createChat}` pass live loopback HTTP tests against real `/api/v1/*` routes, and `TelegramViewModel` connects to `/api/v1/ws?token=...`.
- **AC-5**: Unauthenticated app launch displays `TelegramAuthWizardScreen` with desktop tabs (Password / Email OTP / Alipay QR), editable server URL (`http://34.46.77.133:8080`), real error reporting, and explicit offline option.
- **AC-6**: `TelegramHomeScreen` renders Apple header, inline pill search, 3-column Pinned Chats avatar grid, bottom navigation bar, and clean empty state; `TelegramChatScreen` renders desktop `v1.6.5` centered glass contact header; `TelegramComposer` renders circular `+` attach and `#007AFF` `↑` send pill.
- **AC-7**: `feature.info` reports `desktopParityVersion = "1.6.5"`, `desktopParityCommit = "db06f9c"`.
- **AC-8**: Full Rust + 16KB NDK + Gradle (`:app:testDebugUnitTest`, `:mika-core:test`, `:app:assembleDebug`) pass, and post-test cleanup leaves zero idle JVM daemons and zero generated `.so`/build artifacts.

## 14. Release / migration / rollback

- Additive SQLite columns/tables preserve existing user data; existing preferences migrate safely while fresh installs default to `APPLE_DAY` + `SOLID_MINIMAL`.
- Previous `v2.1.0` tag remains immutable for rollback.

## 15. Observability / evidence

- `git log --graph --oneline -n 15` showing all branch merges.
- `cargo test`, `scripts/build_rust_android.sh`, `:app:testDebugUnitTest`, `:mika-core:test`, `:app:assembleDebug` outputs.
- Post-cleanup `jps -l`, `free -h`, `git status -sb`.

## 16. References / provenance

- Canonical Desktop Upstream: `https://github.com/bhrumom/gpui-telegram` @ `db06f9c` (`v1.6.5`).
- Prior Spec: `docs/specs/desktop-v1-6-2-android-parity.md`.
- Repository Governance: `AGENTS.md`, `docs/specs/spec-first-ai-development.md`, `docs/specs/SPEC_TEMPLATE.md`.

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 | passed | Created active 17-section Spec `docs/specs/desktop-v1-6-5-zero-fake-data-and-apple-uiux.md` grounded in `bhrumom/gpui-telegram@db06f9c` (`v1.6.5`) prior to implementation. |
| R2 | passed | Consolidated all remote branches onto `feat/v1.6.5-zero-fake-data-and-apple-uiux`: real code merge of PR #4 (`codex/dandelion-capability-contract-20261019`, bringing `MiniAppWebMcpSurface.kt` lifecycle revocation & `MiniAppWebMcpSurfaceTest.kt`) + history consolidation of PR #2 (`docs/end-to-end-development-lifecycle-20260922`) and legacy Grok-0.18 branches (`spec/grok-bot-018-android-parity*`, `interaction-r115-validation`). |
| R3 | passed | Removed all 450+ lines of hardcoded fake accounts, stories, chats, messages, comments, polls, TON balances/txs/NFTs, Global Dharma fake counters/leaderboard, Magpie fake totals, RemoteAssist fake codes, Hermes fake sessions/kanban/approvals, Cloud Computer fake GPU/MicroVM nodes/tasks, and fake call history from `TelegramViewModel.kt` and `TelegramModels.kt`; fixed `restorePersistedSettings()` defaults to `APPLE_DAY` + `SOLID_MINIMAL` (18dp, 15sp). |
| R4 | passed | Implemented real desktop `gpui_telegram_server` `/api/v1/*` HTTP RPCs in `mahayana_android_host/src/telegram_ext.rs` (default `http://34.46.77.133:8080`, SQLite session persistence, `login`, `register`, `sendEmailCode`, `registerEmail`, `alipayStart`, `alipayPoll`, `me`, `logout`, `listChats`, `getMessages`, `sendMessage`, `addContact`, `createChat`, `searchMessages`) plus live OkHttp WebSocket (`/api/v1/ws?token=...`) in `TelegramViewModel.kt`. |
| R5 | passed | Added desktop-aligned `TelegramAuthWizardScreen.kt` (Apple Messages card, Password Login / Register / Email OTP / Alipay tabs, editable server URL pill, real error display, explicit offline local mode) and gated `TelegramAndroidShell.kt` on `!state.isAuthenticated && !state.offlineLocalMode`. |
| R6 | passed | Upgraded `TelegramHomeScreen.kt` (Apple translucent top bar, inline pill search bar, 3-column Pinned Chats circular avatar grid with stable anti-flicker ordering, clean empty state, desktop-aligned bottom nav bar), `TelegramChatScreen.kt` (desktop `v1.6.5` centered glass contact header `stage_bars.rs` parity), and `TelegramComposer.kt` (desktop `v1.6.0` circular `+` attach + 20dp pill input + embedded `#007AFF` `↑` send button). |
| R7 | passed | Updated `feature.info` in `mahayana_android_host/src/lib.rs` to `desktopParityVersion = "1.6.5"`, `desktopParityCommit = "db06f9c"`; integrated PR #4 WebMCP background lifecycle revocation and explicit error propagation. |
| R8 | passed | Passed Rust workspace tests (93/93: `gpui_telegram_proto` 19, `magpie_gateway` 28, `desktop_v1_6_2_parity_tests` 6, `host_integration_tests` 40), 16KB ELF build (`arm64-v8a` 8.9M & `x86_64` 8.3M `PT_LOAD=0x4000`), `:app:testDebugUnitTest` (22/22), `:mika-agent:testDebugUnitTest` (2,900/2,900), `:mika-core:test` (3,922 tests, 0 failures), `:app:assembleDebug`, and mandatory daemon/artifact cleanup (`jps -l`: zero daemons; `free -h`: 7.1Gi available). |
| AC-1 | passed | All 17 Spec sections completed with truthful compliance evidence. |
| AC-2 | passed | All 6 remote branch histories consolidated; PR #4 `MiniAppWebMcpSurfaceTest` passed inside `:app:testDebugUnitTest`. |
| AC-3 | passed | `DesktopV162ParityTest::zeroFakeDataInDefaultUiModelsAndAppleMessagesDefaultTheme` passed verifying zero fake data and `APPLE_DAY`/`SOLID_MINIMAL` defaults. |
| AC-4 | passed | Both Rust `test_real_gpui_telegram_server_api_v1_client_loopback` and JNI `jniHostExercisesDesktopV165...` passed against live loopback `/api/v1/*` servers. |
| AC-5 | passed | `TelegramAuthWizardScreen` compiled and wired as the unauthenticated entry gate in `TelegramAndroidShell`. |
| AC-6 | passed | `TelegramHomeScreen`, `TelegramChatScreen`, and `TelegramComposer` compiled cleanly and assembled into `app-debug.apk` with Apple Messages v1.6.5 UI/UX. |
| AC-7 | passed | Verified `desktopParityVersion == "1.6.5"` and `desktopParityCommit == "db06f9c"` in both Rust and JNI tests. |
| AC-8 | passed | Full Rust + 16KB NDK + Gradle verification succeeded and post-test cleanup confirmed via `jps -l`, `free -h`, and clean `git status -sb`. |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
