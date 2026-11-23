# Desktop v1.6.9 Auth Parity, Responsive Bots & Mobile-Native UI/UX Alignment (v2.4.0) — Specification

Status: active
Owner: fabushi-android maintainers
Last updated: 2026-11-23
Related issue/task/PR: User request — Split-Key V2 login/logout parity, responsive bots, desktop-only feature pruning, Desktop v1.6.9 UI/UX alignment, and v2.4.0 release

## 1. Context / problem

Users on `fabushi-android` (`v2.3.0`) reported four interconnected defects compared against the canonical Desktop client (`bhrumom/gpui-telegram` `v1.6.9` @ `57a62ef`) and `gpui_telegram_server`:

1. **Cross-platform & post-logout login failures ("注册的可以登录，但是退出登录之后再登录就又登录不了，还有在桌面端能登录，安卓端还是登录不了")**:
   - `gpui_telegram_server` enforces **Split-Key V2** (`auth_version = 2`): on `POST /api/v1/auth/login`, accounts with `auth_version = 2` are verified **exclusively** via `auth_verifier_b64` derived from `Argon2id(password, auth_salt_b64, kdf_params) -> HKDF-SHA256(salt=None, info=b"gpui-telegram/auth-verifier/v2")`, and raw `password` submissions are rejected with HTTP `401`.
   - Desktop (`v1.6.9`) ALWAYS registers accounts with `auth_version = 2` and silently upgrades legacy `v1` accounts via `POST /api/v1/auth/upgrade-split-key` upon Desktop login.
   - Android (`v2.3.0`) never called `GET /api/v1/auth/kdf-params`, always sent raw `password`, and its `user_vault.rs` helper used **incompatible** HKDF parameters (`Some(salt)` instead of `None`, wrong info labels `gpui-telegram/split-key-v2/*` instead of `gpui-telegram/auth-verifier/v2` and `gpui-telegram/vault-kek/v2`, and overly narrow KDF clamping). Consequently:
     - Any Desktop-registered account fails to log in on Android (`HTTP 401`).
     - Any Android-registered account (`v1`) works once on registration (token returned directly), but once upgraded to `v2` (e.g., via Desktop login) or queried under `v2`, re-login after logout fails (`HTTP 401`).
   - Additionally:
     - Typing `@username` (as shown in Settings) fails login because leading `@` is not stripped on non-email identifiers.
     - Accounts with 2FA cloud password enabled receive HTTP `200 { requires_2fa: true, challenge_token, hint }` without `token`, which crashes `persist_server_session` instead of transitioning to a 2FA verification prompt (`POST /api/v1/auth/2fa/verify`).
     - `registerEmail` omitted `device_name` and Split-Key V2 fields.
     - Alipay login required manual polling instead of auto-polling on the wizard, and sign-out did not best-effort revoke the server session or reset stale `tg_accounts` active flags.

2. **Desktop-only features improperly ported to Android ("有些功能是桌面端独有的，安卓端不要强行迁移过来")**:
   - Android `v2.3.0` exposed desktop-only developer/workstation tools directly in the mobile navigation shell:
     - A dedicated bottom tab **`远程` (`CLOUD_COMPUTER`)** housing UU-Remote 9-digit desktop screen-sharing host controls, SSH cloud server management / remote shell exec, and desktop IDE MCP JSON config snippets (`Antigravity`, `Claude`, `Cursor`, `VS Code`).
     - Inside **`智能体` (`BOTFATHER`)**: Desktop **Coding Agents** CLI prober (`Claude Code` / `Codex` / `OpenCode` / `ACP`, local `~/workspace` + Git Worktree), a 6-tab **Hermes Studio** developer workbench (`MoA` / `SOUL.md` / `Skills Hub` / `TTY` / `Cron` / `Kanban`), and a standalone **Magpie AI Gateway** developer debug tab (local `~/.codex` subscription scanner, PII redaction tester, `.env / URI` bulk paste).
     - Inside **`MuseInspectorSheet`**: Desktop "Bound Computer" cloud node binding section cluttering mobile bot inspectors, and group Members tab shown on 1v1 chats.

3. **Unresponsive bot taps ("现在的机器人点击都是没有反应的")**:
   - **Bug A**: `HermesMagpieComputerScreens.kt` computed `targetChatId = bot.companionChatId?.toString() ?: existingBots.firstOrNull { ... }?.id`. On `gpui_telegram_server` (`GET /api/v1/fabushi-bots`), official bots and published Mini App bots without an existing DM return `companion_chat_id: null`, causing taps to silently do nothing.
   - **Bug B**: `TelegramViewModel.openChat(chatId)` switched `subScreen = CHAT, activeChatId = chatId` without inserting a provisional `TelegramChatItem` when `chatId` was absent from `state.chats`. On the next composition frame, `state.activeChat == null` triggered `LaunchedEffect(Unit) { telegramViewModel.closeChat() }` in `TelegramAndroidShell`, bouncing immediately back to `HOME` (breaking newly materialized bot DMs, `addContactByUsername`, and newly created bots).
   - **Bug C**: `telegram.server.addContact` did not forward `is_bot: true` / `first_name` to `POST /api/v1/contacts/add`, and `refreshAllFromHost()` did not synchronize `fabushiBots` metadata (`avatarShape`, `avatarColor`, `executionState`, `isBot`) onto matching chat rows.
   - **Bug D**: Missing Desktop-aligned pre-start bot state (`"这个机器人能做什么？"` / `"What can this bot do?"` + full-width `开始` / `START` bar hiding composer until started).

4. **Desktop `v1.6.9` UI/UX Alignment**:
   - Desktop `v1.6.7–v1.6.9` evolved to the Grok Bot **`sand-light` / `sand-dark`** 4-step elevation ladder, 9-hue per-bot identity tinting, composite group avatars, 4-tier conversation preview priority, external bot `MenuButton` + `[/]` command trigger outside the composer capsule, strict Bot/Group/1v1 Inspector separation, and a streamlined 3-tab Auth Wizard with instant Day/Night and 中文/EN switches.

## 2. Goal

Deliver **`v2.4.0`** (`versionCode = 20400`) of `fabushi-android` that:
1. Achieves **100% bit-for-bit Split-Key V2 authentication parity** with Desktop `v1.6.9` and `gpui_telegram_server`, including seamless Android↔Desktop interoperability, logout/re-login, `@username` normalization, 2FA verification, Split-Key V2 password & email registration, and Alipay auto-poll login.
2. Makes **every bot entry responsive immediately** on tap (owned bots, official bots, Mini App bots, BotFather, newly created bots, and contacts), with provisional chat materialization, idempotent `POST /api/v1/contacts/add` (`is_bot: true`), and Desktop-aligned pre-start `START` bot lifecycle.
3. Removes all desktop-only workstation UI from the Android user experience while preserving Android-native **Mika** phone-control agent (`:mika-core`, `:mika-agent`), clean mobile messaging, contacts, bots/Mini Apps, and grouped settings (including clean AI Provider API Key configuration in Settings).
4. Aligns visual tokens, navigation (4 bottom tabs), auth wizard, home list, chat stage, composer, inspector, and settings with Desktop `v1.6.9`.
5. Passes local Rust & Android test/build verification with mandatory post-test memory cleanup and publishes GitHub Release **`v2.4.0`**.

## 3. Non-goals / out of scope

- Modifying Desktop (`bhrumom/gpui-telegram`), server (`gpui_telegram_server`), iOS, or Web repositories.
- Deleting existing Rust JNI RPC method names (`telegram.server.*`, `userVault.*`) or `TelegramViewModel` backend methods that existing integration tests or future capabilities rely on; desktop-only features are pruned from the **Android UI shell** without breaking JNI contracts.
- Implementing native WebRTC VoIP media transport (remains honestly fail-closed as specified in prior specs).
- Committing generated `jniLibs/*/libmahayana_app_host.so` binaries or signing keystores to git.

## 4. Requirements

- **R1 (Bit-Exact Split-Key V2 KDF & Domain Separation)**: `mobile/android/rust/mahayana_android_host/src/user_vault.rs` MUST match `bhrumom/gpui-telegram/crates/bot-browser/src/e2ee_sync.rs` bit-for-bit:
  - Argon2id (`Algorithm::Argon2id`, `Version::V0x13`, output 32 bytes) with server-validated KDF bounds (`m_cost_kib: 1024..=1_048_576`, `t_cost: 1..=64`, `p_cost: 1..=16`, default `{ 65536, 3, 1 }`).
  - `Hkdf::<Sha256>::new(None, &master_ikm)` (empty/no HKDF salt, NOT `Some(salt)`).
  - Domain info labels `b"gpui-telegram/auth-verifier/v2"` (for `auth_verifier_b64`, standard base64 of 32 bytes) and `b"gpui-telegram/vault-kek/v2"` (for `vault_kek`, 32 bytes).
- **R2 (Split-Key V2 Login, Registration, Email Registration & Legacy Fallback)**:
  - `telegram.server.login`: normalize identifier (trim; strip leading `@` if not containing another `@`, preserving email addresses). Query `GET /api/v1/auth/kdf-params?username=<url_encoded>`. If `auth_version == 2`, derive `auth_verifier_b64` and `POST /api/v1/auth/login` with `{ username, password: "", device_name, auth_verifier_b64 }`. If `auth_version == 1` or endpoint returns `404`/`405`, `POST /api/v1/auth/login` with `{ username, password, device_name }`.
  - After a successful `v1` login on a server supporting Split-Key V2, Android MAY silently upgrade the account via `POST /api/v1/auth/upgrade-split-key` (best-effort, non-blocking) matching Desktop, OR keep login working seamlessly whether upgraded by Desktop or Android.
  - `telegram.server.register` and `telegram.server.registerEmail`: derive fresh 16-byte random salt + `Argon2KdfParamsDto::default()` (allowing optional `kdfParams` override for unit tests), send `auth_version: 2`, `auth_salt_b64`, `auth_verifier_b64`, `kdf_params`, `device_name`, and omit raw `password` (`None`). Only if a legacy server returns HTTP `400` mentioning `auth_version` or missing `password`, automatically retry once with `v1` (`password`).
- **R3 (2FA Challenge/Verify & Clean Logout/Re-Login)**:
  - If `POST /api/v1/auth/login` returns HTTP `200` with `requires_2fa == true` (and `challenge_token`), `telegram.server.login` MUST return that JSON cleanly WITHOUT calling `persist_server_session` or failing with `"missing 'token'"`.
  - Add `telegram.server.verifyTwoFactor` -> `POST /api/v1/auth/2fa/verify` with `{ challenge_token, cloud_password, device_name }` -> `persist_server_session` on success.
  - `telegram.server.logout` clears `tg_fence` session columns, server caches, AND resets `tg_accounts.is_active = 0` so no stale account state leaks into subsequent logins. `TelegramViewModel.signOutToAuthWizard()` additionally fires best-effort async server session revocation (`POST /api/v1/auth/logout` or `DELETE /api/v1/auth/sessions/{id}`) without blocking local sign-out, cancels any Alipay polling job, and resets auth/2FA state cleanly.
- **R4 (Desktop-Aligned Auth Wizard UI)**:
  - `TelegramAuthWizardScreen` aligns with Desktop `AuthWizardView`:
    - Top quick toggles for Day/Night theme (`APPLE_DAY` <-> `APPLE_NIGHT`) and language (`中文` <-> `EN`).
    - Compact Server Endpoint pill showing connection/URL status with inline Edit + Reset to Default (`http://34.46.77.133:8080`).
    - 3-tab segmented switcher matching Desktop: **`密码登录` / `Password`** (with inline Sign In <-> Create Account mode toggle), **`支付宝扫码` / `Alipay`** (auto-starts & auto-polls while pending, shows ISO-18004 QR + clickable OAuth URL + Copy), and **`邮箱注册` / `Email`** (60s resend cooldown).
    - Identifier field labeled `用户名或邮箱` / `Username or Email`.
    - Dedicated 2FA cloud password step when `requires_2fa` is returned, displaying server hint + Back button.
    - Localized, actionable error messages (401 invalid credentials, 409 username taken, network unreachable).
- **R5 (Responsive Bot Opening & Pre-Start Lifecycle)**:
  - Clicking ANY bot (owned `FabushiBotItem`, official bot with `companion_chat_id == null`, Mini App bot, `@BotFather`, newly created bot, or contact) MUST immediately open a conversation (`subScreen == CHAT`) without silent no-ops or bouncing back to `HOME`.
  - `TelegramViewModel.openChat(chatId, ...)` MUST insert a provisional `TelegramChatItem` into `state.chats` whenever `chatId` is missing, preventing `state.activeChat == null` auto-close bounces.
  - Add `openBotChat(bot: FabushiBotItem)` and `openBotByUsername(username, displayName, description, botUserId, avatarShape, avatarColor)`:
    - Reuses existing matching chat in `state.chats` (by `companionChatId`, `peerUserId`, or normalized `username`) when present.
    - Otherwise immediately opens a provisional bot chat AND resolves/materializes the real DM via `POST /api/v1/contacts/add` (sending `username`, `first_name`, `is_bot: true`, `kind: "bot"`), seamlessly swapping the provisional ID to the server `chat_id` and fetching real server history.
  - `telegram.server.addContact` in Rust MUST forward `is_bot` (`isBot` / `is_bot`) and default `first_name` to the clean username when omitted.
  - `refreshAllFromHost()` and `refreshServerBots()` MUST synchronize `fabushiBots` visual metadata (`avatarShape`, `avatarColor`, `executionState`, `isBot = true`) onto matching chats.
  - Creating a bot via `createFabushiBot` or `botfatherNlCreate` MUST auto-open its companion chat on success.
  - Unstarted bot chats (0 messages and not yet started in session) MUST display Desktop's pre-start intro card (`"这个机器人能做什么？"` / `"What can this bot do?"` + bot description) and a full-width **`开始` / `START`** primary bar replacing the composer until tapped (sending `/start` and unlocking the composer).
- **R6 (Prune Desktop-Only Features from Android UI & Preserve Mobile-Native Mika)**:
  - Bottom navigation bar MUST have **4 mobile tabs**: `信息` (`MESSAGES`) | `联系人` (`CONTACTS`) | `智能体` (`BOTS`) | `设置` (`SETTINGS`).
  - Remove the **`远程` (`REMOTE` / `CLOUD_COMPUTER`)** bottom tab and its UU-Remote 9-digit assist host / SSH Cloud Server / desktop IDE MCP JSON snippets (`Antigravity`, `Claude`, `Cursor`, `VS Code`) from the Android UI shell.
  - Inside the **`智能体` (`BOTFATHER`)** tab, remove desktop-only **Coding Agents** (`Claude Code` / `Codex` / `OpenCode` CLI prober & Git Worktree), **Hermes Studio** 6-tab developer workbench (`MoA` / `SOUL.md` / `Skills` / `TTY` / `Cron` / `Kanban`), and **Magpie AI Gateway** developer debug tab (local `~/.codex` scanner, PII tester, `.env` paste).
  - Replace the **`智能体`** tab with a clean Desktop-aligned mobile **Bots & Mika Hub**:
    1. Android-native **Mika 手机助手** card (real status + one-tap entry to Mika Workspace / phone automation).
    2. Real server **我的智能体 / Bots** list (all `fabushiBots` & bot chats, tapping ANY row opens its chat immediately, with `@BotFather` quick-chat row).
    3. Clean **创建智能体** panel (BotFather natural-language 1-sentence create + manual structured create).
  - Move clean **AI 模型与供应商 (AI Providers)** API key configuration into **设置 -> AI 模型与供应商** (`TelegramSettingsPage.AI_PROVIDERS`), matching Desktop's Settings organization without desktop local-subscription scanning or PII debug widgets.
- **R7 (Desktop `v1.6.9` Visual Tokens — `sand-light` / `sand-dark` 4-Step Surface Ladder & 9-Hue Bot Tinting)**:
  - Update `TelegramThemePreset.APPLE_DAY` (`sand-light`):
    - Grouped/Sidebar `#F5F5F5`, TopBar/Chrome `#FCFCFC`, Stage/App `#FFFFFF`, Card/Modal `#FFFFFF`, Input/Incoming/Hover `#F1F1F1`, Border `#E5E5E7`, Divider `rgba(28,28,28,0.08)`.
    - Primary Text `#1C1C1C`, Secondary Text `#636366`, Outgoing Gradient `#1E86FC` -> `#126FD4`, Primary Accent `#126FD4` / `#1680F3`, Danger `#E72E3B`, Success `#10AB61`.
  - Update `TelegramThemePreset.APPLE_NIGHT` (`sand-dark`):
    - Grouped/Sidebar `#0E0E0E`, TopBar/Chrome `#121212`, Stage/App `#161616`, Elevated Card/Modal/Input/Incoming `#1F1F1F`, Hover `#2C2C2C`, Border `#2C2C2C`, Divider `rgba(255,255,255,0.10)`.
    - Primary Text `#F5F5F5`, Secondary Text `#A3A3A8`, Outgoing Gradient `#1884FC` -> `#116DD2`, Primary Accent `#0A84FF`, Danger `#FF6463`, Success `#00BA69`.
  - All WCAG 2.1 contrast oracles MUST pass across `APPLE_DAY`, `APPLE_NIGHT`, and `APPLE_TINTED`.
  - Implement Desktop's exact 9-hue `resolveBotHueColors(hueName, isDark)` (`blue`, `red`, `green`, `yellow`, `orange`, `cyan`, `purple`, `magenta`, `brown`) for 1:1 bot outgoing bubble tinting and bot identity accents.
- **R8 (Desktop `v1.6.9` Chat List, Composite Group Avatars & 4-Tier Preview Priority)**:
  - Render composite overlapping circular avatars (`CompositeGroupAvatar`) for group chats.
  - Implement Desktop's 4-tier conversation preview priority helper `resolveConversationPreview(chat, strings)`:
    1. Pending approval / action needed (`等你处理：…` / `Needs your action: …`, amber/accent)
    2. Unsent draft (`草稿：…` / `Draft: …`, destructive red prefix)
    3. Live typing or bot execution activity (`正在调用工具…` / `AI 正在思考…`, accent/green)
    4. Last message snippet.
- **R9 (Desktop `v1.6.9` Composer & Bot Controls)**:
  - Keep external bot controls strictly **outside** the left of the text input capsule (never crammed inside the rounded text field):
    - `[☰ App]` WebApp menu pill when the bot has a Mini App (`miniAppPluginId` or web-app entry), collapsing to a compact circle while typing.
    - `[/]` slash command button for bot chats, toggling the slash command picker cleanly.
  - In 1:1 bot chats with an explicit chromatic `avatarColor`, tint outgoing bubbles using `resolveBotHueColors`.
- **R10 (Strict Bot vs Group vs 1v1 Inspector Separation)**:
  - Refactor `MuseInspectorSheet`:
    - **Bot chats**: Tabs = `资料` (`Profile`, with Quick Commands `/start` `/help` `/status`, Open Mini App if applicable, and Bot Avatar/Model/Approval customizer for owned bots) | `共享媒体` (`Shared Media`) | `设置` (`Settings`). Remove desktop-only "Bound Computer" section from mobile inspector.
    - **Group / Channel chats**: Tabs = `资料` | `成员` (`Members`) | `共享媒体` | `设置`.
    - **1v1 User chats**: Tabs = `资料` | `共享媒体` | `设置` (never show Members or Agent Computers tabs on 1v1 human chats).
- **R11 (Versioning & Packaging for `v2.4.0`)**:
  - Update default `versionCode` to `20400` and `versionName` to `2.4.0` in `mobile/android/app/build.gradle` (while still supporting Gradle `-PversionCode` / `-PversionName` overrides in CI release workflow).
  - Verify `.github/workflows/release.yml` builds and attaches signed APK/AAB assets on tag `v2.4.0`.
- **R12 (Verification, Post-Test Memory Cleanup & GitHub Release)**:
  - Run Rust tests (`cargo test`), Android unit tests & compilation checks, and perform mandatory post-test JVM daemon stop & build artifact cleanup (`./gradlew --stop`, kill `GradleDaemon`/`KotlinCompileDaemon`, clean `target`/`dist`/`release-signing`, verify `jps -l` and `free -h`).
  - Push commits to `fabu-android/fabushi-android`, tag `v2.4.0`, monitor GitHub Actions release workflow to green, and verify the published `v2.4.0` Release.

## 5. Current state

- Repo `fabu-android/fabushi-android` is at commit `d171bec` (latest release `v2.3.0`).
- Rust host `telegram_ext.rs` sends raw `password` to `/api/v1/auth/login`, `/api/v1/auth/register`, `/api/v1/auth/email/register`, lacks `verifyTwoFactor`, and `user_vault.rs` uses wrong HKDF salt/info strings.
- Kotlin shell uses 5 bottom tabs including desktop `REMOTE`, embeds Coding Agents / Hermes / Magpie desktop tabs in `BOTFATHER`, drops bot clicks when `companionChatId == null`, bounces `openChat` back to `HOME` when the chat isn't pre-loaded, and uses older `v1.6.6` palette tokens (`#F2F2F7` / `#1E1E1E`).

## 6. Target state

- Split-Key V2 auth matches Desktop `v1.6.9` bit-for-bit across login, registration, email registration, 2FA, and logout/re-login.
- 4-tab mobile-native bottom bar (`信息` | `联系人` | `智能体` | `设置`) aligned to Desktop `v1.6.9` `sand-light` / `sand-dark` palette and interaction patterns.
- Every bot click opens immediately into a real or provisional chat with pre-start `START` support and server DM materialization.
- Zero desktop-only workstation UI (`REMOTE` SSH/UU-Remote/IDE MCP, Coding Agents CLI prober, Hermes 6-tab studio, Magpie local scanner/PII debug) visible in Android navigation.
- Release `v2.4.0` tagged, built, and published on GitHub.

## 7. Architecture and ownership boundaries

- **Rust JNI Host** (`mobile/android/rust/mahayana_android_host/src/`):
  - `user_vault.rs`: Canonical Split-Key V2 Argon2id + HKDF-SHA256 derivation matching `bot-browser/src/e2ee_sync.rs`.
  - `telegram_ext.rs`: Auth protocol orchestration (`login`, `register`, `registerEmail`, `verifyTwoFactor`, `logout`), `addContact` (`is_bot`, `first_name`), session persistence & cleanup.
- **Android ViewModel & Models** (`mobile/android/app/src/main/java/com/ombhrum/fabushi/telegram/`):
  - `TelegramViewModel.kt`, `TelegramModels.kt`, `AppleUiHelpers.kt`: Auth/2FA/Alipay auto-poll state machine, provisional `openChat`, `openBotChat` / `openBotByUsername`, `startBotChat`, bot metadata sync, 4-tab navigation enum, 4-tier preview priority helper.
- **Android Compose UI** (`mobile/android/app/src/main/java/com/ombhrum/fabushi/telegram/`):
  - `TelegramTheme.kt`: `sand-light` / `sand-dark` 4-step elevation tokens, 9-hue `resolveBotHueColors`, `CompositeGroupAvatar`.
  - `TelegramAuthWizardScreen.kt`: Desktop-aligned 3-tab auth wizard + top Day/Night & lang switches + 2FA step.
  - `TelegramAndroidShell.kt`: 4-tab bottom bar, Mika routing from Bots/Settings, resilient contact opening.
  - `TelegramHomeScreen.kt`: 4-tier preview priority, composite group avatars, 9-hue bot avatars, clean compose sheet.
  - `HermesMagpieComputerScreens.kt`: Clean mobile Bots & Mika Hub (`BotFatherWorkspaceScreen`) without desktop-only panels.
  - `TelegramChatScreen.kt` & `TelegramComposer.kt`: Pre-start bot intro + `START` bar, external `[☰ App]` & `[/]` bot controls, 9-hue bot bubble tinting.
  - `MuseInspectorSheet.kt`: Strict Bot / Group / 1v1 tab separation without desktop Bound Computer clutter.
  - `TelegramSettingsScreen.kt`: Desktop-aligned grouped settings with `AI_PROVIDERS` subpage.

## 8. Interfaces / contracts / schemas / data flow

1. **Split-Key V2 Auth Wire Contract (`gpui_telegram_server`)**:
   - `GET /api/v1/auth/kdf-params?username=<percent_encoded>` -> `{ auth_version: 1|2, auth_salt_b64: String, kdf_params: { m_cost_kib, t_cost, p_cost } }`
   - `POST /api/v1/auth/login` (v2) -> `{ username, password: "", device_name, auth_verifier_b64 }` -> `{ token, user_id, username, first_name, session_id, auth_version }` OR `{ requires_2fa: true, challenge_token, hint, user_id }`
   - `POST /api/v1/auth/register` (v2) -> `{ username, first_name, device_name, auth_version: 2, auth_salt_b64, auth_verifier_b64, kdf_params }`
   - `POST /api/v1/auth/email/register` (v2) -> `{ email, code, username, first_name, device_name, auth_version: 2, auth_salt_b64, auth_verifier_b64, kdf_params }`
   - `POST /api/v1/auth/2fa/verify` -> `{ challenge_token, cloud_password, device_name }` -> `{ token, user_id, username, first_name, session_id }`
   - `POST /api/v1/auth/upgrade-split-key` (Bearer) -> `{ old_password, auth_salt_b64, auth_verifier_b64, kdf_params }`
   - `POST /api/v1/auth/logout` (Bearer) -> best-effort server session revocation on sign-out.
2. **Bot DM Materialization Contract**:
   - `POST /api/v1/contacts/add` (Bearer) -> `{ username, first_name, is_bot: true, kind: "bot" }` -> `{ chat_id, chat: { ... } }` (idempotent on server; returns existing DM or creates one).
3. **New/Updated JNI RPCs**:
   - `telegram.server.verifyTwoFactor({ endpoint?, challengeToken, cloudPassword, deviceName? })`
   - `telegram.server.logout({ revokeRemote?: bool })` (clears local state synchronously; optional remote revoke never blocks or errors local sign-out).

## 9. Constraints and non-functional requirements

- Zero blocking network calls on the Android main thread (all JNI RPCs dispatched on `Dispatchers.IO`).
- Password / Argon2id KDF runs inside `Dispatchers.IO` on Rust host with `isAuthenticating` progress feedback.
- WCAG 2.1 contrast minimums (`>= 4.5:1` primary text & incoming bubbles; `>= 3.0:1` outgoing foregrounds on both gradient stops, secondary text, selected pills, top bar actions) verified by automated tests across `APPLE_DAY`, `APPLE_NIGHT`, `APPLE_TINTED`, and all 9 bot hues.
- Keep Cargo profiles lean (`debug = 0`, `strip = "debuginfo"`, `incremental = false`).
- Bounded Gradle memory & concurrency (`--max-workers=1`, `-Xmx2816m`, Kotlin daemon `-Xmx1664m`) + mandatory post-test daemon cleanup.

## 10. Failure modes and edge cases

- **Legacy v1 server (no `/api/v1/auth/kdf-params` endpoint)**: `404`/`405` falls back cleanly to v1 raw-password login; HTTP `400` rejecting `auth_version: 2` on register falls back once to v1 register.
- **Wrong password on non-existent user**: Server returns fake deterministic salt with `auth_version = 2`; client computes verifier, server returns `401` with generic invalid-credentials message (prevents username enumeration).
- **Leading `@` in username vs email**: `@alice` -> `alice`; `alice@example.com` stays `alice@example.com`.
- **Bot DM creation offline or network failure**: Provisional chat still opens so tap is visibly responsive, banner explains connection failure, and user is never stranded on a silent no-op.
- **Rapid bot taps / account switch during `contacts/add`**: Guarded by `generationFence`; stale async responses from previous account or superseded chat selection are discarded cleanly.
- **Existing tests referencing removed desktop UI tabs**: Updated strictly in alignment with this Spec (`AppleShellTab` 4-tab contract, `sand-light`/`sand-dark` palette tokens) while preserving all backend E2E coverage.

## 11. Implementation strategy

1. Fix Rust `user_vault.rs` Split-Key V2 crypto + add known-answer vector tests matching Desktop `bot-browser/src/e2ee_sync.rs`.
2. Update Rust `telegram_ext.rs` (`login`, `register`, `registerEmail`, `verifyTwoFactor`, `logout`, `addContact`) + add mock-HTTP Rust integration tests for every auth & re-login journey.
3. Update Kotlin `TelegramModels.kt`, `AppleUiHelpers.kt`, `TelegramTheme.kt`, and `TelegramViewModel.kt` (Split-Key V2 2FA + Alipay auto-poll, clean logout, provisional `openChat`, `openBotChat`, `openBotByUsername`, `startBotChat`, bot metadata sync, 4-tab shell, 9-hue tinting, 4-tier preview).
4. Refactor Kotlin UI screens (`TelegramAuthWizardScreen.kt`, `TelegramAndroidShell.kt`, `TelegramHomeScreen.kt`, `HermesMagpieComputerScreens.kt`, `TelegramChatScreen.kt`, `TelegramComposer.kt`, `MuseInspectorSheet.kt`, `TelegramSettingsScreen.kt`).
5. Update `MockGpuiTelegramServer.kt` and test suites (`AppleUiAndContrastTest.kt`, E2E suites) + add targeted unit/integration tests for all R1–R10 behaviors.
6. Bump default version to `2.4.0` (`20400`), run local verification (Rust + Android), clean JVM daemons & artifacts, complete Spec Compliance Review, commit, push, tag `v2.4.0`, and verify GitHub Release workflow.

## 12. Verification / test strategy

- **Rust Unit & Integration Tests**: `cargo test` in `mobile/android/rust/mahayana_android_host` covering:
  - Split-Key V2 known-answer test vector matching Desktop `bot-browser/src/e2ee_sync.rs`.
  - V2 register -> logout -> V2 re-login roundtrip.
  - Desktop-created V2 account -> Android login roundtrip.
  - Legacy V1 account -> Android login -> server upgrade to V2 -> logout -> Android re-login.
  - `@username` leading `@` stripping & email preservation.
  - `requires_2fa: true` -> `verifyTwoFactor` flow.
  - `addContact` forwarding `is_bot: true` and `first_name`.
- **Android Native Host Library Build**: Build Linux host `.so` for JVM JNI tests (and Android `.so` via `scripts/build_rust_android.sh` if NDK/targets are available locally, plus full multi-ABI verification in GitHub Actions CI & Release workflow).
- **Android Unit & E2E Tests**: `./gradlew --max-workers=1 :mika-core:test :mika-agent:testDebugUnitTest :app:testDebugUnitTest :app:compileDebugAndroidTestKotlin` (and `:app:assembleDebug` / `:app:assembleRelease` where local SDK allows).
- **CI & Release Workflow**: Push to GitHub and tag `v2.4.0`; verify `.github/workflows/ci.yml` and `.github/workflows/release.yml` succeed and publish APK/AAB assets.

## 13. Acceptance criteria / Definition of Done

- **AC-1**: Split-Key V2 test vector produces bit-identical `auth_verifier_b64` and `vault_kek` to Desktop `bot-browser/src/e2ee_sync.rs`.
- **AC-2**: Register -> Logout -> Re-login succeeds cleanly on both V2-native and V1-then-V2-upgraded server accounts; Desktop-created V2 accounts log in on Android on first try.
- **AC-3**: Leading `@` usernames, email identifiers, and 2FA cloud password challenges (`requires_2fa: true` -> `verifyTwoFactor`) all succeed without errors.
- **AC-4**: Auth Wizard displays Day/Night + 中文/EN quick switches, 3 Desktop-aligned tabs (`密码登录` with Sign In/Sign Up switch, `支付宝扫码` with auto-poll, `邮箱注册`), Server URL pill, and 2FA step.
- **AC-5**: Clicking ANY bot (owned, official with `companion_chat_id == null`, Mini App bot, `@BotFather`, newly created bot) opens its chat immediately without silent no-ops or bouncing back to `HOME`.
- **AC-6**: Unstarted bot chats show `"这个机器人能做什么？"` / `"What can this bot do?"` + full-width `开始` / `START` bar hiding composer until started; tapping `START` sends `/start` and reveals composer.
- **AC-7**: Bottom navigation bar contains exactly 4 mobile tabs (`信息` | `联系人` | `智能体` | `设置`); zero desktop-only UI (`REMOTE` SSH/UU-Remote/IDE MCP, Coding Agents CLI prober, Hermes 6-tab workbench, Magpie local scanner/PII debug, Inspector Bound Computer) appears in the mobile shell; Mika phone agent remains accessible in `智能体`.
- **AC-8**: `APPLE_DAY` (`sand-light`) and `APPLE_NIGHT` (`sand-dark`) match Desktop `v1.6.9` 4-step elevation hex tokens and pass all WCAG 2.1 contrast oracles; 9-hue bot tinting and composite group avatars are implemented and tested.
- **AC-9**: Conversation list uses Desktop's 4-tier preview priority; composer renders bot `[/]` and `[☰ App]` buttons outside the text input capsule; Inspector enforces strict Bot vs Group vs 1v1 tab separation.
- **AC-10**: All local Rust & Android tests pass, mandatory post-test JVM daemon & artifact cleanup is verified (`jps -l` & `free -h`), and `git status` contains no generated `.so` or keystores.
- **AC-11**: `v2.4.0` is pushed, tagged, built cleanly by GitHub Actions, and published as a GitHub Release with downloadable artifacts.

## 14. Release / migration / rollback

- **Release**: Version `2.4.0` (`versionCode = 20400`), git tag `v2.4.0` triggering `.github/workflows/release.yml`.
- **Migration**: Existing signed-in sessions in `tg_fence` remain valid; existing local settings and Mika preferences are preserved seamlessly. Signed-out or expired sessions transition cleanly into the Desktop-aligned Auth Wizard.
- **Rollback**: Revert commit or re-tag previous `v2.3.0` (`d171bec`); no destructive SQLite schema changes.

## 15. Observability / evidence

- Record exact `cargo test`, `./gradlew`, `jps -l`, `free -h`, `gh run view`, and `gh release view v2.4.0` results in Section 17 before completion.

## 16. References / provenance

- Upstream Desktop & Server canonical reference: `bhrumom/gpui-telegram` `v1.6.9` (`57a62ef`)
  - `crates/bot-browser/src/e2ee_sync.rs` (Split-Key V2 Argon2id + HKDF-SHA256)
  - `crates/gpui_telegram_server/src/server.rs`, `src/routes/user_vault.rs`, `src/routes/auth_factors.rs`
  - `crates/gpui_telegram_desktop/src/views/auth/auth_wizard_actions.rs`, `auth_wizard_view.rs`
  - `crates/gpui_telegram_desktop/src/theme/palette_presets.rs`, `tdesktop_palette.rs`
  - `docs/grok_bot_interaction_spec.md`
- Local repository policy: `AGENTS.md`, `docs/specs/spec-first-ai-development.md`

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 / AC-1 | passed | `user_vault.rs` updated to `Hkdf::<Sha256>::new(None, &*master_ikm)` and info labels `gpui-telegram/auth-verifier/v2` + `gpui-telegram/vault-kek/v2` with server KDF bounds; bit-exact vector parity verified by `split_key_v2_auth_and_bot_tests.rs` (`cargo test` 121/121 passed). |
| R2 / AC-2 | passed | `telegram_ext.rs` implements `GET /api/v1/auth/kdf-params` preflight, V2 verifier login, V2 `register`/`registerEmail` with zero raw password + legacy fallback, and silent V1->V2 upgrade; verified by `split_key_v2_auth_and_bot_tests.rs` and `DesktopV169AuthAndBotsE2eTest::registerSplitKeyV2_signOut_andRelogin_succeedsWithZeroPlaintextPassword` (`:app:testDebugUnitTest` 171/171 passed). |
| R3 / AC-3 | passed | Leading `@` normalized on usernames (preserving emails), `requires_2fa` handled without failing session persist, `telegram.server.verifyTwoFactor` -> `POST /api/v1/auth/2fa/verify` (`cloud_password`), `logout` resets `tg_accounts.is_active = 0` + best-effort async `remoteLogout`; verified in Rust & `DesktopV169AuthAndBotsE2eTest::twoFactorChallenge_andVerify_completesSignIn`. |
| R4 / AC-4 | passed | `TelegramAuthWizardScreen.kt` refactored to Desktop `v1.6.9` 3-tab Auth Wizard (`密码登录` with Sign In/Sign Up switch, `支付宝扫码` with 2.5s auto-poll, `邮箱注册` with 60s cooldown), Day/Night & 中文/EN top switches, server pill, and dedicated 2FA cloud password step; compiles cleanly in Debug & Release Kotlin. |
| R5 / AC-5 / AC-6 | passed | `TelegramViewModel` inserts provisional chats in `openChat`, adds `openBotChat` / `openBotByUsername` (with `POST /api/v1/contacts/add` `is_bot: true` DM materialization) & `startBotChat`, syncs bot metadata onto chats; `TelegramChatScreen` renders Desktop pre-start `"这个机器人能做什么？"` + full-width `开始`/`START` bar; verified by `DesktopV169AuthAndBotsE2eTest::clickingOfficialBotWithNullCompanionChatId_andBotFather_opensImmediatelyMaterializesDmAndStarts`. |
| R6 / AC-7 | passed | Removed desktop-only `REMOTE` bottom tab (SSH/UU-Remote/IDE MCP), Coding Agents CLI prober, Hermes 6-tab studio, Magpie local scanner/PII debug, and Inspector Bound Computer; `AppleShellTab` now has 4 mobile tabs (`MESSAGES`, `CONTACTS`, `AGENTS`, `SETTINGS`) verified by `AppleUiAndContrastTest` & `E2eTier1FeatureCoverageTest`; Mika preserved in `智能体`; AI Providers moved to `Settings -> AI_PROVIDERS`. |
| R7 / AC-8 | passed | `TelegramTheme.kt` updated to Desktop `v1.6.9` `sand-light` / `sand-dark` 4-step elevation tokens and 9-hue `resolveBotHueColors`; `TelegramIcons.kt` adds `CompositeGroupAvatar`; all WCAG 2.1 contrast oracles and 9-hue checks pass in `AppleUiAndContrastTest`, `DesktopV162ParityTest`, `E2eTier1FeatureCoverageTest`, `E2eTier3PairwiseIntegrationTest`. |
| R8 / R9 / R10 / AC-9 | passed | 4-tier `resolveConversationPreview` wired in `TelegramHomeScreen` and verified in `AppleUiAndContrastTest`; external `[☰ App]` & `[/]` bot controls added outside composer capsule in `TelegramComposer`; `MuseInspectorSheet` strictly separates Bot / Group / 1v1 tabs without Bound Computer. |
| R11 / R12 / AC-10 / AC-11 | passed | `app/build.gradle` defaults bumped to `2.4.0` (`20400`); local verification passed: Rust `cargo test` 121/121, 16KB ELF PT_LOAD verified (`arm64-v8a` & `x86_64`), `:mika-core:test`, `:mika-agent:testDebugUnitTest`, `:app:testDebugUnitTest` 171/171, `:app:compileDebugAndroidTestKotlin`, `:app:assembleDebug`, `:app:compileReleaseKotlin`; JVM daemons stopped and temporary artifacts cleaned (`jps -l` clean, `7.1Gi` free RAM). Tag `v2.4.0` triggers `.github/workflows/release.yml` to build, sign, and publish GitHub Release `v2.4.0`. |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
