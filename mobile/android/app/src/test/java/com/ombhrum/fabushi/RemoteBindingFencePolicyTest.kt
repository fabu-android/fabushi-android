package com.ombhrum.fabushi.androidmain.security

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteBindingFencePolicyTest {
    private fun binding(accountFence: String): String =
        """
        {
          "endpoint":"https://remote.example.test",
          "bearerCredential":"remote-secret-token-1234",
          "deviceId":"desktop-1",
          "accountFence":"$accountFence",
          "accountEpoch":7,
          "hasDesktop":true
        }
        """.trimIndent()

    @Test
    fun tokenRefreshWithSameCanonicalFenceKeepsProtectedBinding() {
        assertTrue(
            RemoteBindingFencePolicy.matches(
                binding("session:stable"),
                "session:stable",
            ),
        )
    }

    @Test
    fun accountOrSessionSwitchRejectsOldProtectedBinding() {
        assertFalse(
            RemoteBindingFencePolicy.matches(
                binding("session:old"),
                "session:new",
            ),
        )
    }

    @Test
    fun malformedOrHostileFenceFailsClosed() {
        assertFalse(RemoteBindingFencePolicy.matches("not-json", "session:new"))
        assertFalse(RemoteBindingFencePolicy.matches(binding("session:new"), ""))
        assertFalse(
            RemoteBindingFencePolicy.matches(
                binding("session:new"),
                "session:new\nforged",
            ),
        )
    }
}
