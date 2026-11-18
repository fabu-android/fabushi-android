# Upstream `gpui-telegram` Portable Rust Provenance

- **Upstream Repository**: `bhrum/gpui-telegram`
- **Pinned Commit SHA**: `889246e4621e3e8e58022895a887c63ea3130bdd`
- **Spec Reference**: `docs/specs/gpui-telegram-android-production-parity.md` (§4.1 `F3`, `F5`, `F6`)

## Included Portable Crates (`mobile/android/rust/gpui_telegram_vendor/crates/`)

1. `fabushi_miniapp` — Shared pure-Rust Global Dharma Offering (`全球法布施`) & Zen Room core (`sutras`, `countries`, `achievements`, `scene_math`, `state`, `storage`) compiled with the lightweight `sqlite` feature flag (retaining `assets/concap.csv` and `assets/r2-files-list.json` while excluding desktop `.wasm` and `.webp`/`.jpg` assets).
2. `magpie_gateway` — Shared pure-Rust Magpie multi-provider AI gateway core (`presets`, `protocol` 4-way Rosetta wire translators & SSE stream chunk parsers, `routing` 8 strategies + `CircuitBreaker`, `masking` runtime-salted `MaskingEngine` + `StreamingUnmasker`, `import` `magpie://import` & `.env` parsers, `subscriptions`, `usage` `UsageLedger`), compiled with `default-features = false` so `axum`/`tokio` server dependencies are excluded on Android (`ureq` + `rustls` drives live HTTPS/SSE transport).
3. `gpui_telegram_core` — Portable core identifiers (`ChatId`, `PeerId`, `UserId`, `MessageId`, `TopicId`), `GenerationFence`, `SessionToken`, 5-tier `action_bus` risk schema, `mtproto`, and `passport`.
4. `gpui_telegram_proto` — Portable Telegram wire contracts (`api/*`), 333-emoji `calls_emoji_fingerprint`, 5-bit `waveform` codecs, `miniapp_contracts` (`@BotFather`, `initData` HMAC, Grok/Fabushi avatar hashes, `MiniAppPermission`), `cards`, `payments`, `semantic_tree`, `sync`, `tailnet_contracts`, and `ws`.

## Explicitly Excluded Desktop / Server Crates

- `crates/gpui_telegram_desktop` (GPUI / Metal / Vulkan desktop windowing)
- `crates/gpui_telegram_server` (Axum / Tantivy server binary)
- `crates/gpui_telegram_test_harness` (Headless desktop shell test harness)
- `crates/gpui_telegram_webrtc` (Desktop WebRTC C/audio stack)
- `crates/fabushi_miniapp_wasm` (Browser WebAssembly target)
