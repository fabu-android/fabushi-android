package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteComputerPresentationPolicyTest {
    @Test
    fun parsesOnlyPublicComputerAndPairingProjection() {
        val state = RemoteComputerPresentationPolicy.parse(
            JSONObject()
                .put(
                    "computers",
                    JSONArray().put(
                        JSONObject()
                            .put("deviceId", "computer-1")
                            .put("label", "Work Mac")
                            .put("provider", "fabushi")
                            .put("platform", "macos")
                            .put("appVersion", "1.2.3")
                            .put("capabilities", JSONArray().put("display").put("input"))
                            .put("activeSessionCount", 1)
                            .put("lastSeenAt", 1234)
                            .put("online", true),
                    ),
                ),
            JSONObject()
                .put("paired", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("accountEpoch", 7),
        )

        assertEquals(1, state.computers.size)
        assertEquals("Work Mac", state.computers.single().label)
        assertTrue(state.computers.single().online)
        assertEquals(setOf("display", "input"), state.computers.single().capabilities)
        assertEquals("android-client-1", state.pairing?.clientId)
        assertEquals(7L, state.pairing?.accountEpoch)
    }

    @Test
    fun unpairedProjectionDoesNotInventClientIdentity() {
        val state = RemoteComputerPresentationPolicy.parse(
            JSONObject().put("computers", JSONArray()),
            JSONObject().put("paired", false),
        )
        assertTrue(state.computers.isEmpty())
        assertNull(state.pairing)
    }

    @Test
    fun rejectsAnySecretCredentialBeforePresentation() {
        for (key in listOf(
            "clientToken",
            "mobileToken",
            "deviceSecret",
            "bearerCredential",
            "remote_control_token",
            "credential",
        )) {
            val pairing = JSONObject()
                .put("paired", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("accountEpoch", 7)
                .put(key, "secret")
            assertThrows(IllegalArgumentException::class.java) {
                RemoteComputerPresentationPolicy.parse(
                    JSONObject().put("computers", JSONArray()),
                    pairing,
                )
            }
        }
    }

    @Test
    fun pairingCodeAndLabelMatchCoordinatorContract() {
        assertEquals(
            "A1B2C3D4E5F6",
            RemoteComputerPresentationPolicy.normalizePairingCode(" a1b2c3d4e5f6 "),
        )
        assertNull(RemoteComputerPresentationPolicy.normalizePairingCode("A1B2-C3D4-E5F6"))
        assertNull(RemoteComputerPresentationPolicy.normalizePairingCode("G1B2C3D4E5F6"))
        assertEquals(
            "Fabushi Android",
            RemoteComputerPresentationPolicy.normalizePairingLabel(" Fabushi Android "),
        )
        assertNull(RemoteComputerPresentationPolicy.normalizePairingLabel(" "))
        assertFalse(RemoteComputerPresentationPolicy.normalizePairingLabel("bad\nlabel") != null)
    }
}
