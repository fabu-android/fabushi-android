"""Pure-Python 3.11 loopback HTTP/1.1 + RFC 6455 WS + 4-Protocol SSE + TON RPC + WebMCP servers."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import os
import re
import struct
import threading
import time
import urllib.parse
import uuid
from typing import Any

_WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def _build_wav_pcm16_mono(samples_count: int = 2400, sample_rate: int = 24000) -> bytes:
    pcm = b"\x10\x02" * samples_count
    data_size = len(pcm)
    header = struct.pack(
        "<4sI4s4sIHHIIHH4sI",
        b"RIFF",
        36 + data_size,
        b"WAVE",
        b"fmt ",
        16,
        1,  # PCM
        1,  # mono
        sample_rate,
        sample_rate * 2,
        2,
        16,
        b"data",
        data_size,
    )
    return header + pcm


async def _read_http_request(
    reader: asyncio.StreamReader,
) -> tuple[str, str, dict[str, str], bytes] | None:
    try:
        header_bytes = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), timeout=15.0)
    except (asyncio.IncompleteReadError, asyncio.TimeoutError, ConnectionError):
        return None
    header_text = header_bytes.decode("iso-8859-1", errors="replace")
    lines = header_text.split("\r\n")
    if not lines or not lines[0]:
        return None
    parts = lines[0].split(" ")
    if len(parts) < 2:
        return None
    method, raw_target = parts[0].upper(), parts[1]
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if ":" in line:
            k, v = line.split(":", 1)
            headers[k.strip().lower()] = v.strip()

    body = b""
    if headers.get("transfer-encoding", "").lower() == "chunked":
        chunks: list[bytes] = []
        while True:
            sz_line = await asyncio.wait_for(reader.readuntil(b"\r\n"), timeout=10.0)
            sz_hex = sz_line.decode("ascii", errors="ignore").strip().split(";", 1)[0]
            sz = int(sz_hex or "0", 16)
            if sz == 0:
                await reader.readuntil(b"\r\n")
                break
            chunk = await reader.readexactly(sz)
            await reader.readexactly(2)
            chunks.append(chunk)
        body = b"".join(chunks)
    else:
        content_len = int(headers.get("content-length", "0") or "0")
        if content_len > 0:
            body = await asyncio.wait_for(reader.readexactly(content_len), timeout=15.0)
    return method, raw_target, headers, body


def _write_http_response(
    writer: asyncio.StreamWriter,
    status: int,
    reason: str,
    body: bytes,
    content_type: str = "application/json; charset=utf-8",
    extra_headers: dict[str, str] | None = None,
) -> None:
    hdrs = {
        "Content-Type": content_type,
        "Content-Length": str(len(body)),
        "Connection": "close",
    }
    if extra_headers:
        hdrs.update(extra_headers)
    head = f"HTTP/1.1 {status} {reason}\r\n" + "".join(f"{k}: {v}\r\n" for k, v in hdrs.items()) + "\r\n"
    writer.write(head.encode("iso-8859-1") + body)


def _encode_ws_frame(payload: bytes, opcode: int = 0x1) -> bytes:
    length = len(payload)
    first_byte = 0x80 | (opcode & 0x0F)
    if length < 126:
        header = struct.pack("!BB", first_byte, length)
    elif length < 65536:
        header = struct.pack("!BBH", first_byte, 126, length)
    else:
        header = struct.pack("!BBQ", first_byte, 127, length)
    return header + payload


async def _read_ws_frame(reader: asyncio.StreamReader) -> tuple[int, bytes] | None:
    try:
        head = await reader.readexactly(2)
    except (asyncio.IncompleteReadError, ConnectionError):
        return None
    b1, b2 = head[0], head[1]
    opcode = b1 & 0x0F
    masked = bool(b2 & 0x80)
    length = b2 & 0x7F
    if length == 126:
        ext = await reader.readexactly(2)
        length = struct.unpack("!H", ext)[0]
    elif length == 127:
        ext = await reader.readexactly(8)
        length = struct.unpack("!Q", ext)[0]
    mask_key = await reader.readexactly(4) if masked else b""
    data = await reader.readexactly(length) if length > 0 else b""
    if masked and data:
        unmasked = bytearray(len(data))
        for idx, b_val in enumerate(data):
            unmasked[idx] = b_val ^ mask_key[idx % 4]
        data = bytes(unmasked)
    return opcode, data


class _AsyncLoopbackBase:
    """Thread-hosted asyncio server bound strictly to 127.0.0.1:0."""

    def __init__(self) -> None:
        self._loop: asyncio.AbstractEventLoop | None = None
        self._thread: threading.Thread | None = None
        self._server: asyncio.AbstractServer | None = None
        self.host = "127.0.0.1"
        self.port = 0
        self._started = threading.Event()
        self.offline = False
        self.recorded_requests: list[dict[str, Any]] = []
        self._lock = threading.RLock()

    @property
    def base_url(self) -> str:
        return f"http://{self.host}:{self.port}"

    @property
    def ws_url(self) -> str:
        return f"ws://{self.host}:{self.port}"

    def start(self) -> "_AsyncLoopbackBase":
        if self._thread is not None:
            return self
        self._thread = threading.Thread(target=self._run_loop, daemon=True)
        self._thread.start()
        if not self._started.wait(timeout=10.0):
            raise RuntimeError(f"Failed to start {type(self).__name__} on 127.0.0.1:0")
        return self

    def _run_loop(self) -> None:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        self._loop = loop

        async def _boot() -> None:
            self._server = await asyncio.start_server(
                self._dispatch_conn, host="127.0.0.1", port=0
            )
            sockets = self._server.sockets or []
            if sockets:
                self.port = int(sockets[0].getsockname()[1])
            self._started.set()

        loop.run_until_complete(_boot())
        try:
            loop.run_forever()
        finally:
            pending = asyncio.all_tasks(loop)
            for task in pending:
                task.cancel()
            if pending:
                loop.run_until_complete(asyncio.gather(*pending, return_exceptions=True))
            loop.close()

    async def _dispatch_conn(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        try:
            if self.offline:
                writer.close()
                return
            await self._handle_connection(reader, writer)
        except Exception:  # noqa: BLE001
            pass
        finally:
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:  # noqa: BLE001
                pass

    async def _handle_connection(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        raise NotImplementedError

    def set_offline(self, flag: bool) -> None:
        self.offline = flag

    def stop(self) -> None:
        loop = self._loop
        if loop is None:
            return

        async def _shutdown() -> None:
            if self._server is not None:
                self._server.close()
                await self._server.wait_closed()
            loop.stop()

        try:
            asyncio.run_coroutine_threadsafe(_shutdown(), loop).result(timeout=5.0)
        except Exception:  # noqa: BLE001
            pass
        if self._thread is not None:
            self._thread.join(timeout=5.0)
            self._thread = None
        self._loop = None

    def __enter__(self) -> "_AsyncLoopbackBase":
        return self.start()

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.stop()


class LoopbackTelegramServer(_AsyncLoopbackBase):
    """Stateful HTTP/1.1 + RFC 6455 WebSocket mock gpui_telegram_server."""

    def __init__(self) -> None:
        super().__init__()
        self.otp_by_phone: dict[str, str] = {}
        self.otp_by_email: dict[str, str] = {}
        self.twofa_passwords: dict[str, tuple[str, str]] = {}  # key -> (password, hint)
        self.pending_2fa_tokens: dict[str, dict[str, Any]] = {}
        self.sessions: dict[str, dict[str, Any]] = {}  # token -> session dict
        self.qr_pairs: dict[str, dict[str, Any]] = {}  # pair_token -> state
        self.passkeys_by_account: dict[str, list[dict[str, Any]]] = {}
        self.directory_peers: dict[str, dict[str, Any]] = {
            "alice_dev": {"id": "user_alice_dev", "username": "alice_dev", "display_name": "Alice Dev", "phone": "+14155550199"},
            "bob_peer": {"id": "user_bob_peer", "username": "bob_peer", "display_name": "Bob Peer", "phone": "+14155550188"},
        }
        self.contacts_by_account: dict[str, list[dict[str, Any]]] = {}
        self.blocked_by_account: dict[str, set[str]] = {}
        self.chats_by_account: dict[str, dict[str, dict[str, Any]]] = {}
        self.messages_by_chat: dict[str, list[dict[str, Any]]] = {}
        self.nonce_dedup: dict[tuple[str, str, str], dict[str, Any]] = {}
        self.pts_by_account: dict[str, int] = {}
        self.updates_by_account: dict[str, list[dict[str, Any]]] = {}
        self.drafts: dict[tuple[str, str], dict[str, Any]] = {}
        self.folders_by_account: dict[str, list[dict[str, Any]]] = {}
        self.polls: dict[str, dict[str, Any]] = {}
        self.topics_by_chat: dict[str, list[dict[str, Any]]] = {}
        self.media_blobs: dict[str, bytes] = {}
        self.prekey_bundles: dict[str, dict[str, Any]] = {}
        self.call_signals: list[dict[str, Any]] = []
        self.channel_stats: dict[str, dict[str, Any]] = {}
        self.drop_ws_before_ack_once = False
        self.permanent_error_chats: set[str] = set()
        self._ws_clients: dict[str, list[asyncio.StreamWriter]] = {}
        self.ws_received_frames: list[dict[str, Any]] = []

    def enable_2fa_for_identifier(self, identifier: str, password: str, hint: str = "lotus") -> None:
        with self._lock:
            self.twofa_passwords[identifier] = (password, hint)

    def push_ws_frame(self, token: str, frame: dict[str, Any]) -> None:
        if self._loop is None:
            return
        raw = _encode_ws_frame(json.dumps(frame).encode("utf-8"), opcode=0x1)

        async def _send() -> None:
            writers = list(self._ws_clients.get(token, []))
            for w in writers:
                try:
                    w.write(raw)
                    await w.drain()
                except Exception:  # noqa: BLE001
                    pass

        asyncio.run_coroutine_threadsafe(_send(), self._loop).result(timeout=5.0)

    def drop_all_ws_clients(self) -> None:
        if self._loop is None:
            return

        async def _drop() -> None:
            for writers in self._ws_clients.values():
                for w in list(writers):
                    try:
                        w.write(_encode_ws_frame(b"\x03\xf3", opcode=0x8))
                        await w.drain()
                        w.close()
                    except Exception:  # noqa: BLE001
                        pass
            self._ws_clients.clear()

        asyncio.run_coroutine_threadsafe(_drop(), self._loop).result(timeout=5.0)

    def inject_pts_gap(self, account_id: str, texts: list[str], chat_id: str = "chat_general") -> int:
        with self._lock:
            pts = self.pts_by_account.get(account_id, 0)
            for txt in texts:
                pts += 1
                msg = {
                    "id": f"msg_pts_{pts}",
                    "chat_id": chat_id,
                    "sender_id": "user_alice_dev",
                    "text": txt,
                    "pts": pts,
                    "created_at": int(time.time() * 1000),
                }
                self.messages_by_chat.setdefault(chat_id, []).append(msg)
                self.updates_by_account.setdefault(account_id, []).append(
                    {"pts": pts, "type": "new_message", "message": msg}
                )
            self.pts_by_account[account_id] = pts
            return pts

    def _auth_session_from_headers(self, headers: dict[str, str], query: dict[str, list[str]]) -> dict[str, Any] | None:
        auth = headers.get("authorization", "")
        tok = ""
        if auth.lower().startswith("bearer "):
            tok = auth[7:].strip()
        elif "token" in query and query["token"]:
            tok = query["token"][0]
        if not tok:
            return None
        with self._lock:
            sess = self.sessions.get(tok)
            if sess and not sess.get("revoked", False):
                return sess
        return None

    def _issue_session(self, account_id: str, identifier: str, device_name: str = "Fabushi Android") -> dict[str, Any]:
        tok = f"tok_{uuid.uuid4().hex}"
        sess_id = f"sess_{uuid.uuid4().hex[:10]}"
        sess = {
            "session_id": sess_id,
            "token": tok,
            "account_id": account_id,
            "identifier": identifier,
            "device_name": device_name,
            "created_at": int(time.time()),
            "revoked": False,
        }
        self.sessions[tok] = sess
        self.pts_by_account.setdefault(account_id, 0)
        return sess

    async def _handle_connection(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        req = await _read_http_request(reader)
        if req is None:
            return
        method, raw_target, headers, body = req
        parsed_url = urllib.parse.urlsplit(raw_target)
        path = parsed_url.path
        query = urllib.parse.parse_qs(parsed_url.query)

        json_body: Any = None
        if body:
            try:
                json_body = json.loads(body.decode("utf-8", errors="replace"))
            except json.JSONDecodeError:
                json_body = None

        with self._lock:
            self.recorded_requests.append(
                {
                    "method": method,
                    "path": path,
                    "query": query,
                    "headers": dict(headers),
                    "json": json_body,
                    "raw_len": len(body),
                }
            )

        if headers.get("upgrade", "").lower() == "websocket" and path.startswith("/api/v1/ws"):
            await self._handle_websocket(reader, writer, headers, query)
            return

        status, payload, content_type, extra = self._route_http(
            method, path, query, headers, body, json_body
        )
        reason = {
            200: "OK",
            201: "Created",
            206: "Partial Content",
            400: "Bad Request",
            401: "Unauthorized",
            403: "Forbidden",
            404: "Not Found",
            410: "Gone",
            500: "Internal Server Error",
        }.get(status, "OK")
        if isinstance(payload, (bytes, bytearray)):
            raw_out = bytes(payload)
        else:
            raw_out = json.dumps(payload).encode("utf-8")
        _write_http_response(writer, status, reason, raw_out, content_type, extra)
        await writer.drain()

    async def _handle_websocket(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
        headers: dict[str, str],
        query: dict[str, list[str]],
    ) -> None:
        sess = self._auth_session_from_headers(headers, query)
        tok = (query.get("token") or ["anonymous"])[0]
        ws_key = headers.get("sec-websocket-key", "")
        accept = base64.b64encode(hashlib.sha1((ws_key + _WS_GUID).encode("ascii")).digest()).decode("ascii")
        handshake = (
            "HTTP/1.1 101 Switching Protocols\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
        )
        writer.write(handshake.encode("ascii"))
        await writer.drain()

        with self._lock:
            self._ws_clients.setdefault(tok, []).append(writer)
        account_id = sess["account_id"] if sess else f"acc_{tok[:8]}"

        try:
            while not self.offline:
                frame = await _read_ws_frame(reader)
                if frame is None:
                    break
                opcode, data = frame
                if opcode == 0x8:  # Close
                    writer.write(_encode_ws_frame(b"", opcode=0x8))
                    await writer.drain()
                    break
                if opcode == 0x9:  # Ping
                    writer.write(_encode_ws_frame(data, opcode=0xA))
                    await writer.drain()
                    continue
                if opcode != 0x1:
                    continue
                msg = json.loads(data.decode("utf-8", errors="replace"))
                with self._lock:
                    self.ws_received_frames.append(msg)
                ftype = msg.get("type", "")
                fpayload = msg.get("payload") or {}
                if ftype == "Ping":
                    resp = {"type": "Pong", "payload": {"nonce": fpayload.get("nonce", 0)}}
                    writer.write(_encode_ws_frame(json.dumps(resp).encode("utf-8")))
                    await writer.drain()
                elif ftype == "SendMessage":
                    chat_id = str(fpayload.get("chat_id") or fpayload.get("conversation_id") or "chat_default")
                    if chat_id in self.permanent_error_chats:
                        err = {
                            "type": "Error",
                            "payload": {"code": 403, "message": "CHAT_WRITE_FORBIDDEN", "client_nonce": fpayload.get("client_nonce")},
                        }
                        writer.write(_encode_ws_frame(json.dumps(err).encode("utf-8")))
                        await writer.drain()
                        continue
                    stored = self._store_message(account_id, chat_id, fpayload)
                    if self.drop_ws_before_ack_once:
                        self.drop_ws_before_ack_once = False
                        writer.close()
                        break
                    ack = {
                        "type": "Ack",
                        "payload": {
                            "correlation_id": msg.get("correlation_id"),
                            "client_nonce": fpayload.get("client_nonce"),
                            "message_id": stored["id"],
                            "pts": stored["pts"],
                        },
                    }
                    writer.write(_encode_ws_frame(json.dumps(ack).encode("utf-8")))
                    await writer.drain()
                elif ftype == "Sync":
                    from_pts = int(fpayload.get("pts", 0))
                    with self._lock:
                        missed = [u for u in self.updates_by_account.get(account_id, []) if u["pts"] > from_pts]
                        cur_pts = self.pts_by_account.get(account_id, from_pts)
                    diff = {"type": "SyncDiff", "payload": {"pts": cur_pts, "updates": missed}}
                    writer.write(_encode_ws_frame(json.dumps(diff).encode("utf-8")))
                    await writer.drain()
                elif ftype == "CallSignal":
                    with self._lock:
                        self.call_signals.append(fpayload)
                    ack = {"type": "Ack", "payload": {"signal_ok": True}}
                    writer.write(_encode_ws_frame(json.dumps(ack).encode("utf-8")))
                    await writer.drain()
        finally:
            with self._lock:
                lst = self._ws_clients.get(tok, [])
                if writer in lst:
                    lst.remove(writer)

    def _store_message(self, account_id: str, chat_id: str, payload: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            nonce = str(payload.get("client_nonce") or payload.get("clientMessageId") or "")
            if nonce and (account_id, chat_id, nonce) in self.nonce_dedup:
                return self.nonce_dedup[(account_id, chat_id, nonce)]
            pts = self.pts_by_account.get(account_id, 0) + 1
            self.pts_by_account[account_id] = pts
            msg = {
                "id": f"srv_msg_{pts}",
                "chat_id": chat_id,
                "account_id": account_id,
                "client_nonce": nonce,
                "text": payload.get("text") or (payload.get("content", {}) if isinstance(payload.get("content"), dict) else payload.get("content", "")),
                "entities": payload.get("entities", []),
                "topic_id": payload.get("topic_id"),
                "pts": pts,
                "created_at": int(time.time() * 1000),
            }
            self.messages_by_chat.setdefault(chat_id, []).append(msg)
            self.updates_by_account.setdefault(account_id, []).append(
                {"pts": pts, "type": "new_message", "message": msg}
            )
            self.drafts.pop((account_id, chat_id), None)
            if nonce:
                self.nonce_dedup[(account_id, chat_id, nonce)] = msg
            return msg

    def _route_http(
        self,
        method: str,
        path: str,
        query: dict[str, list[str]],
        headers: dict[str, str],
        body: bytes,
        data: Any,
    ) -> tuple[int, Any, str, dict[str, str]]:
        ct_json = "application/json; charset=utf-8"
        payload = data if isinstance(data, dict) else {}

        if path == "/api/v1/status":
            return 200, {"ok": True, "service": "loopback_gpui_telegram_server", "version": "1.2.0"}, ct_json, {}

        # Phone OTP
        if path == "/api/v1/auth/phone/send-code" and method == "POST":
            phone = str(payload.get("phone") or payload.get("phone_number") or "").strip()
            if not phone or not any(ch.isdigit() for ch in phone):
                return 400, {"ok": False, "error": {"code": "PHONE_NUMBER_INVALID", "message": "Invalid phone"}}, ct_json, {}
            code = f"{(abs(hash(phone)) % 899999) + 100000}"
            with self._lock:
                self.otp_by_phone[phone] = code
            return 200, {"ok": True, "phone_code_hash": f"hash_{hashlib.sha1(phone.encode()).hexdigest()[:12]}", "test_otp": code}, ct_json, {}

        if path == "/api/v1/auth/phone/verify" and method == "POST":
            phone = str(payload.get("phone") or payload.get("phone_number") or "").strip()
            code = str(payload.get("code") or payload.get("otp") or "").strip()
            with self._lock:
                expected = self.otp_by_phone.get(phone)
                if not expected or code != expected:
                    return 400, {"ok": False, "error": {"code": "PHONE_CODE_INVALID", "message": "Invalid OTP code"}}, ct_json, {}
                if phone in self.twofa_passwords:
                    _, hint = self.twofa_passwords[phone]
                    tmp_tok = f"2fa_{uuid.uuid4().hex}"
                    self.pending_2fa_tokens[tmp_tok] = {"identifier": phone, "account_id": f"acc_{hashlib.sha1(phone.encode()).hexdigest()[:8]}"}
                    return 200, {"ok": True, "status": "2fa_required", "two_factor_required": True, "hint": hint, "temp_token": tmp_tok}, ct_json, {}
                sess = self._issue_session(f"acc_{hashlib.sha1(phone.encode()).hexdigest()[:8]}", phone)
            return 200, {"ok": True, "status": "authorized", **sess}, ct_json, {}

        # Email OTP
        if path in ("/api/v1/auth/email/send-code", "/api/v1/auth/email/register") and method == "POST":
            email = str(payload.get("email") or "").strip()
            if "@" not in email:
                return 400, {"ok": False, "error": {"code": "EMAIL_INVALID", "message": "Invalid email"}}, ct_json, {}
            code = "654321"
            with self._lock:
                self.otp_by_email[email] = code
            return 200, {"ok": True, "email_code_hash": f"ehash_{email}", "test_otp": code}, ct_json, {}

        if path == "/api/v1/auth/email/verify" and method == "POST":
            email = str(payload.get("email") or "").strip()
            code = str(payload.get("code") or "").strip()
            with self._lock:
                if self.otp_by_email.get(email) != code:
                    return 400, {"ok": False, "error": {"code": "EMAIL_CODE_INVALID", "message": "Invalid email code"}}, ct_json, {}
                sess = self._issue_session(f"acc_{hashlib.sha1(email.encode()).hexdigest()[:8]}", email)
            return 200, {"ok": True, "status": "authorized", **sess}, ct_json, {}

        # QR Pairing
        if path == "/api/v1/auth/pair/init" and method == "POST":
            pair_tok = f"qr_{uuid.uuid4().hex[:16]}"
            with self._lock:
                self.qr_pairs[pair_tok] = {"status": "pending", "session": None, "expired": False}
            return 200, {"ok": True, "pair_token": pair_tok, "qr_uri": f"tg://login?token={pair_tok}"}, ct_json, {}

        if path.startswith("/api/v1/auth/pair/poll/") and method == "GET":
            pair_tok = path.rsplit("/", 1)[-1]
            with self._lock:
                st = self.qr_pairs.get(pair_tok)
                if not st:
                    return 404, {"ok": False, "error": {"code": "PAIR_TOKEN_NOT_FOUND", "message": "Unknown QR token"}}, ct_json, {}
                if st.get("expired"):
                    return 410, {"ok": False, "error": {"code": "PAIR_TOKEN_EXPIRED", "message": "QR token expired"}}, ct_json, {}
                if st["status"] == "authorized" and st["session"]:
                    return 200, {"ok": True, "status": "authorized", **st["session"]}, ct_json, {}
                return 200, {"ok": True, "status": "pending"}, ct_json, {}

        if path in ("/api/v1/auth/pair/accept", "/api/v1/auth/pair/verify") and method == "POST":
            pair_tok = str(payload.get("pair_token") or payload.get("token") or "")
            sess = self._auth_session_from_headers(headers, query)
            with self._lock:
                st = self.qr_pairs.get(pair_tok)
                if not st:
                    return 404, {"ok": False, "error": {"code": "PAIR_TOKEN_NOT_FOUND", "message": "Unknown QR token"}}, ct_json, {}
                acct = sess["account_id"] if sess else str(payload.get("account_id") or "acc_qr_linked")
                ident = sess["identifier"] if sess else "+14155550177"
                linked_sess = self._issue_session(acct, ident, device_name="QR Linked Device")
                st["status"] = "authorized"
                st["session"] = linked_sess
            return 200, {"ok": True, "status": "authorized", **linked_sess}, ct_json, {}

        # 2FA
        if path == "/api/v1/auth/2fa/verify" and method == "POST":
            pw = str(payload.get("password") or "")
            tmp_tok = str(payload.get("temp_token") or "")
            ident = str(payload.get("phone") or payload.get("identifier") or "")
            with self._lock:
                if tmp_tok in self.pending_2fa_tokens:
                    ident = self.pending_2fa_tokens[tmp_tok]["identifier"]
                expected = self.twofa_passwords.get(ident)
                if not expected or expected[0] != pw:
                    return 403, {"ok": False, "error": {"code": "PASSWORD_HASH_INVALID", "message": "Invalid 2FA password"}}, ct_json, {}
                sess = self._issue_session(f"acc_{hashlib.sha1(ident.encode()).hexdigest()[:8]}", ident)
            return 200, {"ok": True, "status": "authorized", **sess}, ct_json, {}

        if path == "/api/v1/auth/2fa/status" and method == "GET":
            sess = self._auth_session_from_headers(headers, query)
            ident = sess["identifier"] if sess else ""
            enabled = ident in self.twofa_passwords
            hint = self.twofa_passwords[ident][1] if enabled else ""
            return 200, {"ok": True, "enabled": enabled, "hint": hint}, ct_json, {}

        if path == "/api/v1/auth/2fa/set" and method == "POST":
            sess = self._auth_session_from_headers(headers, query)
            ident = sess["identifier"] if sess else str(payload.get("identifier") or "default")
            pw = str(payload.get("password") or "")
            hint = str(payload.get("hint") or "")
            with self._lock:
                self.twofa_passwords[ident] = (pw, hint)
            return 200, {"ok": True, "enabled": True, "hint": hint}, ct_json, {}

        if path == "/api/v1/auth/2fa/disable" and method == "POST":
            sess = self._auth_session_from_headers(headers, query)
            ident = sess["identifier"] if sess else str(payload.get("identifier") or "default")
            with self._lock:
                self.twofa_passwords.pop(ident, None)
            return 200, {"ok": True, "enabled": False}, ct_json, {}

        # Passkeys
        if path == "/api/v1/auth/passkey/challenge" and method == "POST":
            return 200, {"ok": True, "challenge": base64.urlsafe_b64encode(os.urandom(32)).decode("ascii")}, ct_json, {}

        if path == "/api/v1/auth/passkey/verify" and method == "POST":
            sess = self._issue_session("acc_passkey_user", "passkey_user")
            return 200, {"ok": True, "status": "authorized", **sess}, ct_json, {}

        if path == "/api/v1/auth/passkeys":
            sess = self._auth_session_from_headers(headers, query)
            acct = sess["account_id"] if sess else "acc_default"
            if method == "GET":
                return 200, {"ok": True, "passkeys": self.passkeys_by_account.get(acct, [])}, ct_json, {}
            if method == "POST":
                pk = {"id": f"pk_{uuid.uuid4().hex[:8]}", "label": payload.get("label", "Pixel Key"), "created_at": int(time.time())}
                self.passkeys_by_account.setdefault(acct, []).append(pk)
                return 201, {"ok": True, "passkey": pk}, ct_json, {}

        if path.startswith("/api/v1/auth/passkeys/") and method == "DELETE":
            pk_id = path.rsplit("/", 1)[-1]
            sess = self._auth_session_from_headers(headers, query)
            acct = sess["account_id"] if sess else "acc_default"
            self.passkeys_by_account[acct] = [p for p in self.passkeys_by_account.get(acct, []) if p["id"] != pk_id]
            return 200, {"ok": True}, ct_json, {}

        # Sessions
        if path == "/api/v1/auth/sessions" and method == "GET":
            sess = self._auth_session_from_headers(headers, query)
            if not sess:
                return 401, {"ok": False, "error": {"code": "UNAUTHORIZED", "message": "Invalid token"}}, ct_json, {}
            acct = sess["account_id"]
            with self._lock:
                active = [s for s in self.sessions.values() if s["account_id"] == acct and not s["revoked"]]
            return 200, {"ok": True, "sessions": active}, ct_json, {}

        if path == "/api/v1/auth/sessions/others" and method == "DELETE":
            sess = self._auth_session_from_headers(headers, query)
            if not sess:
                return 401, {"ok": False, "error": {"code": "UNAUTHORIZED", "message": "Invalid token"}}, ct_json, {}
            with self._lock:
                for s in self.sessions.values():
                    if s["account_id"] == sess["account_id"] and s["session_id"] != sess["session_id"]:
                        s["revoked"] = True
            return 200, {"ok": True}, ct_json, {}

        if path.startswith("/api/v1/auth/sessions/") and method == "DELETE":
            target_id = path.rsplit("/", 1)[-1]
            with self._lock:
                found = False
                for s in self.sessions.values():
                    if s["session_id"] == target_id:
                        s["revoked"] = True
                        found = True
                if not found:
                    return 404, {"ok": False, "error": {"code": "SESSION_NOT_FOUND", "message": "Session not found"}}, ct_json, {}
            return 200, {"ok": True}, ct_json, {}

        # Peers & Contacts
        if path == "/api/v1/peers/resolve" and method == "GET":
            uname = (query.get("username") or [""])[0].lstrip("@")
            peer = self.directory_peers.get(uname)
            if not peer:
                return 404, {"ok": False, "error": {"code": "USERNAME_NOT_OCCUPIED", "message": f"@{uname} not found"}}, ct_json, {}
            return 200, {"ok": True, "peer": peer}, ct_json, {}

        if path == "/api/v1/contacts":
            sess = self._auth_session_from_headers(headers, query)
            acct = sess["account_id"] if sess else "acc_default"
            if method == "GET":
                return 200, {"ok": True, "contacts": self.contacts_by_account.get(acct, [])}, ct_json, {}
            if method == "POST":
                contact = {
                    "user_id": payload.get("user_id") or f"u_{uuid.uuid4().hex[:6]}",
                    "username": str(payload.get("username") or "").lstrip("@"),
                    "phone": payload.get("phone") or "",
                    "display_name": payload.get("display_name") or payload.get("username") or "Contact",
                }
                self.contacts_by_account.setdefault(acct, []).append(contact)
                return 201, {"ok": True, "contact": contact}, ct_json, {}

        # Sync get_difference
        if path == "/api/v1/sync/get_difference" and method == "POST":
            sess = self._auth_session_from_headers(headers, query)
            acct = sess["account_id"] if sess else str(payload.get("account_id") or "acc_default")
            from_pts = int(payload.get("pts", 0))
            with self._lock:
                missed = [u for u in self.updates_by_account.get(acct, []) if u["pts"] > from_pts]
                cur_pts = self.pts_by_account.get(acct, from_pts)
            return 200, {"ok": True, "pts": cur_pts, "updates": missed}, ct_json, {}

        # Messages send
        if path == "/api/v1/messages/send" and method == "POST":
            sess = self._auth_session_from_headers(headers, query)
            acct = sess["account_id"] if sess else str(payload.get("account_id") or "acc_default")
            chat_id = str(payload.get("chat_id") or payload.get("conversation_id") or "chat_default")
            if chat_id in self.permanent_error_chats:
                return 403, {"ok": False, "error": {"code": "CHAT_WRITE_FORBIDDEN", "message": "Forbidden"}}, ct_json, {}
            stored = self._store_message(acct, chat_id, payload)
            return 200, {"ok": True, "message": stored, "pts": stored["pts"]}, ct_json, {}

        # Polls vote
        m_poll = re.match(r"^/api/v1/polls/([^/]+)/vote$", path)
        if m_poll and method == "POST":
            poll_id = m_poll.group(1)
            opt_idx = payload.get("option_index")
            if opt_idx is None and isinstance(payload.get("option_ids"), list) and payload["option_ids"]:
                try:
                    opt_idx = int(payload["option_ids"][0])
                except (ValueError, TypeError):
                    return 400, {"ok": False, "error": {"code": "OPTION_INDEX_INVALID", "message": "Expected integer option index"}}, ct_json, {}
            if not isinstance(opt_idx, int) or opt_idx < 0:
                return 400, {"ok": False, "error": {"code": "OPTION_INDEX_INVALID", "message": "Invalid option_index"}}, ct_json, {}
            with self._lock:
                poll = self.polls.setdefault(poll_id, {"poll_id": poll_id, "votes": {}})
                poll["votes"][str(opt_idx)] = poll["votes"].get(str(opt_idx), 0) + 1
            return 200, {"ok": True, "poll": poll, "chosen_option_index": opt_idx}, ct_json, {}

        # Channel stats & export
        m_stats = re.match(r"^/api/v1/chats/([^/]+)/statistics$", path)
        if m_stats and method == "GET":
            cid = m_stats.group(1)
            stats = self.channel_stats.get(cid, {"subscribers": 4219, "views_per_post": 1337, "shares_per_post": 89})
            return 200, {"ok": True, "statistics": stats}, ct_json, {}

        m_export = re.match(r"^/api/v1/chats/([^/]+)/export$", path)
        if m_export and method == "GET":
            cid = m_export.group(1)
            fmt = (query.get("format") or ["json"])[0]
            msgs = self.messages_by_chat.get(cid, [])
            if fmt == "html":
                import html

                items = "".join(f"<li>{html.escape(str(m.get('text', '')))}</li>" for m in msgs)
                doc = f"<!doctype html><html><body><ul>{items}</ul></body></html>".encode("utf-8")
                return 200, doc, "text/html; charset=utf-8", {}
            return 200, {"ok": True, "chat_id": cid, "messages": msgs}, ct_json, {}

        # Media CAS & HTTP 206 Range
        if path == "/api/v1/media/upload" and method == "POST":
            raw_media = body
            if isinstance(data, dict) and "data_b64" in data:
                raw_media = base64.b64decode(data["data_b64"])
            sha = hashlib.sha256(raw_media).hexdigest()
            with self._lock:
                dedup = sha in self.media_blobs
                self.media_blobs[sha] = raw_media
            return 200, {"ok": True, "file_id": sha, "sha256": sha, "size": len(raw_media), "deduplicated": dedup}, ct_json, {}

        if path.startswith("/api/v1/media/") and method == "GET":
            file_id = path.rsplit("/", 1)[-1]
            blob = self.media_blobs.get(file_id)
            if blob is None:
                return 404, {"ok": False, "error": {"code": "MEDIA_NOT_FOUND", "message": "Missing blob"}}, ct_json, {}
            rng = headers.get("range", "")
            m_rng = re.match(r"^bytes=(\d+)-(\d*)$", rng.strip())
            if m_rng:
                start = int(m_rng.group(1))
                end = int(m_rng.group(2)) if m_rng.group(2) else len(blob) - 1
                end = min(end, len(blob) - 1)
                sliced = blob[start : end + 1]
                return (
                    206,
                    sliced,
                    "application/octet-stream",
                    {"Accept-Ranges": "bytes", "Content-Range": f"bytes {start}-{end}/{len(blob)}"},
                )
            return 200, blob, "application/octet-stream", {"Accept-Ranges": "bytes"}

        # E2EE Prekeys
        if path == "/api/v1/e2ee/prekeys" and method == "POST":
            uid = str(payload.get("user_id") or "bob")
            with self._lock:
                self.prekey_bundles[uid] = dict(payload)
            return 200, {"ok": True}, ct_json, {}

        m_pk = re.match(r"^/api/v1/e2ee/devices/([^/]+)/prekeys$", path)
        if m_pk and method == "GET":
            uid = m_pk.group(1)
            with self._lock:
                bundle = self.prekey_bundles.get(uid)
                if not bundle:
                    return 404, {"ok": False, "error": {"code": "PREKEY_NOT_FOUND", "message": "No prekeys"}}, ct_json, {}
                out = dict(bundle)
                otks = list(out.get("one_time_prekeys") or [])
                if otks:
                    out["one_time_prekey"] = otks.pop(0)
                    bundle["one_time_prekeys"] = otks
            return 200, {"ok": True, "bundle": out}, ct_json, {}

        # Calls / WebRTC signaling
        if path.startswith("/api/v1/calls/") and method == "POST":
            with self._lock:
                self.call_signals.append({"path": path, **payload})
            return 200, {"ok": True, "call_id": payload.get("call_id", "call_1")}, ct_json, {}

        if path == "/api/v1/webrtc/turn_credentials" and method == "GET":
            return 200, {"ok": True, "uris": ["stun:127.0.0.1:3478"], "username": "u", "credential": "c"}, ct_json, {}

        return 200, {"ok": True, "echo_path": path}, ct_json, {}


class LoopbackMagpieUpstreamHub(_AsyncLoopbackBase):
    """Multi-protocol LLM SSE + TTS + ASR loopback hub (OpenAI, Responses, Anthropic, Gemini)."""

    def __init__(self, expected_api_key: str = "sk-live-e2e-key") -> None:
        super().__init__()
        self.expected_api_key = expected_api_key
        self.route_faults: dict[str, int] = {}  # substring -> HTTP status (401/429/500)
        self.drop_mid_stream = False
        self.reply_prefix = "LIVE_UPSTREAM_OK"

    def set_fault(self, path_substring: str, status_code: int) -> None:
        with self._lock:
            self.route_faults[path_substring] = status_code

    def clear_faults(self) -> None:
        with self._lock:
            self.route_faults.clear()
            self.drop_mid_stream = False

    def _extract_prompt_text(self, payload: dict[str, Any]) -> str:
        if isinstance(payload.get("messages"), list) and payload["messages"]:
            last = payload["messages"][-1]
            if isinstance(last, dict):
                c = last.get("content", "")
                if isinstance(c, str):
                    return c
                if isinstance(c, list):
                    return " ".join(str(p.get("text", "")) for p in c if isinstance(p, dict))
        if isinstance(payload.get("input"), str):
            return payload["input"]
        if isinstance(payload.get("input"), list) and payload["input"]:
            last = payload["input"][-1]
            if isinstance(last, dict):
                return str(last.get("content", ""))
        if isinstance(payload.get("contents"), list) and payload["contents"]:
            parts = payload["contents"][-1].get("parts", [])
            return " ".join(str(p.get("text", "")) for p in parts if isinstance(p, dict))
        return str(payload.get("prompt") or "ping")

    def _check_auth(self, path: str, headers: dict[str, str], query: dict[str, list[str]]) -> bool:
        if not self.expected_api_key:
            return True
        if ":streamGenerateContent" in path or ":generateContent" in path:
            key = headers.get("x-goog-api-key") or (query.get("key") or [""])[0]
            return key == self.expected_api_key
        if path.endswith("/messages"):
            key = headers.get("x-api-key") or ""
            if key == self.expected_api_key:
                return True
        auth = headers.get("authorization", "")
        if auth.lower().startswith("bearer "):
            return auth[7:].strip() == self.expected_api_key
        return False

    async def _handle_connection(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        req = await _read_http_request(reader)
        if req is None:
            return
        method, raw_target, headers, body = req
        parsed = urllib.parse.urlsplit(raw_target)
        path = parsed.path
        query = urllib.parse.parse_qs(parsed.query)
        body_text = body.decode("utf-8", errors="replace")
        try:
            payload = json.loads(body_text) if body else {}
        except json.JSONDecodeError:
            payload = {}

        with self._lock:
            self.recorded_requests.append(
                {
                    "method": method,
                    "path": path,
                    "query": query,
                    "headers": dict(headers),
                    "body_text": body_text,
                    "json": payload if isinstance(payload, dict) else {},
                }
            )
            for sub, code in self.route_faults.items():
                if sub in path:
                    err_b = json.dumps({"error": {"message": f"Injected HTTP {code}", "type": "upstream_error"}}).encode("utf-8")
                    _write_http_response(writer, code, "Error", err_b)
                    await writer.drain()
                    return

        if not self._check_auth(path, headers, query):
            err_b = json.dumps({"error": {"message": "Invalid or missing API key", "code": "invalid_api_key"}}).encode("utf-8")
            _write_http_response(writer, 401, "Unauthorized", err_b)
            await writer.drain()
            return

        # OpenAI TTS /audio/speech
        if path.endswith("/audio/speech"):
            wav = _build_wav_pcm16_mono()
            _write_http_response(writer, 200, "OK", wav, "audio/wav")
            await writer.drain()
            return

        # Whisper ASR /audio/transcriptions
        if path.endswith("/audio/transcriptions"):
            out = json.dumps({"text": "金刚般若波罗蜜语音转写", "language": "zh"}).encode("utf-8")
            _write_http_response(writer, 200, "OK", out)
            await writer.drain()
            return

        prompt_text = self._extract_prompt_text(payload if isinstance(payload, dict) else {})
        # Preserve any placeholder tokens so PII unmasking round-trips cleanly!
        reply_text = f"{self.reply_prefix}: {prompt_text[:240]}"
        tokens = [f"{self.reply_prefix}: ", prompt_text[:240]]
        stream = bool(payload.get("stream", True)) if isinstance(payload, dict) else True
        if ":generateContent" in path and ":streamGenerateContent" not in path:
            stream = False

        if not stream:
            if ":generateContent" in path:
                # Support Gemini TTS AUDIO modality too
                modalities = ((payload.get("generationConfig") or {}).get("responseModalities") or []) if isinstance(payload, dict) else []
                if "AUDIO" in modalities:
                    pcm_b64 = base64.b64encode(b"\x10\x02" * 2400).decode("ascii")
                    gem_audio = {
                        "candidates": [{"content": {"role": "model", "parts": [{"inlineData": {"mimeType": "audio/L16;rate=24000", "data": pcm_b64}}]}}],
                    }
                    _write_http_response(writer, 200, "OK", json.dumps(gem_audio).encode("utf-8"))
                    await writer.drain()
                    return
                gem = {
                    "candidates": [{"content": {"role": "model", "parts": [{"text": reply_text}]}, "finishReason": "STOP"}],
                    "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 8, "totalTokenCount": 18},
                }
                _write_http_response(writer, 200, "OK", json.dumps(gem).encode("utf-8"))
            elif path.endswith("/messages"):
                ant = {
                    "id": "msg_1",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": reply_text}],
                    "usage": {"input_tokens": 10, "output_tokens": 8},
                }
                _write_http_response(writer, 200, "OK", json.dumps(ant).encode("utf-8"))
            elif path.endswith("/responses"):
                rsp = {
                    "id": "resp_1",
                    "status": "completed",
                    "output": [{"type": "message", "content": [{"type": "output_text", "text": reply_text}]}],
                    "usage": {"input_tokens": 10, "output_tokens": 8, "total_tokens": 18},
                }
                _write_http_response(writer, 200, "OK", json.dumps(rsp).encode("utf-8"))
            else:
                oai = {
                    "id": "chatcmpl-1",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": reply_text}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18},
                }
                _write_http_response(writer, 200, "OK", json.dumps(oai).encode("utf-8"))
            await writer.drain()
            return

        # SSE Streaming
        sse_head = (
            "HTTP/1.1 200 OK\r\n"
            "Content-Type: text/event-stream; charset=utf-8\r\n"
            "Cache-Control: no-cache\r\n"
            "Connection: close\r\n\r\n"
        )
        writer.write(sse_head.encode("iso-8859-1"))
        await writer.drain()

        if self.drop_mid_stream:
            writer.write(b'data: {"id":"cut","choices":[{"index":0,"delta":{"content":"partial"}}]}\n\n')
            await writer.drain()
            writer.close()
            return

        if ":streamGenerateContent" in path:
            for chunk in tokens:
                ev = {
                    "candidates": [{"content": {"role": "model", "parts": [{"text": chunk}]}}],
                    "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 8, "totalTokenCount": 18},
                }
                writer.write(f"data: {json.dumps(ev)}\n\n".encode("utf-8"))
                await writer.drain()
        elif path.endswith("/messages"):
            writer.write(b'event: message_start\ndata: {"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10}}}\n\n')
            for chunk in tokens:
                ev = {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": chunk}}
                writer.write(f"event: content_block_delta\ndata: {json.dumps(ev)}\n\n".encode("utf-8"))
                await writer.drain()
            writer.write(b'event: message_delta\ndata: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":8}}\n\n')
            writer.write(b'event: message_stop\ndata: {"type":"message_stop"}\n\n')
            await writer.drain()
        elif path.endswith("/responses"):
            for chunk in tokens:
                ev = {"delta": chunk}
                writer.write(f"event: response.output_text.delta\ndata: {json.dumps(ev)}\n\n".encode("utf-8"))
                await writer.drain()
            done_ev = {"response": {"status": "completed", "usage": {"input_tokens": 10, "output_tokens": 8, "total_tokens": 18}}}
            writer.write(f"event: response.completed\ndata: {json.dumps(done_ev)}\n\n".encode("utf-8"))
            await writer.drain()
        else:
            for chunk in tokens:
                ev = {"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": None}]}
                writer.write(f"data: {json.dumps(ev)}\n\n".encode("utf-8"))
                await writer.drain()
            fin = {
                "id": "chatcmpl-1",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18},
            }
            writer.write(f"data: {json.dumps(fin)}\n\ndata: [DONE]\n\n".encode("utf-8"))
            await writer.drain()


class LoopbackTonRpcServer(_AsyncLoopbackBase):
    """Stateful TON HTTP v2 REST & JSON-RPC endpoint validating BOC magic 0xb5ee9c72."""

    BOC_MAGIC = b"\xb5\xee\x9c\x72"

    def __init__(self) -> None:
        super().__init__()
        self.balances: dict[str, int] = {}
        self.seqnos: dict[str, int] = {}
        self.submitted_bocs: list[dict[str, Any]] = []
        self.http_error_code: int = 0

    def fund_address(self, address: str, nanotons: int) -> None:
        with self._lock:
            self.balances[address] = max(0, nanotons)

    async def _handle_connection(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        req = await _read_http_request(reader)
        if req is None:
            return
        method, raw_target, headers, body = req
        parsed = urllib.parse.urlsplit(raw_target)
        path = parsed.path
        query = urllib.parse.parse_qs(parsed.query)
        payload = json.loads(body.decode("utf-8", errors="replace")) if body else {}

        with self._lock:
            self.recorded_requests.append({"method": method, "path": path, "query": query, "json": payload})
            if self.http_error_code:
                err = json.dumps({"ok": False, "error": f"Injected TON RPC {self.http_error_code}"}).encode("utf-8")
                _write_http_response(writer, self.http_error_code, "Error", err)
                await writer.drain()
                return

        if path.endswith("/getAddressInformation"):
            addr = (query.get("address") or [""])[0]
            with self._lock:
                bal = self.balances.get(addr, 0)
                seq = self.seqnos.get(addr, 0)
            resp = {"ok": True, "result": {"balance": str(bal), "state": "active" if bal > 0 else "uninitialized", "seqno": seq}}
            _write_http_response(writer, 200, "OK", json.dumps(resp).encode("utf-8"))
            await writer.drain()
            return

        if path.endswith("/getTransactions"):
            addr = (query.get("address") or [""])[0]
            with self._lock:
                txs = [tx for tx in self.submitted_bocs if tx.get("address") in ("", addr)]
            _write_http_response(writer, 200, "OK", json.dumps({"ok": True, "result": txs}).encode("utf-8"))
            await writer.drain()
            return

        if path.endswith("/sendBoc") or path.endswith("/sendBocReturnHash") or path.endswith("/jsonRPC"):
            boc_b64 = ""
            if isinstance(payload, dict):
                if payload.get("method") == "getAddressInformation":
                    addr = str((payload.get("params") or {}).get("address") or "")
                    bal = self.balances.get(addr, 0)
                    _write_http_response(writer, 200, "OK", json.dumps({"ok": True, "result": {"balance": str(bal), "seqno": self.seqnos.get(addr, 0)}}).encode("utf-8"))
                    await writer.drain()
                    return
                boc_b64 = str(payload.get("boc") or (payload.get("params") or {}).get("boc") or "")
            try:
                raw_boc = base64.b64decode(boc_b64, validate=True)
            except Exception:  # noqa: BLE001
                _write_http_response(writer, 400, "Bad Request", json.dumps({"ok": False, "error": "Invalid base64 BOC"}).encode("utf-8"))
                await writer.drain()
                return
            if len(raw_boc) < 68 or not raw_boc.startswith(self.BOC_MAGIC):
                _write_http_response(writer, 400, "Bad Request", json.dumps({"ok": False, "error": "Missing 0xb5ee9c72 BOC magic or Ed25519 signature"}).encode("utf-8"))
                await writer.drain()
                return
            tx_hash = hashlib.sha256(raw_boc).hexdigest()
            addr = str(payload.get("address") or "")
            amt = int(payload.get("amount_nanotons") or 0)
            with self._lock:
                if addr and addr in self.balances and amt > 0:
                    self.balances[addr] = max(0, self.balances[addr] - amt)
                if addr:
                    self.seqnos[addr] = self.seqnos.get(addr, 0) + 1
                elif self.balances:
                    first_addr = next(iter(self.balances))
                    self.seqnos[first_addr] = self.seqnos.get(first_addr, 0) + 1
                self.submitted_bocs.append({"hash": tx_hash, "size": len(raw_boc), "address": addr})
            _write_http_response(writer, 200, "OK", json.dumps({"ok": True, "result": {"hash": tx_hash}}).encode("utf-8"))
            await writer.drain()
            return

        _write_http_response(writer, 200, "OK", json.dumps({"ok": True}).encode("utf-8"))
        await writer.drain()


class LoopbackMarketplaceMcpServer(_AsyncLoopbackBase):
    """Loopback Streamable HTTP WebMCP + Global Dharma entitlements + TinyFish WebFetch/Search."""

    def __init__(self) -> None:
        super().__init__()
        self.prayer_wheel_entitled = False
        self.force_401 = False

    async def _handle_connection(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        req = await _read_http_request(reader)
        if req is None:
            return
        method, raw_target, headers, body = req
        parsed = urllib.parse.urlsplit(raw_target)
        path = parsed.path
        query = urllib.parse.parse_qs(parsed.query)
        payload = json.loads(body.decode("utf-8", errors="replace")) if body else {}

        with self._lock:
            self.recorded_requests.append({"method": method, "path": path, "headers": dict(headers), "json": payload})
            if self.force_401:
                _write_http_response(writer, 401, "Unauthorized", json.dumps({"error": "unauthorized"}).encode("utf-8"))
                await writer.drain()
                return

        if path.endswith("/entitlements/local.prayer-wheel.start"):
            allowed = self.prayer_wheel_entitled
            resp = {
                "ok": True,
                "access": {
                    "capability": "local.prayer-wheel.start",
                    "protected": True,
                    "allowed": allowed,
                    "reason": "entitled" if allowed else "not_entitled",
                    "sku": "local-prayer-wheel.lifetime",
                    "price_minor": 108000,
                    "currency": "CNY",
                },
            }
            _write_http_response(writer, 200, "OK", json.dumps(resp).encode("utf-8"))
            await writer.drain()
            return

        if path.endswith("/commerce/purchase") and method == "POST":
            with self._lock:
                self.prayer_wheel_entitled = True
            _write_http_response(writer, 200, "OK", json.dumps({"ok": True, "entitled": True, "sku": "local-prayer-wheel.lifetime"}).encode("utf-8"))
            await writer.drain()
            return

        if path.startswith("/api/mcp/apps/"):
            rpc_id = payload.get("id", 1) if isinstance(payload, dict) else 1
            rpc_method = payload.get("method", "") if isinstance(payload, dict) else ""
            params = payload.get("params") or {} if isinstance(payload, dict) else {}
            if rpc_method == "tools/call" and params.get("name") == "local.prayer-wheel.start":
                if not self.prayer_wheel_entitled:
                    res = {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": -32001, "message": "not_entitled"}}
                    _write_http_response(writer, 200, "OK", json.dumps(res).encode("utf-8"))
                    await writer.drain()
                    return
            res = {"jsonrpc": "2.0", "id": rpc_id, "result": {"content": [{"type": "text", "text": "MCP_OK"}], "entitled": self.prayer_wheel_entitled}}
            _write_http_response(writer, 200, "OK", json.dumps(res).encode("utf-8"))
            await writer.drain()
            return

        if path == "/v1/marketplace/apps":
            apps = [{"id": "global-dharma", "title": "全球法布施", "category": "dharma", "countries": 242}]
            _write_http_response(writer, 200, "OK", json.dumps({"ok": True, "apps": apps}).encode("utf-8"))
            await writer.drain()
            return

        # TinyFish WebFetch & Search routes
        if method == "POST" and ("urls" in payload if isinstance(payload, dict) else False):
            urls = payload.get("urls") or []
            results = [{"url": u, "title": "Dharma Article", "content": "# Heart Sutra\nForm is emptiness."} for u in urls]
            _write_http_response(writer, 200, "OK", json.dumps({"results": results, "errors": []}).encode("utf-8"))
            await writer.drain()
            return

        if method == "GET" and "query" in query:
            q_str = query["query"][0]
            hits = [{"url": "https://example.com/sutra", "title": f"Result for {q_str}", "snippet": "Prajnaparamita summary"}]
            _write_http_response(writer, 200, "OK", json.dumps({"results": hits}).encode("utf-8"))
            await writer.drain()
            return

        _write_http_response(writer, 200, "OK", json.dumps({"ok": True}).encode("utf-8"))
        await writer.drain()
