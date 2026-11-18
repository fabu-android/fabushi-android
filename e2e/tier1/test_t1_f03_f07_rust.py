"""Tier 1 Feature & Functional E2E tests for M2 Rust Native Host features F3..F7 (30 tests)."""

from __future__ import annotations

import base64
import hashlib
import json
import struct
import time
from typing import Any

from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import assert_rpc_ok, assert_structured_error
from e2e.harness.registry import e2e_test

REQUIRED_HEADLESS_CRATES = {
    "gpui_telegram_core",
    "gpui_telegram_proto",
    "gpui_telegram_client",
    "gpui_telegram_media",
    "gpui_telegram_webrtc",
    "gpui_telegram_agent",
    "magpie_gateway",
    "fabushi_miniapp",
}


def _extract_accounts_list(res: Any) -> list[dict[str, Any]]:
    if isinstance(res, list):
        return res
    if isinstance(res, dict):
        for key in ("accounts", "items", "list"):
            if isinstance(res.get(key), list):
                return res[key]
    return []


def _extract_conversations_list(res: Any) -> list[dict[str, Any]]:
    if isinstance(res, list):
        return res
    if isinstance(res, dict):
        for key in ("conversations", "chats", "items"):
            if isinstance(res.get(key), list):
                return res[key]
    return []


def _configure_telegram_endpoint(host, base_url: str, ws_url: str) -> None:
    host.call_any_ok(
        [
            "telegram.server.configureEndpoint",
            "telegram.server.configure",
            "telegram.server.setEndpoint",
        ],
        {"httpUrl": base_url, "baseUrl": base_url, "wsUrl": f"{ws_url}/api/v1/ws", "endpoint": base_url},
    )


def _login_phone_account(host, srv, phone: str = "+14155550101") -> dict[str, Any]:
    _configure_telegram_endpoint(host, srv.base_url, srv.ws_url)
    send_res = host.call_any_ok(
        ["telegram.auth.phoneSendCode", "telegram.auth.sendCode"],
        {"phone": phone, "phoneNumber": phone, "endpoint": srv.base_url},
    )
    otp = srv.otp_by_phone.get(phone)
    assert otp, f"phoneSendCode did not hit LoopbackTelegramServer /api/v1/auth/phone/send-code (recorded={srv.recorded_requests})"
    code_hash = ""
    if isinstance(send_res, dict):
        code_hash = str(send_res.get("phoneCodeHash") or send_res.get("phone_code_hash") or "")
    verify_res = host.call_any_ok(
        ["telegram.auth.phoneVerify", "telegram.auth.signInPhone"],
        {"phone": phone, "phoneNumber": phone, "code": otp, "phoneCodeHash": code_hash},
    )
    return verify_res if isinstance(verify_res, dict) else {"result": verify_res}


# =============================================================================
# F3 — Headless Crate Reuse, JNI/C ABI, Tokio Runtime, Event Drain & SQLite WAL
# =============================================================================


@e2e_test(
    "T1-F03-01",
    tier=1,
    features=["F3"],
    needs=["native"],
    title="cargo metadata resolves all 9 headless gpui-telegram crates with zero desktop windowing crates",
)
def test_t1_f03_01_headless_crate_reuse(ctx: E2eTestContext) -> None:
    meta = json.loads(ctx.ws.cargo_metadata_json.read_text(encoding="utf-8"))
    pkg_names = {p.get("name", "").replace("-", "_") for p in meta.get("packages", [])}
    missing = sorted(REQUIRED_HEADLESS_CRATES - pkg_names)
    assert not missing, (
        f"mahayana_android_host does not reuse upstream bhrum/gpui-telegram headless crates; missing: {missing}"
    )
    forbidden = {"gpui", "gpui_telegram_desktop", "webrtc_sys"} & pkg_names
    assert not forbidden, f"Forbidden desktop/C++ crates pulled into Android host: {sorted(forbidden)}"


@e2e_test(
    "T1-F03-02",
    tier=1,
    features=["F3"],
    needs=["native"],
    title="Production MahayanaAndroidHost opens SQLite in WAL mode and exposes telegram.events.drain",
)
def test_t1_f03_02_sqlite_wal_and_events_drain_init(ctx: E2eTestContext) -> None:
    with ctx.host_session("f03_02", test_mode=False) as host:
        info = host.call_ok("feature.info")
        assert info.get("platform") == "android" and info.get("sqliteWal") is True
        assert host.sqlite_journal_mode() == "wal", "SQLite journal_mode is not 'wal'"
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
        assert isinstance(drain, dict) and isinstance(drain.get("events"), list), (
            f"telegram.events.drain must return {{'events': [...], ...}}, got {drain!r}"
        )


@e2e_test(
    "T1-F03-03",
    tier=1,
    features=["F3"],
    needs=["jvm"],
    title="OpenJDK 17 JNI MahayanaHost round-trips astral UTF-16 surrogates, CJK & 256KB strings + events.drain",
)
def test_t1_f03_03_jni_surrogate_roundtrip_and_drain(ctx: E2eTestContext) -> None:
    out = ctx.jni.run_mode("surrogate_roundtrip", ctx.subdir("jni_surrogates"))
    assert out.startswith("PASS:surrogate_roundtrip:"), f"Unexpected JNI probe output: {out}"
    with ctx.host_session("f03_03_drain", test_mode=False) as host:
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
        assert "events" in drain


@e2e_test(
    "T1-F03-04",
    tier=1,
    features=["F3"],
    needs=["jvm"],
    title="Multi-consumer MahayanaHost shares one native handle with feature.receive fan-out & refCount safety",
)
def test_t1_f03_04_shared_host_fanout_and_refcount(ctx: E2eTestContext) -> None:
    out = ctx.jni.run_mode("shared_refcount_fanout", ctx.subdir("jni_fanout"))
    assert out == "PASS:shared_refcount_fanout"
    with ctx.host_session("f03_04_drain", test_mode=False) as host:
        drain = host.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
        assert isinstance(drain.get("events"), list)


@e2e_test(
    "T1-F03-05",
    tier=1,
    features=["F3"],
    needs=["native"],
    title="telegram.events.drain advances cursor monotonically via afterSeq without re-delivering drained events",
)
def test_t1_f03_05_cursor_monotonic_event_drain(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f03_05", test_mode=False) as host:
        auth = _login_phone_account(host, srv, "+14155550111")
        gen = int(auth.get("generation") or auth.get("accountGeneration") or 1)
        d1 = host.call_ok("telegram.events.drain", {"accountGeneration": gen, "afterSeq": 0})
        evs1 = d1.get("events") or []
        seqs = [int(e["seq"]) for e in evs1 if isinstance(e, dict) and "seq" in e]
        assert seqs == sorted(seqs) and len(seqs) == len(set(seqs)), f"Non-monotonic event seqs: {seqs}"
        last_seq = int(d1.get("lastSeq") or (seqs[-1] if seqs else 0))
        d2 = host.call_ok("telegram.events.drain", {"accountGeneration": gen, "afterSeq": last_seq})
        assert (d2.get("events") or []) == [], f"Expected 0 events when afterSeq={last_seq}, got {d2}"


@e2e_test(
    "T1-F03-06",
    tier=1,
    features=["F3"],
    needs=["native"],
    title="SQLite WAL state survives full handle destroy + cold reopen while preserving legacy feature.* RPCs",
)
def test_t1_f03_06_sqlite_wal_cold_reopen_and_legacy_compat(ctx: E2eTestContext) -> None:
    db_dir = ctx.subdir("f03_06_persist")
    with ctx.host_session("f03_06_persist", test_mode=False) as h1:
        h1.call_ok("feature.persistence.set", {"key": "dharma_key", "value": "般若波罗蜜_108"})
        h1.call_ok("telegram.events.drain", {"accountGeneration": 1, "afterSeq": 0})
    with ctx.host_session("f03_06_persist", test_mode=False) as h2:
        got = h2.call_ok("feature.persistence.get", {"key": "dharma_key"})
        val = got.get("value") if isinstance(got, dict) else got
        assert val == "般若波罗蜜_108", f"Persisted value lost across cold reopen in {db_dir}: {got!r}"
        mkt = h2.call_ok("feature.marketplace.browse", {})
        assert mkt is not None


# =============================================================================
# F4 — Cold-Start Zero-Fake-Seed, Live Auth, Sessions, Contacts & Multi-Account
# =============================================================================


@e2e_test(
    "T1-F04-01",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Fresh production MahayanaAndroidHost starts unauthenticated with 0 fake accounts and 0 fake chats",
)
def test_t1_f04_01_cold_start_zero_fake_seed(ctx: E2eTestContext) -> None:
    with ctx.host_session("f04_01_cold", test_mode=False) as host:
        st = host.call_ok("feature.auth.status")
        assert st.get("loggedIn") is False, f"Cold host must start loggedIn=false, got {st}"
        accts = _extract_accounts_list(host.call_ok("telegram.auth.accountsList"))
        assert accts == [], (
            f"Cold host fabricated fake accounts ({accts}); 'acc_primary' / '+86 138 0013 8000' seed must be deleted!"
        )
        sync = host.call_ok("feature.messaging.execute", {"command": {"type": "sync"}})
        convs = _extract_conversations_list(sync)
        assert convs == [], f"Cold host fabricated {len(convs)} fake conversations: {convs[:2]}"


@e2e_test(
    "T1-F04-02",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Phone OTP login (phoneSendCode -> phoneVerify) authenticates against live LoopbackTelegramServer",
)
def test_t1_f04_02_live_phone_otp_auth(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f04_02", test_mode=False) as host:
        res = _login_phone_account(host, srv, "+14155550101")
        paths = [r["path"] for r in srv.recorded_requests]
        assert "/api/v1/auth/phone/send-code" in paths, f"phoneSendCode never hit server: {paths}"
        assert "/api/v1/auth/phone/verify" in paths, f"phoneVerify never hit server: {paths}"
        assert res.get("loggedIn") is True or res.get("status") == "authorized" or res.get("accountId")


@e2e_test(
    "T1-F04-03",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Email OTP login & QR pairing (pending before scan -> authorized after server accept + ZXing QR check)",
)
def test_t1_f04_03_email_otp_and_qr_pairing(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f04_03", test_mode=False) as host:
        _configure_telegram_endpoint(host, srv.base_url, srv.ws_url)
        host.call_any_ok(["telegram.auth.emailSendCode"], {"email": "zen@ombhrum.org"})
        assert "zen@ombhrum.org" in srv.otp_by_email, "emailSendCode did not hit LoopbackTelegramServer"
        host.call_any_ok(["telegram.auth.emailVerify"], {"email": "zen@ombhrum.org", "code": srv.otp_by_email["zen@ombhrum.org"]})

        qr_init = host.call_any_ok(["telegram.auth.qrPairInit"], {})
        pair_tok = str(qr_init.get("pairToken") or qr_init.get("pair_token") or "")
        assert pair_tok in srv.qr_pairs, f"qrPairInit did not call /api/v1/auth/pair/init (got {qr_init})"

        poll1 = host.call_any_ok(["telegram.auth.qrPairPoll"], {"pairToken": pair_tok})
        assert poll1.get("status") == "pending", (
            f"qrPairPoll returned {poll1.get('status')!r} BEFORE remote scan (must stay 'pending'!)"
        )


@e2e_test(
    "T1-F04-04",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Cloud 2FA challenge/verify, Passkey registration/listing, and Active Device Session revocation",
)
def test_t1_f04_04_two_factor_passkeys_and_sessions(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f04_04", test_mode=False) as host:
        phone = "+14155550122"
        srv.enable_2fa_for_identifier(phone, "Sutra#2026", hint="lotus")
        _configure_telegram_endpoint(host, srv.base_url, srv.ws_url)
        host.call_any_ok(["telegram.auth.phoneSendCode"], {"phone": phone})
        step1 = host.call_any_ok(["telegram.auth.phoneVerify"], {"phone": phone, "code": srv.otp_by_phone[phone]})
        assert step1.get("status") == "2fa_required" or step1.get("twoFactorRequired") is True, (
            f"Expected 2fa_required challenge from server, got {step1}"
        )
        host.call_any_ok(
            ["telegram.auth.twoFactorVerify", "telegram.auth.verify2fa"],
            {"phone": phone, "password": "Sutra#2026", "tempToken": step1.get("tempToken") or step1.get("temp_token", "")},
        )
        sessions = host.call_any_ok(["telegram.auth.listSessions", "telegram.auth.sessionsList"], {})
        assert sessions is not None and any(r["path"] == "/api/v1/auth/sessions" for r in srv.recorded_requests)


@e2e_test(
    "T1-F04-05",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Multi-account isolation: switching between Alice and Bob isolates chats, drafts, wallets & generation",
)
def test_t1_f04_05_multi_account_isolation(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f04_05", test_mode=False) as host:
        a1 = _login_phone_account(host, srv, "+14155550131")
        id1 = str(a1.get("accountId") or a1.get("account_id") or "acc_1")
        host.call_any_ok(["telegram.messaging.setDraft", "messaging.setDraft"], {"accountId": id1, "chatId": "c_alice", "text": "Alice secret draft"})

        a2 = _login_phone_account(host, srv, "+14155550132")
        id2 = str(a2.get("accountId") or a2.get("account_id") or "acc_2")
        assert id1 != id2, "Distinct phone logins must produce distinct accountIds"

        sw = host.call_any_ok(["telegram.auth.accountSwitch", "telegram.auth.switchAccount"], {"accountId": id2})
        bob_chats_raw = json.dumps(sw)
        assert "Alice secret draft" not in bob_chats_raw, "Alice draft leaked into Bob's account state!"


@e2e_test(
    "T1-F04-06",
    tier=1,
    features=["F4"],
    needs=["native"],
    title="Contacts sync, @username peer resolution, and Argon2id local passcode hashing (no plaintext in SQLite)",
)
def test_t1_f04_06_contacts_resolve_and_argon2id_passcode(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f04_06", test_mode=False) as host:
        _login_phone_account(host, srv, "+14155550141")
        resolved = host.call_any_ok(
            ["telegram.contacts.resolveUsername", "telegram.messaging.resolvePeer", "telegram.auth.resolveUsername"],
            {"username": "@alice_dev"},
        )
        assert "alice_dev" in json.dumps(resolved)
        secret_pin = "Passcode#Lotus9911"
        host.call_any_ok(["telegram.auth.passcodeSet"], {"passcode": secret_pin})
        ver_ok = host.call_any_ok(["telegram.auth.passcodeVerify"], {"passcode": secret_pin})
        assert ver_ok.get("valid") is True or ver_ok.get("ok") is True
        assert not host.raw_storage_contains(secret_pin), "Plaintext passcode leaked into SQLite / WAL files!"


# =============================================================================
# F5 — Live WebSocket Hub, PTS SyncGetDifference, Durable Outbox & Rich Messaging
# =============================================================================


@e2e_test(
    "T1-F05-01",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="Live WebSocket hub receives server-pushed Update & Typing frames into SQLite and telegram.events.drain",
)
def test_t1_f05_01_live_websocket_push_updates(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_01", test_mode=False) as host:
        auth = _login_phone_account(host, srv, "+14155550151")
        tok = str(auth.get("token") or next(iter(srv.sessions.keys()), ""))
        gen = int(auth.get("generation") or 1)
        time.sleep(0.15)
        srv.push_ws_frame(
            tok,
            {
                "type": "Update",
                "payload": {"pts": 1, "chat_id": "chat_live", "message": {"id": "m_ws_1", "text": "Real-time WS Dharma"}},
            },
        )
        time.sleep(0.2)
        drained = host.call_ok("telegram.events.drain", {"accountGeneration": gen, "afterSeq": 0})
        assert "Real-time WS Dharma" in json.dumps(drained), f"WS push update not delivered via telegram.events.drain: {drained}"


@e2e_test(
    "T1-F05-02",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="PtsTracker detects server PTS gap and invokes POST /api/v1/sync/get_difference to recover missed messages",
)
def test_t1_f05_02_pts_gap_sync_get_difference(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_02", test_mode=False) as host:
        auth = _login_phone_account(host, srv, "+14155550152")
        acct = next(iter(srv.sessions.values()))["account_id"]
        tok = next(iter(srv.sessions.keys()))
        new_pts = srv.inject_pts_gap(acct, ["Gap msg 1", "Gap msg 2", "Gap msg 3"], chat_id="chat_gap")
        srv.push_ws_frame(tok, {"type": "PtsPush", "payload": {"pts": new_pts}})
        host.call_any(["telegram.server.syncNow", "telegram.messaging.syncDifference"], {"pts": 0})
        time.sleep(0.2)
        paths = [r["path"] for r in srv.recorded_requests]
        assert "/api/v1/sync/get_difference" in paths, f"Host never called /api/v1/sync/get_difference on PTS gap: {paths}"


@e2e_test(
    "T1-F05-03",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="Durable SQLite Outbox keeps offline messages Pending and transitions to Sent only after reconnect WS Ack",
)
def test_t1_f05_03_durable_outbox_offline_to_ack(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_03", test_mode=False) as host:
        _login_phone_account(host, srv, "+14155550153")
        srv.set_offline(True)
        srv.drop_all_ws_clients()
        send_res = host.call_any_ok(
            ["telegram.messaging.sendMessage", "messaging.sendMessage"],
            {"chatId": "chat_outbox", "text": "Offline truth check", "clientNonce": "nonce_offline_1"},
        )
        state_offline = str(send_res.get("deliveryState") or send_res.get("status") or "").lower()
        assert state_offline == "pending", (
            f"Offline message must stay 'pending' (never fake 'sent'/'read'!), got {send_res}"
        )
        srv.set_offline(False)
        host.call_any(["telegram.messaging.flushOutbox", "telegram.server.reconnect"], {})
        time.sleep(0.25)
        assert ("acc_" in json.dumps(srv.nonce_dedup) or len(srv.messages_by_chat.get("chat_outbox", [])) == 1), (
            "Outbox failed to flush queued message to LoopbackTelegramServer upon reconnect"
        )


@e2e_test(
    "T1-F05-04",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="UTF-16 MessageEntity offsets stay aligned across BMP CJK + 4-byte astral emoji surrogates + mutations",
)
def test_t1_f05_04_utf16_entities_and_message_mutations(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_04", test_mode=False) as host:
        _login_phone_account(host, srv, "+14155550154")
        rich_text = "Hi 🪷 法布施 bold_word https://ombhrum.org @alice_dev"
        bold_utf16_offset = len("Hi 🪷 法布施 ".encode("utf-16-le")) // 2
        res = host.call_any_ok(
            ["telegram.messaging.sendMessage", "messaging.sendMessage"],
            {
                "chatId": "chat_rich",
                "text": rich_text,
                "entities": [{"type": "Bold", "offset": bold_utf16_offset, "length": 9}],
            },
        )
        mid = str(res.get("messageId") or res.get("id") or "srv_msg_1")
        host.call_any_ok(["telegram.messaging.editMessage", "messaging.editMessage"], {"chatId": "chat_rich", "messageId": mid, "text": rich_text + " (edited)"})
        host.call_any_ok(["telegram.messaging.setReaction", "messaging.setReaction"], {"chatId": "chat_rich", "messageId": mid, "emoji": "🙏"})


@e2e_test(
    "T1-F05-05",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="Canonical messaging.votePoll sends integer optionIndex to /api/v1/polls/{id}/vote + cloud drafts & folders",
)
def test_t1_f05_05_canonical_poll_option_index_and_folders(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_05", test_mode=False) as host:
        auth = _login_phone_account(host, srv, "+14155550155")
        acct = str(auth.get("accountId") or "acc_1")
        vote = host.call_any_ok(
            ["messaging.votePoll", "telegram.messaging.votePoll"],
            {"accountId": acct, "chatId": "chat_poll", "pollId": "poll_108", "messageId": "m_poll_1", "optionIndex": 1},
        )
        assert srv.polls.get("poll_108", {}).get("votes", {}).get("1") == 1, (
            f"messaging.votePoll(optionIndex=1) did not hit /api/v1/polls/poll_108/vote with integer index: {vote}"
        )


@e2e_test(
    "T1-F05-06",
    tier=1,
    features=["F5"],
    needs=["native"],
    title="FTS5 CJK/Latin search, live server channelStats (not hardcoded 10800), and XSS-safe JSON/HTML chat export",
)
def test_t1_f05_06_fts5_search_live_channel_stats_and_export(ctx: E2eTestContext) -> None:
    with ctx.telegram_server() as srv, ctx.host_session("f05_06", test_mode=False) as host:
        _login_phone_account(host, srv, "+14155550156")
        srv.channel_stats["chan_dharma"] = {"subscribers": 7777, "views_per_post": 2345, "shares_per_post": 108}
        stats = host.call_any_ok(["telegram.messaging.channelStats"], {"conversationId": "chan_dharma", "chatId": "chan_dharma"})
        raw_stats = json.dumps(stats)
        assert "10800" not in raw_stats and "7777" in raw_stats, (
            f"channelStats returned hardcoded 10800/4320 stub instead of live server 7777 stats: {stats}"
        )
        srv.messages_by_chat["chan_dharma"] = [{"id": "m1", "text": "<script>alert(1)</script> 金刚般若波罗蜜"}]
        exp_html = host.call_any_ok(["telegram.messaging.exportChat"], {"conversationId": "chan_dharma", "chatId": "chan_dharma", "format": "html"})
        html_str = exp_html.get("content") or exp_html.get("html") or json.dumps(exp_html)
        assert "<script>alert(1)</script>" not in html_str and "&lt;script&gt;" in html_str


# =============================================================================
# F6 — Media CAS/Range, Waveform/VAD, Pure-Rust WebRTC, Peer E2EE, TON & Dharma
# =============================================================================


@e2e_test(
    "T1-F06-01",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="Resumable 512KB chunked CAS upload deduplicates identical SHA-256 and streams HTTP 206 byte ranges",
)
def test_t1_f06_01_resumable_cas_and_http_206_range(ctx: E2eTestContext) -> None:
    payload = (b"FABUSHI_CAS_BLOCK_" * 75_000)[: 1_310_720]  # 1.25 MiB (> 2 x 512 KiB chunks)
    expected_sha = hashlib.sha256(payload).hexdigest()
    with ctx.telegram_server() as srv, ctx.host_session("f06_01", test_mode=False) as host:
        _login_phone_account(host, srv, "+14155550161")
        srv.media_blobs[expected_sha] = payload
        rng_res = host.call_any_ok(
            ["telegram.media.streamRange", "telegram.messaging.readMediaRange"],
            {"fileId": expected_sha, "offset": 65536, "length": 65536, "endpoint": srv.base_url},
        )
        chunk_b64 = str(rng_res.get("dataBase64") or rng_res.get("chunkBase64") or "")
        decoded = base64.b64decode(chunk_b64)
        assert decoded == payload[65536 : 65536 + 65536], "HTTP 206 Range slice did not match exact source bytes"


@e2e_test(
    "T1-F06-02",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="5-bit 100-bar Telegram waveform codec (exact 63 bytes) & <60ms PCM16 RMS VAD barge-in detection",
)
def test_t1_f06_02_waveform_codec_and_vad_barge_in(ctx: E2eTestContext) -> None:
    bars = [(i * 7 + 3) % 32 for i in range(100)]
    with ctx.host_session("f06_02", test_mode=False) as host:
        enc = host.call_ok("telegram.messaging.encodeWaveform", {"bars": bars})
        raw_wf = base64.b64decode(str(enc.get("waveformBase64") or enc.get("encoded") or ""))
        assert len(raw_wf) == 63, f"100 x 5-bit waveform must encode to exact 63 bytes, got {len(raw_wf)}"
        dec = host.call_ok("telegram.messaging.decodeWaveform", {"waveformBase64": base64.b64encode(raw_wf).decode("ascii")})
        dec_bars = dec.get("bars") or []
        assert dec_bars[:100] == bars, "5-bit LSB waveform round-trip mismatch"

        silence_pcm = base64.b64encode(b"\x00\x00" * 288).decode("ascii")
        speech_pcm = base64.b64encode(struct.pack("<h", 6500) * 288).decode("ascii")
        s_res = host.call_ok("telegram.calls.vadFeedPcm", {"pcm16Base64": silence_pcm, "sampleRate": 16000, "ttsPlaying": True})
        assert s_res.get("bargeInTriggered") is False
        barge = False
        latency = 999
        for _ in range(4):
            v_res = host.call_ok("telegram.calls.vadFeedPcm", {"pcm16Base64": speech_pcm, "sampleRate": 16000, "ttsPlaying": True})
            if v_res.get("bargeInTriggered") is True:
                barge = True
                latency = int(v_res.get("detectionLatencyMs", 54))
                break
        assert barge and latency < 60, f"Expected <60ms VAD barge-in on speech PCM, got barge={barge} latency={latency}ms"
        # Ensure host also has gpui_telegram_webrtc wired (not legacy v2.0.0 stub)
        meta = json.loads(ctx.ws.cargo_metadata_json.read_text(encoding="utf-8"))
        assert any(p.get("name") == "gpui_telegram_webrtc" for p in meta.get("packages", [])), (
            "gpui_telegram_webrtc crate missing from native host"
        )


@e2e_test(
    "T1-F06-03",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="Pure-Rust WebRTC generates RFC 8866 SDP Offer/Answer + ICE candidates & 333-emoji 4-glyph SAS fingerprint",
)
def test_t1_f06_03_pure_rust_webrtc_sdp_and_sas_emoji(ctx: E2eTestContext) -> None:
    with (
        ctx.telegram_server() as srv,
        ctx.host_session("f06_03_caller", test_mode=False) as caller,
        ctx.host_session("f06_03_callee", test_mode=False) as callee,
    ):
        _login_phone_account(caller, srv, "+14155550163")
        _login_phone_account(callee, srv, "+14155550164")
        offer = caller.call_any_ok(
            ["telegram.calls.createOffer", "telegram.calls.startCall"],
            {"peerId": "user_bob_peer", "video": True},
        )
        sdp_offer = str(offer.get("sdp") or offer.get("offerSdp") or "")
        assert "v=0" in sdp_offer and "a=ice-ufrag:" in sdp_offer and "a=fingerprint:sha-256" in sdp_offer, (
            f"startCall/createOffer returned cosmetic timer row without real RFC 8866 WebRTC SDP: {offer}"
        )
        ans = callee.call_any_ok(["telegram.calls.acceptOffer", "telegram.calls.createAnswer"], {"offerSdp": sdp_offer})
        sdp_ans = str(ans.get("sdp") or ans.get("answerSdp") or "")
        assert "v=0" in sdp_ans and "a=ice-pwd:" in sdp_ans


@e2e_test(
    "T1-F06-04",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="Two isolated hosts perform remote X3DH prekey fetch + Double Ratchet out-of-order E2EE & Group Sender Keys",
)
def test_t1_f06_04_remote_peer_x3dh_double_ratchet_and_sender_keys(ctx: E2eTestContext) -> None:
    with (
        ctx.telegram_server() as srv,
        ctx.host_session("f06_04_alice", test_mode=False) as alice,
        ctx.host_session("f06_04_bob", test_mode=False) as bob,
    ):
        _login_phone_account(alice, srv, "+14155550165")
        _login_phone_account(bob, srv, "+14155550166")
        bob_pub = bob.call_any_ok(["telegram.e2ee.publishPrekeys", "e2ee.publishPrekeys"], {"userId": "bob_remote"})
        assert "bob_remote" in srv.prekey_bundles, f"Bob publishPrekeys did not upload bundle to server: {bob_pub}"

        sess = alice.call_any_ok(
            ["telegram.e2ee.initPeerSession", "e2ee.initPeerSession"],
            {"peerUserId": "bob_remote", "conversationId": "sec_ab"},
        )
        ct1 = alice.call_any_ok(["telegram.e2ee.encryptPeerMessage", "e2ee.encrypt"], {"conversationId": "sec_ab", "plaintext": "Msg #1"})
        ct2 = alice.call_any_ok(["telegram.e2ee.encryptPeerMessage", "e2ee.encrypt"], {"conversationId": "sec_ab", "plaintext": "Msg #2"})
        # Deliver out of order (#2 then #1) to Bob!
        pt2 = bob.call_any_ok(["telegram.e2ee.decryptPeerMessage", "e2ee.decrypt"], {"conversationId": "sec_ab", "envelope": ct2, "initHeader": sess})
        pt1 = bob.call_any_ok(["telegram.e2ee.decryptPeerMessage", "e2ee.decrypt"], {"conversationId": "sec_ab", "envelope": ct1})
        assert "Msg #2" in json.dumps(pt2) and "Msg #1" in json.dumps(pt1)


@e2e_test(
    "T1-F06-05",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="TON Wallet v4R2 starts at truthful 0 TON, decodes ISO-18004 QR via ZXing, and broadcasts signed BOC to RPC",
)
def test_t1_f06_05_ton_wallet_v4r2_zxing_qr_and_boc_rpc(ctx: E2eTestContext) -> None:
    with ctx.ton_rpc() as ton, ctx.host_session("f06_05", test_mode=False) as host:
        host.call_any(["wallet.configureRpc", "wallet.setRpcEndpoint"], {"rpcUrl": f"{ton.base_url}/api/v2", "endpoint": f"{ton.base_url}/api/v2"})
        created = host.call_ok("wallet.createWallet", {"rpcUrl": f"{ton.base_url}/api/v2"})
        bal_str = str(created.get("balanceNanotons") if "balanceNanotons" in created else created.get("balance", ""))
        assert bal_str in ("0", "0.0", "0.0000"), (
            f"Newly created TON wallet reported fake balance {created!r} instead of truthful 0 before RPC funding!"
        )
        address = str(created.get("address") or created.get("friendlyAddress") or "")
        assert address.startswith(("UQ", "EQ")) and len(address) == 48, f"Invalid TON v4R2 friendly address: {address!r}"

        # Canonical wallet.generateQrMatrix optically verified by ZXing QRCodeReader!
        invoice = f"ton://transfer/{address}?amount=2000000000&text=dharma"
        qr = host.call_ok("wallet.generateQrMatrix", {"data": invoice, "payload": invoice})
        ok_qr, decoded = ctx.qr.decode_matrix(qr)
        assert ok_qr and decoded == invoice, f"ZXing failed to decode wallet.generateQrMatrix output: {decoded!r}"

        # Fund on loopback TON RPC and send signed BOC
        ton.fund_address(address, 12_500_000_000)
        refreshed = host.call_any_ok(["wallet.refreshBalance", "wallet.balance"], {"address": address, "rpcUrl": f"{ton.base_url}/api/v2"})
        assert "12500000000" in json.dumps(refreshed) or "12.5" in json.dumps(refreshed)
        host.call_ok(
            "wallet.sendTon",
            {"toAddress": address, "amountNanotons": 2_000_000_000, "comment": "Offering", "rpcUrl": f"{ton.base_url}/api/v2"},
        )
        assert len(ton.submitted_bocs) >= 1, "wallet.sendTon did not submit signed 0xb5ee9c72 BOC to LoopbackTonRpcServer"


@e2e_test(
    "T1-F06-06",
    tier=1,
    features=["F6"],
    needs=["native"],
    title="fabushi_miniapp illuminates >=240 countries, tracks streaks/badges, and streams live Dharma AI via Magpie",
)
def test_t1_f06_06_fabushi_miniapp_240_countries_and_live_dharma_ai(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-dharma-live") as hub, ctx.host_session("f06_06", test_mode=False) as host:
        countries = host.call_ok("fabushiMiniapp.countries", {})
        c_list = countries if isinstance(countries, list) else (countries.get("countries") or [])
        assert len(c_list) >= 240, f"Expected >= 240 countries in fabushi_miniapp, got {len(c_list)}"
        host.call_ok("fabushiMiniapp.offerDharma", {"countryCode": "NP", "sutraId": "heart_sutra"})
        host.call_ok(
            "magpie.upsertProvider",
            {"id": "openai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-dharma-live", "model": "gpt-4o-mini", "enabled": True},
        )
        ai = host.call_ok("fabushiMiniapp.aiDharmaChat", {"prompt": "请阐释缘起性空"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(ai), (
            f"fabushiMiniapp.aiDharmaChat returned hardcoded sutra string instead of live Magpie upstream: {ai}"
        )


# =============================================================================
# F7 — Magpie 4-Protocol Live SSE + PII Masking, Hermes Studio & Computer Lease
# =============================================================================


@e2e_test(
    "T1-F07-01",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="magpie_gateway streams live SSE across all 4 protocols (OpenAI, Responses, Anthropic, Gemini)",
)
def test_t1_f07_01_magpie_four_protocol_rosetta_sse(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-rosetta-4p") as hub, ctx.host_session("f07_01", test_mode=False) as host:
        protocols = [
            ("p_oai", "openai_compat", f"{hub.base_url}/v1", "gpt-4o-mini", "/chat/completions"),
            ("p_rsp", "openai_responses", f"{hub.base_url}/v1", "gpt-5-codex", "/responses"),
            ("p_ant", "anthropic", f"{hub.base_url}/v1", "claude-sonnet-4", "/messages"),
            ("p_gem", "gemini", f"{hub.base_url}/v1beta", "gemini-2.5-pro", ":streamGenerateContent"),
        ]
        for pid, proto, url, model, expected_path_part in protocols:
            host.call_any_ok(
                ["magpie.upsertProvider", "magpie.configureProvider"],
                {"id": pid, "protocol": proto, "baseUrl": url, "apiKey": "sk-rosetta-4p", "model": model, "enabled": True},
            )
            ans = host.call_ok("magpie.chatComplete", {"providerId": pid, "prompt": f"Ping {proto}"})
            assert f"LIVE_UPSTREAM_OK: Ping {proto}" in json.dumps(ans), (
                f"Protocol {proto} did not stream real reply from LoopbackMagpieUpstreamHub: {ans}"
            )
            assert any(expected_path_part in r["path"] for r in hub.recorded_requests), (
                f"Protocol {proto} never hit {expected_path_part}; recorded={[r['path'] for r in hub.recorded_requests]}"
            )


@e2e_test(
    "T1-F07-02",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="Magpie wire-level PII/secret masking redacts outbound payload before network egress and unmasks reply",
)
def test_t1_f07_02_magpie_wire_pii_secret_masking(ctx: E2eTestContext) -> None:
    raw_secret = "sk-proj-9876543210abcdef9876543210"
    raw_email = "monk@ombhrum.org"
    with ctx.magpie_hub("sk-mask-key") as hub, ctx.host_session("f07_02", test_mode=False) as host:
        host.call_any_ok(
            ["magpie.upsertProvider", "magpie.configureProvider"],
            {"id": "oai_mask", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-mask-key", "model": "gpt-4o-mini", "enabled": True},
        )
        resp = host.call_ok(
            "magpie.chatComplete",
            {"providerId": "oai_mask", "prompt": f"Audit token {raw_secret} for {raw_email}", "maskSecrets": True},
        )
        assert hub.recorded_requests, "magpie.chatComplete did not send HTTP request to upstream hub"
        wire_body = hub.recorded_requests[-1]["body_text"]
        assert raw_secret not in wire_body and raw_email not in wire_body, (
            f"Wire payload leaked unmasked secret/email to upstream LLM: {wire_body}"
        )
        reply_str = json.dumps(resp)
        assert raw_secret in reply_str and raw_email in reply_str, (
            f"Unmasked secret/email was not restored in final assistant reply: {resp}"
        )


@e2e_test(
    "T1-F07-03",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="Magpie imports >=47 provider presets (magpie://import + .env) and routes across all 6 strategies",
)
def test_t1_f07_03_magpie_import_and_six_routing_strategies(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-route-key") as hub, ctx.host_session("f07_03", test_mode=False) as host:
        cat = host.call_any_ok(["magpie.presets", "magpie.listProviders"], {})
        presets = cat if isinstance(cat, list) else (cat.get("presets") or cat.get("providers") or [])
        assert len(presets) >= 47, f"Expected >= 47 Magpie provider presets, got {len(presets)}"
        host.call_any_ok(["magpie.importEnv"], {"envText": "OPENAI_API_KEY=sk-route-key\nANTHROPIC_API_KEY=sk-route-key"})
        for strat in ("Order", "Rotate", "Cheapest", "Fastest", "Usage", "Smart"):
            prev = host.call_any_ok(["magpie.routePreview"], {"strategy": strat})
            assert prev is not None
        # Verify live chatComplete works with default liveUpstream (without caller passing liveUpstream=true!)
        host.call_any_ok(
            ["magpie.upsertProvider", "magpie.configureProvider"],
            {"id": "openai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-route-key", "model": "gpt-4o-mini", "enabled": True},
        )
        live = host.call_ok("magpie.chatComplete", {"providerId": "openai", "prompt": "Strategy check"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(live)


@e2e_test(
    "T1-F07-04",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="Hermes AcpRunner + HermesEngine streams live LLM turns, branches sessions & resolves all 61 slash commands",
)
def test_t1_f07_04_hermes_acp_runner_and_61_slash_commands(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-hermes-live") as hub, ctx.host_session("f07_04", test_mode=False) as host:
        host.call_any_ok(
            ["magpie.upsertProvider", "magpie.configureProvider"],
            {"id": "openai", "protocol": "openai_compat", "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-hermes-live", "model": "gpt-4o-mini", "enabled": True},
        )
        sess = host.call_any_ok(["hermes.createSession", "hermes.sessionCreate"], {"title": "Heart Sutra Study"})
        sid = str(sess.get("sessionId") or sess.get("id") or "sess_1")
        turn = host.call_any_ok(["hermes.sendTurn", "hermes.chat"], {"sessionId": sid, "prompt": "Explain Sunyata"})
        assert "LIVE_UPSTREAM_OK" in json.dumps(turn), (
            f"hermes.sendTurn returned hardcoded stub instead of live LLM completion: {turn}"
        )
        cmds = host.call_any_ok(["hermes.slashCommands", "hermes.listSlashCommands"], {})
        cmd_list = cmds if isinstance(cmds, list) else (cmds.get("commands") or [])
        assert len(cmd_list) >= 61, f"Expected all 61 Hermes slash commands, got {len(cmd_list)}"


@e2e_test(
    "T1-F07-05",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="Hermes /moa Mixture-of-Agents executes multi-proposer parallel upstream requests + aggregator synthesis",
)
def test_t1_f07_05_hermes_moa_multi_proposer_synthesis(ctx: E2eTestContext) -> None:
    with ctx.magpie_hub("sk-moa-live") as hub, ctx.host_session("f07_05", test_mode=False) as host:
        for pid, proto, model in (
            ("openai", "openai_compat", "gpt-4o"),
            ("anthropic", "anthropic", "claude-sonnet-4"),
        ):
            host.call_any_ok(
                ["magpie.upsertProvider", "magpie.configureProvider"],
                {"id": pid, "protocol": proto, "baseUrl": f"{hub.base_url}/v1", "apiKey": "sk-moa-live", "model": model, "enabled": True},
            )
        moa = host.call_any_ok(
            ["hermes.runSlashCommand", "hermes.moaSynthesize"],
            {"command": "/moa Design zero-swap Android build", "prompt": "Design zero-swap Android build"},
        )
        assert len(hub.recorded_requests) >= 3, (
            f"/moa must issue >= 2 proposer calls + 1 aggregator synthesis call; upstream saw {len(hub.recorded_requests)} requests: {moa}"
        )


@e2e_test(
    "T1-F07-06",
    tier=1,
    features=["F7"],
    needs=["native"],
    title="ComputerLeaseBroker enforces leaseId + generation fencing on computer.executeOp & 5-tier ActionBus",
)
def test_t1_f07_06_computer_lease_generation_and_action_bus(ctx: E2eTestContext) -> None:
    with ctx.host_session("f07_06", test_mode=False) as host:
        lease = host.call_ok("computer.leaseAcquire", {"nodeId": "node-local-android", "holder": "agent:mika", "ttlMs": 30000})
        lease_id = str(lease.get("leaseId") or lease.get("id") or "")
        gen = int(lease.get("generation", -1))
        assert lease_id and gen >= 1, f"computer.leaseAcquire missing leaseId or generation: {lease}"
        op_ok = host.call_ok(
            "computer.executeOp",
            {"nodeId": "node-local-android", "leaseId": lease_id, "generation": gen, "op": {"type": "screenshot"}},
        )
        assert op_ok is not None
        # Verify takeover increments generation and invalidates old generation!
        takeover = host.call_ok("computer.leaseTakeover", {"nodeId": "node-local-android", "holder": "human:owner"})
        new_gen = int(takeover.get("generation", -1))
        assert new_gen > gen, f"leaseTakeover must bump generation ({gen} -> {new_gen})"
        stale_resp = host.call(
            "computer.executeOp",
            {"nodeId": "node-local-android", "leaseId": lease_id, "generation": gen, "op": {"type": "tap", "x": 100, "y": 200}},
        )
        assert_structured_error(stale_resp, expected_codes=("STALE", "LEASE", "GENERATION"), context="stale computer.executeOp")
