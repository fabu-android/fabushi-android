package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import kotlinx.coroutines.test.runTest
import org.junit.Test

class FabushiAppAgentSurfaceTest {
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
