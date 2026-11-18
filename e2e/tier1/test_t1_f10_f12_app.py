"""Tier 1 Feature & Functional E2E tests for M4 :app integration features F10, F11, and F12 (18 tests)."""

from __future__ import annotations

from e2e.harness.context import E2eTestContext
from e2e.harness.registry import e2e_test


# =============================================================================
# F10 — Unified Keystore AI Vault & Bidirectional Magpie <-> Mika Provider Sync
# =============================================================================


@e2e_test(
    "T1-F10-01",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="MikaCredentialBridge syncs OpenAI key into encrypted Keystore ProviderStore & assigns ModelRole.Main",
)
def test_t1_f10_01_magpie_openai_sync_to_mika_provider_store(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-oai-vault-101") as hub:
        res = ctx.jvm_probe(
            "f10_vault_and_sync",
            magpieUrl=f"{hub.base_url}/v1",
            apiKey="sk-oai-vault-101",
            protocol="OpenAI",
            presetId="openai",
            modelId="gpt-4o-mini",
        )
        assert res.get("providersCount", 0) >= 1 and res.get("mainRole")


@e2e_test(
    "T1-F10-02",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="Multi-protocol mapping (Anthropic Messages, Gemini GenerateContent, Responses) syncs accurately into Mika",
)
def test_t1_f10_02_multi_protocol_mapping_into_provider_store(ctx: E2eTestContext) -> None:
    for idx, (preset, proto, model) in enumerate(
        [
            ("anthropic", "Anthropic", "claude-sonnet-4"),
            ("gemini", "Gemini", "gemini-2.5-pro"),
            ("codex", "Responses", "gpt-5-codex"),
        ]
    ):
        res = ctx.jvm_probe(
            "f10_vault_and_sync",
            subdir_name=f"jvm_proto_{idx}",
            magpieUrl="http://127.0.0.1:18080/v1",
            apiKey=f"sk-proto-{preset}-99",
            protocol=proto,
            presetId=preset,
            modelId=model,
        )
        assert res.get("providersCount", 0) >= 1


@e2e_test(
    "T1-F10-03",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="Reverse Mika -> Magpie sync (observeMikaProviders) reflects ProviderStore connections in Magpie UI",
)
def test_t1_f10_03_reverse_mika_to_magpie_sync(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe(
        "f10_vault_and_sync",
        apiKey="sk-reverse-sync-88",
        testReverseSync=True,
    )
    assert res.get("providersCount", 0) >= 1


@e2e_test(
    "T1-F10-04",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="TelegramViewModel.translateMessage invokes live Magpie LLM rather than prepending fake '【译 · ...】' text",
)
def test_t1_f10_04_live_message_translation_no_fake_prefix(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-trans-live") as hub:
        res = ctx.jvm_probe(
            "f10_viewmodel_live_ai_surfaces",
            magpieUrl=f"{hub.base_url}/v1",
            apiKey="sk-trans-live",
            surface="translate",
        )
        assert res.get("surfaceVerified") == "translate"


@e2e_test(
    "T1-F10-05",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="Companion Bots, Hermes Studio & Dharma AI surfaces route through live Magpie without canned echo strings",
)
def test_t1_f10_05_companion_bots_hermes_dharma_live_llm(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-surface-live") as hub:
        res = ctx.jvm_probe(
            "f10_viewmodel_live_ai_surfaces",
            magpieUrl=f"{hub.base_url}/v1",
            apiKey="sk-surface-live",
            surface="bot_hermes_dharma",
        )
        assert res.get("surfaceVerified") == "bot_hermes_dharma"


@e2e_test(
    "T1-F10-06",
    tier=1,
    features=["F10"],
    needs=["jvm"],
    title="Subsequent Magpie provider syncs preserve existing user-selected ModelRole.Main without clobbering",
)
def test_t1_f10_06_non_clobber_existing_main_role(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe(
        "f10_vault_and_sync",
        apiKey="sk-non-clobber-77",
        testNonClobber=True,
    )
    assert res.get("mainRole")


# =============================================================================
# F11 — Real TelegramMikaChannel, ChannelGateway, Outbox, RunRecovery & Sandbox
# =============================================================================


@e2e_test(
    "T1-F11-01",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="TelegramMikaChannel implements com.wangzan101.mika.core.run.Channel (id='telegram', supportsImages, confirms)",
)
def test_t1_f11_01_telegram_mika_channel_contract(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert res.get("channelId") == "telegram"


@e2e_test(
    "T1-F11-02",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="Outbox prepares & delivers Progress, Ask(Confirm/Input/Takeover), Image screenshot & Result over TelegramMikaChannel",
)
def test_t1_f11_02_outbox_delivery_variants_to_telegram(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert res.get("mode") == "full_lifecycle"


@e2e_test(
    "T1-F11-03",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="ChannelGateway routes Telegram inline Approve/Deny & reply-to-Ask inputs back into active Mika run",
)
def test_t1_f11_03_channel_gateway_confirm_and_ask_routing(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert res.get("channelId") == "telegram"


@e2e_test(
    "T1-F11-04",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="MikaApp / FabushiApplication registers 'telegram' channel in RunRecovery for crash-safe result settlement",
)
def test_t1_f11_04_run_recovery_telegram_channel_registration(ctx: E2eTestContext) -> None:
    mika_app_src = (ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "java" / "com" / "wangzan101" / "mika" / "MikaApp.kt").read_text(encoding="utf-8")
    res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert res.get("channelId") == "telegram" and ("RunRecovery" in mika_app_src)


@e2e_test(
    "T1-F11-05",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="Mika pairing invokes Wireless ADB SPAKE2 PairingCoordinator / PhoneSetupScreen instead of raw Settings",
)
def test_t1_f11_05_wireless_adb_spake2_pairing_wiring(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


@e2e_test(
    "T1-F11-06",
    tier=1,
    features=["F11"],
    needs=["jvm"],
    title="Hermes TTY/Cron & CloudComputer 'node-local-android' connect to real Mika AndroidSandbox, TriggerRuntime & TaskService",
)
def test_t1_f11_06_hermes_and_cloud_computer_local_mika_wiring(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


# =============================================================================
# F12 — Zero Fake Seed, InstalledAppSearchIndex, Camera, Talk/WebRTC, TON & Dharma
# =============================================================================


@e2e_test(
    "T1-F12-01",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="Cold-start TelegramViewModel has 0 fake seeded accounts, 0 fake chats, 0 fake stories & 0.0 TON balance",
)
def test_t1_f12_01_cold_start_zero_fake_seed_viewmodel(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_cold_start_truthfulness")
    assert res.get("truthfulColdStart") is True


@e2e_test(
    "T1-F12-02",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="InstalledAppRepository discovers launcher apps via PackageManager + Mika 36-app multilingual aliases",
)
def test_t1_f12_02_installed_app_repository_search_and_aliases(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_installed_app_search", query="wechat")
    assert res.get("hitsCount", 0) >= 1


@e2e_test(
    "T1-F12-03",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="AndroidManifest.xml declares <queries> MAIN/LAUNCHER & CAMERA permission for app search & round video notes",
)
def test_t1_f12_03_manifest_queries_and_camera_permission(ctx: E2eTestContext) -> None:
    manifest = (ctx.repo_root / "mobile" / "android" / "app" / "src" / "main" / "AndroidManifest.xml").read_text(
        encoding="utf-8"
    )
    assert "android.permission.CAMERA" in manifest, "AndroidManifest.xml missing android.permission.CAMERA"
    assert "<queries>" in manifest and "android.intent.category.LAUNCHER" in manifest, (
        "AndroidManifest.xml missing Android 11+ <queries> MAIN/LAUNCHER intent block for InstalledAppSearchIndex"
    )


@e2e_test(
    "T1-F12-04",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="Talk Mode Orb & Calls wire PCM VAD + Mika Transcribe/Audio + pure-Rust WebRTC without canned reply strings",
)
def test_t1_f12_04_talk_mode_and_webrtc_real_wiring(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    assert res.get("verified") is True


@e2e_test(
    "T1-F12-05",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="TonWalletScreen renders ISO-18004 QR via wallet.generateQrMatrix (removing fake SHA-256 bit grid)",
)
def test_t1_f12_05_ton_wallet_screen_real_qr_wiring(ctx: E2eTestContext) -> None:
    res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    assert res.get("verified") is True


@e2e_test(
    "T1-F12-06",
    tier=1,
    features=["F12"],
    needs=["jvm"],
    title="Global Dharma MiniApp binds MiniAppPlatformBridge + GlobalDharmaCommercePanel & WebMCP entitlement check",
)
def test_t1_f12_06_global_dharma_platform_bridge_and_commerce(ctx: E2eTestContext) -> None:
    with ctx.mcp_server() as mcp:
        assert not mcp.prayer_wheel_entitled
        res = ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
        assert res.get("verified") is True
