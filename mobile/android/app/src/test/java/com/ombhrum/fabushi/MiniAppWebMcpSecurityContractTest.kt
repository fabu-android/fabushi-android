package com.ombhrum.fabushi

import androidx.lifecycle.Lifecycle
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Additive AND-GD-01..05 security & pure-JVM seam verification alongside untouched
 * [MiniAppWebMcpSurfaceTest].
 */
class MiniAppWebMcpSecurityContractTest {

    private fun samplePlugin(tools: List<MiniAppToolContract>): MarketplacePlugin = MarketplacePlugin(
        pluginId = "global-dharma",
        displayName = "全球法布施",
        description = "MiniApp WebMCP test plugin",
        latestVersion = "2.3.0",
        tools = tools,
    )

    @Test
    fun accountAndEventProjectionsStripAllCredentialsAndNestedObjects() {
        // AND-GD-01
        val rawAuth = JSONObject()
            .put("loggedIn", true)
            .put("token", "secret-jwt-token")
            .put("accessToken", "secret-access-token")
            .put(
                "user",
                JSONObject()
                    .put("id", "u42")
                    .put("username", "dharma_user")
                    .put("password", "hunter2")
                    .put("privateKey", "ed25519-priv")
                    .put("sessionId", "sess-secret")
                    .put("localPath", "/data/data/secret"),
            )
        val projectedAcc = miniAppAccountProjection(rawAuth)
        assertTrue(projectedAcc.getBoolean("loggedIn"))
        assertFalse(projectedAcc.has("token"))
        assertFalse(projectedAcc.has("accessToken"))
        val user = projectedAcc.getJSONObject("user")
        assertEquals("u42", user.getString("id"))
        assertEquals("dharma_user", user.getString("username"))
        assertFalse(user.has("password"))
        assertFalse(user.has("privateKey"))
        assertFalse(user.has("sessionId"))
        assertFalse(user.has("localPath"))

        val rawEvent = JSONObject()
            .put("type", "operation.progress")
            .put("progress", 50)
            .put("token", "leak-token")
            .put("credentials", JSONObject().put("secret", "nope"))
        val projectedEv = miniAppEventProjection(rawEvent)
        assertEquals("operation.progress", projectedEv.getString("type"))
        assertEquals(50, projectedEv.getInt("progress"))
        assertFalse(projectedEv.has("token"))
        assertFalse(projectedEv.has("credentials"))
    }

    @Test
    fun toolContractJsonPropagatesAnnotationsReadOnlyHintAndCapabilityProbe() {
        // AND-GD-02 & AND-GD-03
        val plugin = samplePlugin(
            listOf(
                MiniAppToolContract(name = "state.read", description = "Read state", approval = "none", title = "Read", inputSchemaJson = """{"type":"object"}"""),
                MiniAppToolContract(name = "transfer.send", description = "Send transfer", approval = "confirm", title = "Send"),
            ),
        )
        val rows = JSONArray(toolContractJson(plugin))
        assertEquals(3, rows.length())
        val probe = rows.getJSONObject(0)
        assertEquals("offline.capabilities", probe.getString("name"))
        assertTrue(probe.getJSONObject("annotations").getBoolean("readOnlyHint"))

        val readTool = rows.getJSONObject(1)
        assertEquals("state.read", readTool.getString("name"))
        assertTrue(readTool.getBoolean("readOnlyHint"))
        assertTrue(readTool.getJSONObject("annotations").getBoolean("readOnlyHint"))

        val sendTool = rows.getJSONObject(2)
        assertEquals("transfer.send", sendTool.getString("name"))
        assertFalse(sendTool.getBoolean("readOnlyHint"))
        assertFalse(sendTool.getJSONObject("annotations").getBoolean("readOnlyHint"))

        val emptyAnswer = miniAppCapabilityAnswer(emptyList())
        assertEquals("fabushi.miniapp.tools.v1", emptyAnswer.getString("contract"))
        assertEquals("android", emptyAnswer.getString("platform"))
        assertEquals(0, emptyAnswer.getJSONArray("capabilities").length())

        val html = injectLocalWebMcp("<html><head></head><body>hi</body></html>", plugin, JSONObject().put("loggedIn", false))
        assertTrue(html.contains("offline.capabilities"))
        assertTrue(html.contains("__fabushiWebMcp"))
    }

    @Test
    fun nativeBridgeWithInjectableApprovalPromptEnforcesSecurityBoundariesOnPureJvm() {
        // AND-GD-04
        val app = M3TestApplication()
        try {
            val plugin = samplePlugin(
                listOf(
                    MiniAppToolContract(name = "state.get", description = "Get state", approval = "none", title = "Get"),
                    MiniAppToolContract(name = "transfer.exec", description = "Mutate state", approval = "confirm", title = "Exec"),
                ),
            )
            var active = true
            var approveDecision = false
            var executed = 0

            val bridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { active },
                callRuntimeToolJson = { _, _, _ ->
                    executed++
                    """{"done":true}"""
                },
                approvalPrompt = { approveDecision },
            )

            // Undeclared tool rejected
            val undeclared = JSONObject(bridge.callTool("shell.exec", "{}"))
            assertFalse(undeclared.getBoolean("ok"))

            // Read-only tool succeeds without approval
            val readOk = JSONObject(bridge.callTool("state.get", "{}"))
            assertTrue(readOk.getBoolean("ok"))
            assertEquals(1, executed)

            // Mutating tool declined via approvalPrompt fails immediately without executing
            approveDecision = false
            val declined = JSONObject(bridge.callTool("transfer.exec", "{}"))
            assertFalse(declined.getBoolean("ok"))
            assertEquals(1, executed)

            // Mutating tool approved via approvalPrompt executes
            approveDecision = true
            val approved = JSONObject(bridge.callTool("transfer.exec", "{}"))
            assertTrue(approved.getBoolean("ok"))
            assertEquals(2, executed)

            // Non-local / inactive document rejected
            active = false
            val inactive = JSONObject(bridge.callTool("state.get", "{}"))
            assertFalse(inactive.getBoolean("ok"))
            assertEquals(2, executed)
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun allowedMiniAppUrlsAndLifecycleGateRevokeOnPauseStopAndDispose() {
        // AND-GD-05
        assertTrue(isAllowedMiniAppUrl("https://fabushi.ombhrum.com/miniapps/global-dharma/"))
        assertTrue(isAllowedMiniAppUrl("https://miniapp.local.fabushi.invalid/miniapps/global-dharma/"))
        assertFalse(isAllowedMiniAppUrl("http://fabushi.ombhrum.com/miniapps/global-dharma/"))
        assertFalse(isAllowedMiniAppUrl("file:///etc/passwd"))
        assertFalse(isAllowedMiniAppUrl("javascript:alert(1)"))
        assertFalse(isAllowedMiniAppUrl("https://fabushi.ombhrum.com@evil.example/"))
        assertFalse(isAllowedMiniAppUrl("https://fabushi.ombhrum.com:8443/miniapps/"))

        val gate = MiniAppLifecycleGate()
        assertFalse(gate.localDocumentActive)
        gate.onLocalHtmlLoaded()
        assertTrue(gate.localDocumentActive)

        var delivered = 0
        assertTrue(gate.dispatchHostEvent(JSONObject().put("type", "ping").put("token", "secret")) { ev ->
            assertFalse(ev.has("token"))
            delivered++
        })
        assertEquals(1, delivered)

        gate.onLifecycleEvent(Lifecycle.Event.ON_PAUSE)
        assertFalse(gate.localDocumentActive)
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "late")) { delivered++ })
        assertEquals(1, delivered)

        gate.onLifecycleEvent(Lifecycle.Event.ON_RESUME)
        assertTrue(gate.localDocumentActive)

        gate.onLifecycleEvent(Lifecycle.Event.ON_STOP)
        assertFalse(gate.localDocumentActive)

        gate.onDispose()
        assertTrue(gate.disposed)
        gate.onLifecycleEvent(Lifecycle.Event.ON_RESUME)
        assertFalse(gate.localDocumentActive)
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "post-dispose")) { delivered++ })
    }
}
