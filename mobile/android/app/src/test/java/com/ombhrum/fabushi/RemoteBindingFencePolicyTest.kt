package com.ombhrum.fabushi.androidmain.security

import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteBindingFencePolicyTest {
    private fun binding(accountFence: String): String =
        """
        {
          "credentialPlane":"authorized-remote-runner-v1",
          "endpoint":"https://remote.example.test",
          "bearerCredential":"remote-secret-token-1234",
          "deviceId":"desktop-1",
          "accountFence":"$accountFence",
          "accountEpoch":7,
          "executors":["computer","screenshot"]
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
    fun executorCredentialPlaneIsRequiredAndControlPlaneTokensCannotBeRelabelled() {
        RemoteBindingCredentialContract.validate(binding("session:stable"))

        for (wrongPlane in listOf(
            "computer-client-token-v1",
            "computer-mobile-token-v1",
            "computer-device-secret-v1",
            "codex-remote-control-token-v1",
        )) {
            val wrong = binding("session:stable")
                .replace("authorized-remote-runner-v1", wrongPlane)
            assertFalse(runCatching { RemoteBindingCredentialContract.validate(wrong) }.isSuccess)
        }

        val missingPlane = JSONObject(binding("session:stable"))
            .apply { remove("credentialPlane") }
            .toString()
        assertFalse(runCatching { RemoteBindingCredentialContract.validate(missingPlane) }.isSuccess)
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
