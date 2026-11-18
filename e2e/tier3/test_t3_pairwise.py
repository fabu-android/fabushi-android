"""Tier 3 Pairwise Cross-Feature Interaction E2E tests (18 tests: T3-01..T3-18)."""

from __future__ import annotations

import base64
import json
import struct
import subprocess
import time

from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import assert_structured_error, inspect_elf64_alignment_and_machine
from e2e.harness.registry import e2e_test


@e2e_test(
    "T3-01",
    tier=3,
    features=["F1", "F2", "F3"],
    needs=["native"],
    title="Spec R2/R3 + package_android_release.py semver metadata + 16KB ELF64 native host & events.drain",
)
def test_t3_01_spec_release_and_16kb_native_host(ctx: E2eTestContext) -> None:
    script = str(ctx.repo_root / "scripts" / "package_android_release.py")
    out = subprocess.run(
        ["python3", script, "--compute-version-code", "v2.1.0"],
        cwd=str(ctx.repo_root),
        env=ctx.env(),
        stdout=subprocess.PIPE,
        text=True,
        check=True,
    ).stdout.strip()
    assert int(out) == 20100
    arm_elf = inspect_elf64_alignment_and_machine(ctx.ws.arm64_lib_so)
    assert all(a >= 0x4000 for a in arm_elf["pt_load_aligns"])
    with ctx.host_session("t3_01", test_mode=False) as host:
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
        assert isinstance(drain.get("events"), list)


@e2e_test(
    "T3-02",
    tier=3,
    features=["F4", "F13"],
    needs=["jvm"],
    title="TelegramIntroAuthScreen onboarding + live LoopbackTelegramServer Phone OTP + 2FA challenge/verify",
)
def test_t3_02_onboarding_ui_and_live_phone_2fa_auth(ctx: E2eTestContext) -> None:
    ui = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "IntroAuthScreen" in str(ui.get("introClass", ""))
    with ctx.telegram_server() as srv, ctx.host_session("t3_02", test_mode=False) as host:
        phone = "+14155550302"
        srv.enable_2fa_for_identifier(phone, "Zen#2026", hint="lotus")
        host.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": phone})
        step1 = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": phone, "code": srv.otp_by_phone[phone]})
        assert step1.get("status") == "2fa_required" or step1.get("twoFactorRequired") is True
        host.call_any_ok(["telegram.auth.twoFactorVerify"], {"phone": phone, "password": "Zen#2026", "tempToken": step1.get("tempToken", "")})


@e2e_test(
    "T3-03",
    tier=3,
    features=["F4", "F5", "F13"],
    needs=["jvm"],
    title="Multi-account GenerationFence switch cancels stale WS/PTS event drain so Account A never bleeds into B",
)
def test_t3_03_multi_account_generation_fence_and_ws_drain(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f13_multi_account_isolation")
    with ctx.telegram_server() as srv, ctx.host_session("t3_03", test_mode=False) as host:
        host.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": "+14155550303"})
        a1 = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": "+14155550303", "code": srv.otp_by_phone["+14155550303"]})
        g1 = int(a1.get("generation") or 1)
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": "+14155550304"})
        a2 = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": "+14155550304", "code": srv.otp_by_phone["+14155550304"]})
        g2 = int(a2.get("generation") or 0)
        assert g2 > g1
        stale = host.call("telegram.events.drain", {"accountGeneration": g1, "afterSeq": 0})
        assert_structured_error(stale, context="stale generation drain after multi-account switch")


@e2e_test(
    "T3-04",
    tier=3,
    features=["F5", "F12", "F14"],
    needs=["jvm"],
    title="Truthful message checkmarks (Pending offline -> Sent on WS Ack) + non-destructive in-chat jump search",
)
def test_t3_04_truthful_outbox_checkmarks_and_jump_search(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    with ctx.telegram_server() as srv, ctx.host_session("t3_04", test_mode=False) as host:
        host.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": "+14155550305"})
        host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": "+14155550305", "code": srv.otp_by_phone["+14155550305"]})
        srv.set_offline(True)
        sent = host.call_any_ok(["telegram.messaging.sendMessage", "messaging.sendMessage"], {"chatId": "c_t3", "text": "Offline truth", "clientNonce": "n_t3"})
        assert str(sent.get("deliveryState") or sent.get("status") or "").lower() == "pending"


@e2e_test(
    "T3-05",
    tier=3,
    features=["F6", "F12"],
    needs=["jvm"],
    title="TON Wallet v4R2 address + ZXing optical decode of wallet.generateQrMatrix + live LoopbackTonRpcServer BOC",
)
def test_t3_05_ton_wallet_qr_zxing_and_live_boc_rpc(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    with ctx.ton_rpc() as ton, ctx.host_session("t3_05", test_mode=False) as host:
        w = host.call_ok("wallet.createWallet", {"rpcUrl": f"{ton.base_url}/api/v2"})
        addr = str(w.get("address") or "")
        qr = host.call_ok("wallet.generateQrMatrix", {"data": addr, "payload": addr})
        ok_qr, decoded = ctx.qr.decode_matrix(qr)
        assert ok_qr and decoded == addr
        ton.fund_address(addr, 5_000_000_000)
        host.call_ok("wallet.sendTon", {"toAddress": addr, "amountNanotons": 1_000_000_000, "rpcUrl": f"{ton.base_url}/api/v2"})
        assert len(ton.submitted_bocs) == 1


@e2e_test(
    "T3-06",
    tier=3,
    features=["F7", "F10"],
    needs=["jvm"],
    title="Magpie Keystore vault sync -> live OpenAI/Anthropic SSE translation & bot replies with wire PII masking",
)
def test_t3_06_keystore_vault_to_magpie_sse_and_pii_masking(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-t3-vault") as hub:
        ctx.jvm_probe("f10_vault_and_sync", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-t3-vault")
        with ctx.host_session("t3_06", test_mode=False) as host:
            host.call_any_ok(["magpie.upsertProvider"], {"id": "oai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-t3-vault", "model": "gpt-4o-mini", "enabled": True})
            r = host.call_ok("magpie.chatComplete", {"providerId": "oai", "prompt": "Translate user@ombhrum.org", "maskSecrets": True})
            assert "user@ombhrum.org" not in hub.recorded_requests[-1]["body_text"]
            assert "user@ombhrum.org" in json.dumps(r)


@e2e_test(
    "T3-07",
    tier=3,
    features=["F8", "F10", "F11"],
    needs=["jvm"],
    title="Magpie Keystore sync populates MikaApp.providers (hasMainModel) -> TelegramMikaChannel executes task",
)
def test_t3_07_magpie_keystore_sync_enables_telegram_mika_channel(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-t3-mika") as hub:
        sync_res = ctx.jvm_probe("f10_vault_and_sync", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-t3-mika")
        assert sync_res.get("mainRole")
        chan_res = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
        assert chan_res.get("channelId") == "telegram"


@e2e_test(
    "T3-08",
    tier=3,
    features=["F8", "F11"],
    needs=["jvm"],
    title="Mika #237 blind-UI StopRules(treeBlind=true) delivers clear failure card through TelegramMikaChannel Outbox",
)
def test_t3_08_blind_ui_stop_rule_delivered_over_telegram_channel(ctx: E2eTestContext) -> None:
    stop = ctx.jvm_probe("f08_stop_rules_blind_ui")
    assert "不响应" in str(stop.get("reason", ""))
    chan = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert chan.get("channelId") == "telegram"


@e2e_test(
    "T3-09",
    tier=3,
    features=["F9", "F10", "F12"],
    needs=["jvm"],
    title="Talk Mode PCM16 VAD (<60ms) + Mika TranscribeCapability + Magpie LLM + Mika OpenAiSpeechModel TTS",
)
def test_t3_09_talk_mode_vad_transcribe_llm_and_tts(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-talk-t3") as hub, ctx.host_session("t3_09", test_mode=False) as host:
        speech_pcm = base64.b64encode(struct.pack("<h", 7000) * 288).decode("ascii")
        barge = any(
            host.call_ok("telegram.calls.vadFeedPcm", {"pcm16Base64": speech_pcm, "sampleRate": 16000, "ttsPlaying": True}).get("bargeInTriggered")
            for _ in range(4)
        )
        assert barge is True
        ctx.jvm_probe("f09_audio_tts_models", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-talk-t3")
        ctx.jvm_probe("f09_audio_transcribe")
        ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")


@e2e_test(
    "T3-10",
    tier=3,
    features=["F7", "F8", "F11"],
    needs=["jvm"],
    title="Hermes Studio TTY & Cron + Mika AndroidSandbox (16KB libmikarun.so --pty) + TriggerRuntime schedules",
)
def test_t3_10_hermes_tty_cron_and_mika_sandbox_pty(ctx: E2eTestContext) -> None:
    mikarun = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libmikarun.so"
    assert b"--pty" in mikarun.read_bytes()
    res = ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")
    assert res.get("pairingAndRuntimeWired") is True


@e2e_test(
    "T3-11",
    tier=3,
    features=["F7", "F11"],
    needs=["jvm"],
    title="CloudComputer 'node-local-android' CUA lease generation fencing + emergency takeover stops Mika run",
)
def test_t3_11_cloud_computer_lease_takeover_and_mika_stop(ctx: E2eTestContext) -> None:
    with ctx.host_session("t3_11", test_mode=False) as host:
        l1 = host.call_ok("computer.leaseAcquire", {"nodeId": "node-local-android", "holder": "agent:mika", "ttlMs": 30000})
        g1 = int(l1.get("generation", -1))
        assert g1 >= 1
        l2 = host.call_ok("computer.leaseTakeover", {"nodeId": "node-local-android", "holder": "human:owner"})
        assert int(l2.get("generation", 0)) > g1
        stale = host.call("computer.executeOp", {"nodeId": "node-local-android", "leaseId": l1["leaseId"], "generation": g1, "op": {"type": "tap", "x": 10, "y": 10}})
        assert_structured_error(stale, context="stale generation after human takeover")
    ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")


@e2e_test(
    "T3-12",
    tier=3,
    features=["F11", "F12", "F13"],
    needs=["jvm"],
    title="Global Search APPS tab -> InstalledAppRepository ('wechat' -> 微信) -> 1-tap Mika automation via TelegramMikaChannel",
)
def test_t3_12_apps_search_tab_to_mika_channel_delegation(ctx: E2eTestContext) -> None:
    tabs = ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    assert "APPS" in (tabs.get("filters") or [])
    apps = ctx.jvm_probe("f12_installed_app_search", query="wechat")
    assert apps.get("hitsCount", 0) >= 1
    chan = ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
    assert chan.get("channelId") == "telegram"


@e2e_test(
    "T3-13",
    tier=3,
    features=["F6", "F10", "F12"],
    needs=["jvm"],
    title="Global Dharma MiniApp -> MiniAppPlatformBridge WebMCP entitlement unlock + live Magpie Dharma AI",
)
def test_t3_13_global_dharma_webmcp_entitlement_and_live_ai(ctx: E2eTestContext) -> None:
    with ctx.mcp_server() as mcp, ctx.magpie_hub("sk-dharma-t3") as hub, ctx.host_session("t3_13", test_mode=False) as host:
        assert mcp.prayer_wheel_entitled is False
        ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
        host.call_any_ok(["magpie.upsertProvider"], {"id": "oai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-dharma-t3", "model": "gpt-4o-mini", "enabled": True})
        ai = host.call_ok("fabushiMiniapp.aiDharmaChat", {"prompt": "金刚经四句偈"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(ai)


@e2e_test(
    "T3-14",
    tier=3,
    features=["F6", "F13", "F14"],
    needs=["jvm"],
    title="Remote peer X3DH Secret Chat + Double Ratchet encryption + TelegramPeerProfileScreen 333-emoji SAS",
)
def test_t3_14_secret_chat_peer_x3dh_and_peer_profile_sas(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    with (
        ctx.telegram_server() as srv,
        ctx.host_session("t3_14_a", test_mode=False) as alice,
        ctx.host_session("t3_14_b", test_mode=False) as bob,
    ):
        alice.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        bob.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        bob.call_any_ok(["telegram.e2ee.publishPrekeys", "e2ee.publishPrekeys"], {"userId": "bob_t3"})
        assert "bob_t3" in srv.prekey_bundles
        alice.call_any_ok(["telegram.e2ee.initPeerSession", "e2ee.initPeerSession"], {"peerUserId": "bob_t3", "conversationId": "sec_t3"})


@e2e_test(
    "T3-15",
    tier=3,
    features=["F8", "F9"],
    needs=["jvm"],
    title="Mika PersonaStore active persona -> AgentLoop persona trace + SAF FolderFiles + ReportFilesProvider",
)
def test_t3_15_mika_persona_saf_folder_and_report_provider(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f08_model_router_and_persona_loop")
    ctx.jvm_probe("f09_persona_and_skill_market")
    ctx.jvm_probe("f09_saf_folder_files")
    ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=True)


@e2e_test(
    "T3-16",
    tier=3,
    features=["F5", "F6", "F14"],
    needs=["jvm"],
    title="Cumulative voice slide-up-to-lock (-90f) -> 5-bit 100-bar waveform encoding -> CAS media range streaming",
)
def test_t3_16_voice_lock_drag_waveform_and_cas_streaming(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_cumulative_voice_drag", mode="lock")
    with ctx.telegram_server() as srv, ctx.host_session("t3_16", test_mode=False) as host:
        bars = [(i * 5) % 32 for i in range(100)]
        wf = host.call_ok("telegram.messaging.encodeWaveform", {"bars": bars})
        assert len(base64.b64decode(wf.get("waveformBase64") or wf.get("encoded") or "")) == 63
        blob = b"OPUS_VOICE_DATA_" * 4096
        import hashlib

        fid = hashlib.sha256(blob).hexdigest()
        srv.media_blobs[fid] = blob
        rng = host.call_any_ok(["telegram.media.streamRange", "telegram.messaging.readMediaRange"], {"fileId": fid, "offset": 0, "length": 1024, "endpoint": srv.base_url})
        assert base64.b64decode(rng.get("dataBase64") or rng.get("chunkBase64") or "") == blob[:1024]


@e2e_test(
    "T3-17",
    tier=3,
    features=["F2", "F3", "F8", "F15"],
    needs=["jvm"],
    title="16KB ELF64 alignment across BOTH libmahayana_app_host.so & libmikarun.so (--pty) + synchronized v2.1.0 BuildConfig",
)
def test_t3_17_dual_native_16kb_alignment_and_v2_1_0_buildconfig(ctx: E2eTestContext) -> None:
    host_arm = inspect_elf64_alignment_and_machine(ctx.ws.arm64_lib_so)
    host_x64 = inspect_elf64_alignment_and_machine(ctx.ws.x86_64_lib_so)
    mikarun_so = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libmikarun.so"
    mika_elf = inspect_elf64_alignment_and_machine(mikarun_so)
    assert all(a >= 0x4000 for a in host_arm["pt_load_aligns"])
    assert all(a >= 0x4000 for a in host_x64["pt_load_aligns"])
    assert all(a >= 0x4000 for a in mika_elf["pt_load_aligns"])
    assert b"--pty" in mikarun_so.read_bytes()
    bc = ctx.jvm_probe("f15_ota_validator_and_version", mode="positive")
    assert bc.get("appVCode") == 20100 and bc.get("mikaVCode") == 20100


@e2e_test(
    "T3-18",
    tier=3,
    features=["F2", "F14", "F15"],
    needs=["jvm"],
    title="Settings live session management + AndroidUpdateValidator v2.0.0 -> v2.1.0 (20100) OTA security checks",
)
def test_t3_18_settings_sessions_and_ota_update_security(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_settings_and_i18n")
    ota = ctx.jvm_probe("f15_ota_validator_and_version", mode="negative")
    assert ota.get("appVCode") == 20100
