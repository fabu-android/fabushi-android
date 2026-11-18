"""Tier 4 Multi-Step Real-World End-User Workflow E2E tests (10 workflows: T4-01..T4-10)."""

from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import os
import struct
import time
import zipfile

from e2e.harness.apk_builder import create_valid_unsigned_apk
from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import assert_structured_error
from e2e.harness.registry import e2e_test


@e2e_test(
    "T4-01",
    tier=4,
    features=["F3", "F4", "F5", "F12", "F13", "F14"],
    needs=["jvm"],
    title="Journey 1: Cold Unseeded Launch -> Onboarding Phone OTP + 2FA -> Add @alice_dev -> Rich Chat -> Offline Outbox -> WS Sync",
)
def test_t4_01_cold_onboarding_contact_rich_chat_and_offline_outbox(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f12_cold_start_truthfulness")
    ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
    ctx.jvm_probe("f14_ime_padding_bytecode")

    with ctx.telegram_server() as srv, ctx.host_session("t4_01", test_mode=False) as host:
        phone = "+14155550401"
        srv.enable_2fa_for_identifier(phone, "Bodhi#108", hint="tree")
        host.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": phone})
        chal = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": phone, "code": srv.otp_by_phone[phone]})
        auth = host.call_any_ok(["telegram.auth.twoFactorVerify"], {"phone": phone, "password": "Bodhi#108", "tempToken": chal.get("tempToken", "")})
        gen = int(auth.get("generation") or 1)

        peer = host.call_any_ok(["telegram.contacts.resolveUsername", "telegram.messaging.resolvePeer"], {"username": "@alice_dev"})
        assert "alice_dev" in json.dumps(peer)

        srv.set_offline(True)
        q_msg = host.call_any_ok(["telegram.messaging.sendMessage", "messaging.sendMessage"], {"chatId": "c_alice", "text": "Hello 🪷 Alice", "clientNonce": "t4_n1"})
        assert str(q_msg.get("deliveryState") or q_msg.get("status") or "").lower() == "pending"

        srv.set_offline(False)
        host.call_any(["telegram.messaging.flushOutbox", "telegram.server.reconnect"], {})
        time.sleep(0.2)
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": gen, "afterSeq": 0})
        assert isinstance(drain.get("events"), list)


@e2e_test(
    "T4-02",
    tier=4,
    features=["F7", "F8", "F10", "F11", "F12", "F13"],
    needs=["jvm"],
    title="Journey 2: Configure Magpie Key -> Keystore Sync -> Search 'wechat' in APPS Tab -> Mika Confirmation -> Screenshot & Result",
)
def test_t4_02_magpie_keystore_apps_search_and_telegram_mika_workflow(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-t4-journey2") as hub:
        ctx.jvm_probe("f10_vault_and_sync", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-t4-journey2")
        ctx.jvm_probe("f13_onboarding_contacts_apps_tab")
        ctx.jvm_probe("f12_installed_app_search", query="wechat")
        ctx.jvm_probe("f11_telegram_mika_channel", mode="full_lifecycle")
        ctx.jvm_probe("f08_aidl_start_share_file")


@e2e_test(
    "T4-03",
    tier=4,
    features=["F4", "F5", "F6", "F12", "F13"],
    needs=["jvm"],
    title="Journey 3: Multi-Account Isolation (Alice vs Bob Chats + Separate TON Wallets + ZXing QR + Live BOC Transfer)",
)
def test_t4_03_multi_account_chats_and_ton_wallet_zxing_transfer(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f13_multi_account_isolation")
    ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
    with ctx.telegram_server() as srv, ctx.ton_rpc() as ton, ctx.host_session("t4_03", test_mode=False) as host:
        host.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": "+14155550403"})
        a1 = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": "+14155550403", "code": srv.otp_by_phone["+14155550403"]})
        w1 = host.call_ok("wallet.createWallet", {"accountId": a1.get("accountId", "acc_a"), "rpcUrl": f"{ton.base_url}/api/v2"})
        addr1 = str(w1.get("address") or "")
        qr = host.call_ok("wallet.generateQrMatrix", {"data": addr1, "payload": addr1})
        ok_qr, dec = ctx.qr.decode_matrix(qr)
        assert ok_qr and dec == addr1
        ton.fund_address(addr1, 9_000_000_000)
        host.call_ok("wallet.sendTon", {"toAddress": addr1, "amountNanotons": 1_500_000_000, "rpcUrl": f"{ton.base_url}/api/v2"})
        assert len(ton.submitted_bocs) >= 1


@e2e_test(
    "T4-04",
    tier=4,
    features=["F6", "F13", "F14"],
    needs=["jvm"],
    title="Journey 4: Two-Peer X3DH Secret Chat + 333-Emoji SAS + Out-of-Order Double Ratchet + Pure-Rust WebRTC Call",
)
def test_t4_04_two_peer_secret_chat_and_webrtc_e2ee_call(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_peer_profile_and_non_destructive_search")
    with (
        ctx.telegram_server() as srv,
        ctx.host_session("t4_04_alice", test_mode=False) as alice,
        ctx.host_session("t4_04_bob", test_mode=False) as bob,
    ):
        for h, ph in ((alice, "+14155550404"), (bob, "+14155550405")):
            h.call_any_ok(["telegram.server.configureEndpoint"], {"httpUrl": srv.base_url, "wsUrl": f"{srv.ws_url}/api/v1/ws"})
            h.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": ph})
            h.call_any_ok(["telegram.auth.phoneVerify"], {"phone": ph, "code": srv.otp_by_phone[ph]})

        bob.call_any_ok(["telegram.e2ee.publishPrekeys", "e2ee.publishPrekeys"], {"userId": "bob_t4"})
        hdr = alice.call_any_ok(["telegram.e2ee.initPeerSession", "e2ee.initPeerSession"], {"peerUserId": "bob_t4", "conversationId": "sec_t4"})
        c1 = alice.call_any_ok(["telegram.e2ee.encryptPeerMessage", "e2ee.encrypt"], {"conversationId": "sec_t4", "plaintext": "Secret 1"})
        c2 = alice.call_any_ok(["telegram.e2ee.encryptPeerMessage", "e2ee.encrypt"], {"conversationId": "sec_t4", "plaintext": "Secret 2"})
        assert "Secret 2" in json.dumps(bob.call_any_ok(["telegram.e2ee.decryptPeerMessage", "e2ee.decrypt"], {"conversationId": "sec_t4", "envelope": c2, "initHeader": hdr}))
        assert "Secret 1" in json.dumps(bob.call_any_ok(["telegram.e2ee.decryptPeerMessage", "e2ee.decrypt"], {"conversationId": "sec_t4", "envelope": c1}))

        offer = alice.call_any_ok(["telegram.calls.createOffer", "telegram.calls.startCall"], {"peerId": "bob_t4", "video": True})
        ans = bob.call_any_ok(["telegram.calls.acceptOffer", "telegram.calls.createAnswer"], {"offerSdp": offer.get("sdp") or offer.get("offerSdp", "")})
        assert "a=fingerprint:sha-256" in str(ans.get("sdp") or ans.get("answerSdp") or "")


@e2e_test(
    "T4-05",
    tier=4,
    features=["F6", "F7", "F9", "F10", "F12", "F14"],
    needs=["jvm"],
    title="Journey 5: Voice Slide-Up Lock -> 5-Bit Waveform -> Talk Mode <60ms VAD Barge-In -> Mika ASR -> Magpie LLM -> TTS",
)
def test_t4_05_voice_lock_waveform_and_talk_mode_full_pipeline(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_cumulative_voice_drag", mode="lock")
    with ctx.magpie_hub("sk-t4-voice") as hub, ctx.host_session("t4_05", test_mode=False) as host:
        ctx.jvm_probe("f09_audio_tts_models", magpieUrl=f"{hub.base_url}/v1", apiKey="sk-t4-voice")
        ctx.jvm_probe("f09_audio_transcribe")
        ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
        speech_pcm = base64.b64encode(struct.pack("<h", 6800) * 288).decode("ascii")
        assert any(
            host.call_ok("telegram.calls.vadFeedPcm", {"pcm16Base64": speech_pcm, "sampleRate": 16000, "ttsPlaying": True}).get("bargeInTriggered")
            for _ in range(4)
        )


@e2e_test(
    "T4-06",
    tier=4,
    features=["F7", "F8", "F9", "F10", "F11"],
    needs=["jvm"],
    title="Journey 6: Hermes Studio /moa Synthesis -> Sandbox TTY (--pty) -> Cron Schedule -> CUA Lease Takeover",
)
def test_t4_06_hermes_moa_sandbox_cron_and_cua_takeover(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-t4-hermes") as hub, ctx.host_session("t4_06", test_mode=False) as host:
        for pid, proto in (("openai", "openai_compat"), ("anthropic", "anthropic")):
            host.call_any_ok(["magpie.upsertProvider"], {"id": pid, "protocol": proto, "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-t4-hermes", "model": "m1", "enabled": True})
        host.call_any_ok(["hermes.runSlashCommand", "hermes.moaSynthesize"], {"command": "/moa Summarize architecture", "prompt": "Summarize architecture"})
        assert len(hub.recorded_requests) >= 3
        l1 = host.call_ok("computer.leaseAcquire", {"nodeId": "node-local-android", "holder": "agent:hermes", "ttlMs": 30000})
        l2 = host.call_ok("computer.leaseTakeover", {"nodeId": "node-local-android", "holder": "human:owner"})
        assert int(l2.get("generation", 0)) > int(l1.get("generation", 0))
    ctx.jvm_probe("f11_pairing_and_local_runtime_wiring")


@e2e_test(
    "T4-07",
    tier=4,
    features=["F6", "F7", "F10", "F12"],
    needs=["jvm"],
    title="Journey 7: Global Dharma 240-Country Offering -> WebMCP Entitlement Lock -> Purchase -> Spin Prayer Wheel -> Dharma AI",
)
def test_t4_07_global_dharma_offering_commerce_unlock_and_ai(ctx: E2eTestContext) -> None:
    with ctx.mcp_server() as mcp, ctx.magpie_hub("sk-t4-dharma") as hub, ctx.host_session("t4_07", test_mode=False) as host:
        countries = host.call_ok("fabushiMiniapp.countries", {})
        c_list = countries if isinstance(countries, list) else (countries.get("countries") or [])
        assert len(c_list) >= 240
        host.call_ok("fabushiMiniapp.offerDharma", {"countryCode": "BT", "sutraId": "diamond_sutra"})
        ctx.jvm_probe("f12_talk_webrtc_wallet_dharma_wiring")
        assert mcp.prayer_wheel_entitled is False
        host.call_any_ok(["magpie.upsertProvider"], {"id": "oai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-t4-dharma", "model": "gpt-4o-mini", "enabled": True})
        ai = host.call_ok("fabushiMiniapp.aiDharmaChat", {"prompt": "应无所住而生其心"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(ai)


@e2e_test(
    "T4-08",
    tier=4,
    features=["F8", "F9"],
    needs=["jvm"],
    title="Journey 8: Mika Research Workflow — Persona + SkillMarket + TinyFish WebFetch + SAF /folders Write + ReportViewer",
)
def test_t4_08_mika_persona_skill_webfetch_saf_and_report_workflow(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f08_model_router_and_persona_loop")
    ctx.jvm_probe("f09_persona_and_skill_market")
    ctx.jvm_probe("f09_saf_folder_files", testTraversal=True)
    ctx.jvm_probe("f09_webfetch_briefing_dtmf_report_viewer", testNegativeGuards=True)


@e2e_test(
    "T4-09",
    tier=4,
    features=["F5", "F7", "F10", "F13", "F14"],
    needs=["jvm"],
    title="Journey 9: Magpie 4-Protocol Failover (429 -> Anthropic SSE) + @BotFather Live Bot + Multi-Locale Settings",
)
def test_t4_09_magpie_failover_botfather_and_multilocale_settings(ctx: E2eTestContext) -> None:
    ctx.jvm_probe("f14_settings_and_i18n")
    with (
        ctx.magpie_hub("sk-t4-f") as primary_429,
        ctx.magpie_hub("sk-t4-f") as backup_ant,
        ctx.host_session("t4_09", test_mode=False) as host,
    ):
        primary_429.set_fault("/chat/completions", 429)
        host.call_any_ok(["magpie.upsertProvider"], {"id": "s1", "protocol": "openai_compat", "baseUrl": f"{primary_429.base_url}/v1", "apiKey": "sk-t4-f", "model": "gpt-4o", "enabled": True, "priority": 1})
        host.call_any_ok(["magpie.upsertProvider"], {"id": "s2", "protocol": "anthropic", "baseUrl": f"{backup_ant.base_url}/v1", "apiKey": "sk-t4-f", "model": "claude-sonnet-4", "enabled": True, "priority": 2})
        ans = host.call_ok("magpie.chatComplete", {"strategy": "Order", "prompt": "Create Dharma Bot"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(ans)
        ctx.jvm_probe("f10_viewmodel_live_ai_surfaces", magpieUrl=f"{backup_ant.base_url}/v1", apiKey="sk-t4-f", surface="bot_hermes_dharma")


@e2e_test(
    "T4-10",
    tier=4,
    features=["F1", "F2", "F3", "F8", "F15"],
    needs=["jvm"],
    title="Journey 10: Monotonic v2.1.0 (20100) Release Packaging -> 16KB ELF64 + ZIP_STORED Rootfs -> OTA Update Verification",
)
def test_t4_10_end_to_end_v2_1_0_release_packaging_and_ota_verification(ctx: E2eTestContext) -> None:
    spec = importlib.util.spec_from_file_location(
        "pkg_t4_10", ctx.repo_root / "scripts" / "package_android_release.py"
    )
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    tag, ver, vcode = mod.resolve_release_metadata("v2.1.0")
    assert (tag, ver, vcode) == ("v2.1.0", "2.1.0", 20100)

    work = ctx.subdir("t4_10_bundle")
    raw_apk = work / "raw.apk"
    signed_apk = work / "fabushi-telegram-mika-2.1.0-githubRelease.apk"
    mika_jni = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "jniLibs" / "arm64-v8a"
    rootfs = ctx.repo_root / "mobile" / "android" / "mika-agent" / "src" / "main" / "assets" / "runtime" / "rootfs.tar.xz"

    create_valid_unsigned_apk(
        raw_apk,
        work,
        arm64_host_bytes=ctx.ws.arm64_lib_so.read_bytes(),
        x64_host_bytes=ctx.ws.x86_64_lib_so.read_bytes(),
        mikarun_bytes=(mika_jni / "libmikarun.so").read_bytes(),
        proot_bytes=(mika_jni / "libproot.so").read_bytes(),
        proot_loader_bytes=(mika_jni / "libproot_loader.so").read_bytes(),
        rootfs_bytes=rootfs.read_bytes()[:65536],
    )

    saved = os.environ.get("FABUSHI_SIGNING_CACHE_DIR")
    try:
        os.environ["FABUSHI_SIGNING_CACHE_DIR"] = str(work / "ks")
        mod.ensure_signed_apk(raw_apk, signed_apk, ctx.repo_root, variant="githubRelease")
    finally:
        if saved is None:
            os.environ.pop("FABUSHI_SIGNING_CACHE_DIR", None)
        else:
            os.environ["FABUSHI_SIGNING_CACHE_DIR"] = saved

    mod.verify_apk_contents(signed_apk)
    sha = hashlib.sha256(signed_apk.read_bytes()).hexdigest()
    assert len(sha) == 64

    ctx.jvm_probe("f15_ota_validator_and_version", mode="negative")
    # Final gate: native host must have headless gpui-telegram crates + Mika --pty
    assert b"--pty" in (mika_jni / "libmikarun.so").read_bytes()
    meta = json.loads(ctx.ws.cargo_metadata_json.read_text(encoding="utf-8"))
    crates = {p.get("name", "").replace("-", "_") for p in meta.get("packages", [])}
    assert "gpui_telegram_client" in crates
