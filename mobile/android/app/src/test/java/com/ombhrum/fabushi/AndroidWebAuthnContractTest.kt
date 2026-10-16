package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.webauthn.AndroidCredentialManagerWebAuthn
import com.ombhrum.fabushi.androidmain.webauthn.AndroidWebAuthnCeremony
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AndroidWebAuthnContractTest {
    @Test
    fun ceremonyFrameRequiresTypedKindOriginAndValidPayloadJson() {
        val parsed = AndroidWebAuthnCeremony.fromFrame(
            JSONObject()
                .put("kind", "ceremony")
                .put("requestId", "request-1")
                .put(
                    "ceremony",
                    JSONObject()
                        .put("kind", "get")
                        .put("origin", "https://example.com")
                        .put("payloadJson", "{\"challenge\":\"abc\"}"),
                ),
        )
        assertEquals("request-1", parsed?.requestId)
        assertEquals("get", parsed?.kind)

        assertNull(
            AndroidWebAuthnCeremony.fromFrame(
                JSONObject()
                    .put("kind", "ceremony")
                    .put("requestId", "request-2")
                    .put(
                        "ceremony",
                        JSONObject()
                            .put("kind", "unknown")
                            .put("origin", "https://example.com")
                            .put("payloadJson", "{}"),
                    ),
            ),
        )
    }

    @Test
    fun nativePasskeyStallMatchesDesktopDeadlineAndFailsClosed() {
        assertEquals(60_000L, AndroidCredentialManagerWebAuthn.PASSKEY_STALL_MS)

        val ceremony = AndroidWebAuthnCeremony(
            requestId = "request-stalled",
            kind = "get",
            origin = "https://cursor.com",
            payloadJson = "{}",
        )
        val frames = AndroidCredentialManagerWebAuthn.passkeyStallFrames(ceremony)
        assertEquals(2, frames.size)
        assertEquals("stage", frames[0].getString("kind"))
        assertEquals("sign", frames[0].getString("stage"))
        assertEquals("failed", frames[0].getString("outcome"))
        assertEquals("error", frames[1].getString("kind"))
        assertEquals("request-stalled", frames[1].getString("requestId"))
        assertEquals("NotAllowedError", frames[1].getString("name"))
        assertEquals("passkey_stall", frames[1].getString("code"))
    }

    @Test
    fun stageFramesUseOnlySharedGrantSignAndOutcomeVocabulary() {
        val frame = AndroidCredentialManagerWebAuthn.stageFrame(
            requestId = "request-1",
            stage = "sign",
            outcome = "ok",
        )
        assertEquals("stage", frame.getString("kind"))
        assertEquals("request-1", frame.getString("requestId"))
        assertEquals("sign", frame.getString("stage"))
        assertEquals("ok", frame.getString("outcome"))
    }
}
