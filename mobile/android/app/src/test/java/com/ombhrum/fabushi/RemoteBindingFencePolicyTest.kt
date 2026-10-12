package com.ombhrum.fabushi.androidmain.security

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteBindingFencePolicyTest {
    private fun binding(accountFence: String): String =
        """
        {
          "credentialPlane":"authorized-remote-runner-v1",
          "credentialId":"runner-credential-1",
          "issuedAtMs":1,
          "expiresAtMs":4102444800000,
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
    fun credentialLifecycleAndRotationFailClosed() {
        val current = binding("session:stable")
        assertTrue(RemoteBindingLifecyclePolicy.isActive(current, 1L))
        assertTrue(RemoteBindingLifecyclePolicy.isActive(current, 4102444799999L))
        assertFalse(RemoteBindingLifecyclePolicy.isActive(current, 4102444800000L))

        val future = JSONObject(current)
            .put("credentialId", "runner-credential-future")
            .put("issuedAtMs", 200L)
            .put("expiresAtMs", 500L)
            .toString()
        assertFalse(RemoteBindingLifecyclePolicy.isActive(future, 199L))
        assertTrue(RemoteBindingLifecyclePolicy.isActive(future, 200L))

        assertTrue(RemoteBindingRotationPolicy.canReplace(current, current))
        val changedMaterialSameId = JSONObject(current)
            .put("bearerCredential", "different-remote-secret-token-1234")
            .toString()
        assertFalse(RemoteBindingRotationPolicy.canReplace(current, changedMaterialSameId))

        val rollback = JSONObject(current)
            .put("credentialId", "runner-credential-old")
            .put("issuedAtMs", 1L)
            .put("expiresAtMs", 4102444801000L)
            .toString()
        assertFalse(RemoteBindingRotationPolicy.canReplace(current, rollback))

        val newer = JSONObject(current)
            .put("credentialId", "runner-credential-2")
            .put("issuedAtMs", 2L)
            .put("expiresAtMs", 4102444801000L)
            .toString()
        assertTrue(RemoteBindingRotationPolicy.canReplace(current, newer))

        val switchedAccount = JSONObject(newer)
            .put("accountFence", "session:other")
            .toString()
        assertFalse(RemoteBindingRotationPolicy.canReplace(current, switchedAccount))
    }

    @Test
    fun browserExecutorRequiresTheCanonicalRemoteRunnerCredentialPlane() {
        val browser = JSONObject(binding("session:stable"))
            .put("executors", JSONArray().put("browser"))
            .toString()
        RemoteBindingCredentialContract.validate(browser)

        val unknown = JSONObject(binding("session:stable"))
            .put("executors", JSONArray().put("clipboard"))
            .toString()
        assertFalse(runCatching { RemoteBindingCredentialContract.validate(unknown) }.isSuccess)
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
