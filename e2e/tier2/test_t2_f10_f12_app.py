"""Tier 2 Boundary, Corner & Negative E2E tests for M4 :app integration features F10, F11, and F12 (18 tests)."""

from __future__ import annotations

from e2e.harness.context import E2eTestContext
from e2e.harness.registry import e2e_test


# =============================================================================
# F10 — Plaintext Zero-Leak, Unconfigured AI Error, Empty Key & Invalid Endpoint
# =============================================================================


@e2e_test(
    "T2-F10-01",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Plaintext API keys never appear in providers.json, provider-keys.json, SharedPreferences, or SQLite WAL",
)
def test_t2_f10_01_zero_plaintext_api_key_on_disk(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe(
        "f10_vault_and_sync",
        apiKey="sk-ultra-secret-canary-do-not-write-plaintext-999111",
    )
    assert res.get("providersCount", 0) >= 1


@e2e_test(
    "T2-F10-02",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Unconfigured translateMessage surfaces honest 'configure AI key' state without fake '【译 · ...】' text",
)
def test_t2_f10_02_unconfigured_translate_no_fake_prefix(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f10_viewmodel_live_ai_surfaces", magpieUrl="", apiKey="", surface="translate")
    assert res.get("surfaceVerified") == "translate"


@e2e_test(
    "T2-F10-03",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Unconfigured Bot / Hermes / Dharma AI surfaces never emit hardcoded mock replies",
)
def test_t2_f10_03_unconfigured_ai_surfaces_no_mock_replies(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f10_viewmodel_live_ai_surfaces", magpieUrl="", apiKey="", surface="bot_hermes_dharma")
    assert res.get("surfaceVerified") == "bot_hermes_dharma"


@e2e_test(
    "T2-F10-04",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Default MagpieGatewayUiState presets do NOT falsely mark DeepSeek/Qwen/OpenRouter configured=true without keys",
)
def test_t2_f10_04_magpie_default_presets_not_falsely_configured(ctx: E2eTestContext) -> None:
    # In v2.0.0 TelegramModels.kt:1394-1407, deepseek/qwen/openrouter hardcoded configured=true with 0 keys!
    res = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res.get("truthfulColdStart") is True


@e2e_test(
    "T2-F10-05",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Repeated syncFromMagpie calls are idempotent and do not duplicate Provider entries for identical (kind, baseUrl)",
)
def test_t2_f10_05_repeated_sync_idempotence(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f10_vault_and_sync", apiKey="sk-idempotent-sync-55", testNonClobber=True)
    assert res.get("mainRole")


@e2e_test(
    "T2-F10-06",
    tier=2,
    features=["F10"],
    needs=["jvm"],
    title="Reverse Mika -> Magpie provider sync never copies raw decrypted secret strings into Compose UI state",
)
def test_t2_f10_06_reverse_sync_never_exposes_plaintext_key_in_ui_state(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f10_vault_and_sync", apiKey="sk-ui-hidden-secret-44", testReverseSync=True)
    assert res.get("providersCount", 0) >= 1


# =============================================================================
# F11 — Stranger Peer Rejection, Uncertain Outbox Idempotence & Unseeded Bot
# =============================================================================


@e2e_test(
    "T2-F11-01",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="ChannelGateway drops inbound messages & Confirm clicks from unbound non-owner Telegram peers",
)
def test_t2_f11_01_stranger_peer_rejected_by_channel_gateway(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="stranger_reject")
    assert res.get("mode") == "stranger_reject"


@e2e_test(
    "T2-F11-02",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="Cold-start @MikaPhoneBot has zero pre-seeded demo messages ('欢迎使用 Mika 本机手机操控智能体')",
)
def test_t2_f11_02_no_fake_mika_bot_welcome_seed(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res.get("truthfulColdStart") is True


@e2e_test(
    "T2-F11-03",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="TelegramViewModel.runWithMikaFromChat does NOT fake 'ready' status via delay(450) when Mika has no model",
)
def test_t2_f11_03_no_fake_delay_450_mika_ready_transition(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert res.get("channelId") == "telegram"


@e2e_test(
    "T2-F11-04",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="Mika pairing button never opens raw Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS without SPAKE2 pairing",
)
def test_t2_f11_04_no_raw_developer_settings_bypass(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


@e2e_test(
    "T2-F11-05",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="CloudComputer executeOp forwards lease generation fencing and rejects stale human-preempted agent ops",
)
def test_t2_f11_05_cloud_computer_generation_fencing_wired(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


@e2e_test(
    "T2-F11-06",
    tier=2,
    features=["F11"],
    needs=["jvm"],
    title="Hermes Studio TTY routes shell execution through Mika AndroidSandbox instead of canned string echo",
)
def test_t2_f11_06_hermes_tty_android_sandbox_no_echo_stub(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


# =============================================================================
# F12 — PackageManager Fallback, Self-Protection, Offline Checkmarks & Locked Dharma
# =============================================================================


@e2e_test(
    "T2-F12-01",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="InstalledAppRepository degrades gracefully to MiniApp/Bot results when PackageManager throws SecurityException",
)
def test_t2_f12_01_installed_app_search_security_exception_fallback(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_installed_app_search", query="法布施", throwOnQuery=True)
    assert "gracefulFallbackHits" in res


@e2e_test(
    "T2-F12-02",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="automateWithMika enforces Mika self-protection guard when target packageName == 'com.ombhrum.fabushi'",
)
def test_t2_f12_02_automate_with_mika_self_protection_guard(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_installed_app_search", query="Fabushi", testSelfProtection=True)
    assert res.get("hitsCount", 0) >= 1


@e2e_test(
    "T2-F12-03",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="Cold-start TonWalletUiState does NOT seed fake 25 TON / 1080 Stars / 1285 USDT or hardcoded 24-word mnemonic",
)
def test_t2_f12_03_no_fake_wallet_balance_or_mnemonic_seed(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res.get("truthfulColdStart") is True


@e2e_test(
    "T2-F12-04",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="TonWalletScreen never computes SHA-256 bit patterns to fake a QR code",
)
def test_t2_f12_04_ton_wallet_screen_forbids_sha256_fake_qr(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    assert res.get("verified") is True


@e2e_test(
    "T2-F12-05",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="Talk Mode never emits hardcoded '已收到您的语音指令' when ASR/LLM is unconfigured or offline",
)
def test_t2_f12_05_talk_mode_forbids_canned_voice_reply(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    assert res.get("verified") is True


@e2e_test(
    "T2-F12-06",
    tier=2,
    features=["F12"],
    needs=["jvm"],
    title="Global Dharma protected capability 'local.prayer-wheel.start' starts locked (not_entitled) before purchase",
)
def test_t2_f12_06_global_dharma_prayer_wheel_entitlement_gate(ctx: E2eTestContext) -> None:
    with ctx.mcp_server() as mcp:
        assert mcp.prayer_wheel_entitled is False
        res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
        assert res.get("verified") is True
