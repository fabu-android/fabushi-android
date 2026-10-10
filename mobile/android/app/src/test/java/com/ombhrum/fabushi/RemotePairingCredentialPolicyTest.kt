package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.security.RemotePairingCredential
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test

class RemotePairingCredentialPolicyTest {
    @Test
    fun controlPlaneCredentialRoundTripsWithoutBecomingExecutorBinding() {
        val credential = RemotePairingCredential.parse(
            """{"deviceId":"remote-device-1","clientId":"remote-client-1","clientToken":"123456789012345678901234567890123456789012345678","accountFence":"session:abc","accountEpoch":7}""",
        )
        assertEquals("remote-device-1", credential.deviceId)
        assertEquals("remote-client-1", credential.clientId)
        assertEquals(7L, credential.accountEpoch)
        val public = credential.publicProjection()
        assertFalse(public.has("clientToken"))
        assertFalse(public.has("bearerCredential"))
        assertFalse(public.has("executors"))
    }

    @Test
    fun rejectsShortTokenAndExecutorCredentialConfusion() {
        assertThrows(IllegalArgumentException::class.java) {
            RemotePairingCredential.parse(
                """{"deviceId":"remote-device-1","clientId":"remote-client-1","clientToken":"too-short","accountFence":"session:abc","accountEpoch":7}""",
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemotePairingCredential.parse(
                """{"deviceId":"remote-device-1","clientId":"remote-client-1","clientToken":"123456789012345678901234567890123456789012345678","accountFence":"session:abc","accountEpoch":7,"bearerCredential":"forbidden"}""",
            )
        }
    }

    @Test
    fun rejectsInvalidEpochAndControlCharacters() {
        assertThrows(IllegalArgumentException::class.java) {
            RemotePairingCredential.parse(
                """{"deviceId":"remote-device-1","clientId":"remote-client-1","clientToken":"123456789012345678901234567890123456789012345678","accountFence":"session:abc","accountEpoch":0}""",
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemotePairingCredential.parse(
                "{\"deviceId\":\"remote\\ndevice\",\"clientId\":\"remote-client-1\",\"clientToken\":\"123456789012345678901234567890123456789012345678\",\"accountFence\":\"session:abc\",\"accountEpoch\":7}",
            )
        }
    }
}
