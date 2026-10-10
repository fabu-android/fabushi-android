package com.ombhrum.fabushi.androidmain.security

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test

class RemoteControlSessionCredentialPolicyTest {
    private val validToken = "m".repeat(64)

    @Test
    fun roundTripKeepsMobileCredentialSecretFromPresentationProjection() {
        val credential = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )

        val parsed = RemoteControlSessionCredential.parse(credential.toSecretJson())
        assertEquals(credential, parsed)
        val projection = parsed.publicProjection()
        assertEquals("computer-1", projection.getString("deviceId"))
        assertEquals("remote-session-1", projection.getString("sessionId"))
        assertFalse(projection.has("mobileToken"))
        assertFalse(projection.has("accountFence"))
    }

    @Test
    fun rejectsCredentialPlaneConfusionAndUnknownSecretFields() {
        val base = JSONObject()
            .put("deviceId", "computer-1")
            .put("clientId", "remote-client-1")
            .put("sessionId", "remote-session-1")
            .put("mobileToken", validToken)
            .put("accountFence", "session:account-1")
            .put("accountEpoch", 7)
            .put("expiresAt", 2_000_000_000)

        val wrongClientPlane = JSONObject(base.toString())
        wrongClientPlane.remove("mobileToken")
        wrongClientPlane.put("clientToken", validToken)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(wrongClientPlane.toString())
        }

        val wrongExecutorPlane = JSONObject(base.toString())
            .put("bearerCredential", validToken)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(wrongExecutorPlane.toString())
        }
    }

    @Test
    fun rejectsInvalidEpochExpiryAndControlCharacters() {
        val credential = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )

        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("accountEpoch", 0).toString(),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("expiresAt", 0).toString(),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("sessionId", "bad\nidentity").toString(),
            )
        }
    }
}
