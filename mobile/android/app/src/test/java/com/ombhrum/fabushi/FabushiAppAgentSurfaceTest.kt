package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import kotlinx.coroutines.test.runTest
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Test

class FabushiAppAgentSurfaceTest {
    @Test
    fun officialDeviceGatewayMatchesDesktopStatusOnlyRegistrationBoundary() {
        assertEquals(
            listOf(FabushiAppAgentSurface.StatusTool),
            FabushiRemoteDeviceGateway.OfficialToolNames,
        )
        assertFalse(FabushiRemoteDeviceGateway.OfficialToolNames.contains(FabushiAppAgentSurface.ActionTool))
        assertFalse(FabushiRemoteDeviceGateway.OfficialToolNames.contains(FabushiAppAgentSurface.SnapshotTool))
    }

    @Test
    fun officialDeviceGatewayFencesExpiryIdentityAndReadOnlyPreflight() {
        val now = 1_000_000L
        val token = "fabushi-account-token-".padEnd(48, 'x')
        assertTrue(
            FabushiRemoteDeviceGateway.isUsableOfficialSession(
                token,
                "device-a",
                "browser:a",
                now + 60_000L,
                now,
            ),
        )
        assertFalse(
            FabushiRemoteDeviceGateway.isUsableOfficialSession(
                token,
                "device-a",
                "browser:a",
                now + 30_000L,
                now,
            ),
        )
        assertTrue(
            FabushiRemoteDeviceGateway.isSameOfficialSession(
                "device-a",
                "browser:a",
                token,
                "device-a",
                "browser:a",
                token,
            ),
        )
        assertFalse(
            FabushiRemoteDeviceGateway.isSameOfficialSession(
                "device-a",
                "browser:a",
                token,
                "device-b",
                "browser:b",
                token,
            ),
        )

        val readOnly = JSONObject().put(
            "diskPreflight",
            JSONObject()
                .put("impact", "none")
                .put("targets", JSONArray()),
        )
        assertTrue(FabushiRemoteDeviceGateway.isReadOnlyOfficialPreflight(readOnly))
        val writeShaped = JSONObject().put(
            "diskPreflight",
            JSONObject()
                .put("impact", "writes")
                .put("targets", JSONArray().put("/tmp/write")),
        )
        assertFalse(FabushiRemoteDeviceGateway.isReadOnlyOfficialPreflight(writeShaped))
        assertFalse(FabushiRemoteDeviceGateway.isReadOnlyOfficialPreflight(JSONObject()))
    }

    @Test
    fun semanticSurfaceRejectsStaleAndSensitiveWrites() {
        val surface = FabushiAppAgentSurface()
        var invoked = false
        val first = surface.publish(
            screen = "login",
            elements = listOf(
                FabushiAppAgentSurface.Element("login-submit", "button", "登录"),
                FabushiAppAgentSurface.Element("login-password", "textbox", "密码", sensitive = true),
            ),
            actions = mapOf(
                "login-submit" to FabushiAppAgentSurface.Action(setOf("invoke")) { invoked = true },
                "login-password" to FabushiAppAgentSurface.Action(setOf("setValue")) {},
            ),
        )
        assertTrue(surface.status().available)
        assertEquals("login", first.screen)
        assertEquals(listOf("login-submit"), surface.find(role = "button").map { it.agentId })

        val after = surface.action(first.generation, "login-submit", "invoke")
        assertTrue(invoked)
        assertTrue(after.generation > first.generation)
        assertFalse(runCatching {
            surface.action(first.generation, "login-submit", "invoke")
        }.isSuccess)
        assertFalse(runCatching {
            surface.action(after.generation, "login-password", "setValue", "secret")
        }.isSuccess)
    }

    @Test
    fun toolNamesMatchWebAndDesktopAppMcpContract() {
        assertEquals(
            listOf(
                "fabushi.app.status",
                "fabushi.app.snapshot",
                "fabushi.app.find",
                "fabushi.app.action",
                "fabushi.app.wait",
                "fabushi.app.assert",
            ),
            FabushiAppAgentSurface.ToolNames,
        )
    }
    @Test
    fun assertionsAndWaitUseTheSameSemanticState() = runTest {
        val surface = FabushiAppAgentSurface()
        val snapshot = surface.publish(
            screen = "home",
            elements = listOf(
                FabushiAppAgentSurface.Element("home-add-button", "button", "添加"),
            ),
        )
        val assertion = surface.assertState(
            expectedScreen = "home",
            agentId = "home-add-button",
            state = "enabled",
        )
        assertTrue(assertion.passed)
        assertEquals(snapshot.generation, assertion.generation)
        assertTrue(surface.waitFor(agentId = "home-add-button", timeoutMilliseconds = 100).passed)
        assertFalse(surface.waitFor(agentId = "missing", timeoutMilliseconds = 100).passed)
    }

}
