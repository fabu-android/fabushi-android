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
