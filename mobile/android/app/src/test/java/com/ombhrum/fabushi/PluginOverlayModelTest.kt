package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PluginOverlayModelTest {
    @Test
    fun teamPolicyMatchesDesktopRemovalRules() {
        assertEquals("Managed by your team", pluginOverlayTeamPolicyLabel(PluginOverlayInstallMode.TEAM_REQUIRED))
        assertEquals(emptySet<String>(), pluginOverlayTeamPolicyActions(PluginOverlayInstallMode.TEAM_REQUIRED))
        assertEquals(setOf("remove"), pluginOverlayTeamPolicyActions(PluginOverlayInstallMode.TEAM_DEFAULT))
        assertEquals(setOf("uninstall"), pluginOverlayTeamPolicyActions(PluginOverlayInstallMode.USER))
        assertNull(pluginOverlayTeamPolicyLabel(PluginOverlayInstallMode.UNKNOWN))
    }

    @Test
    fun setupRequiredFieldsHonorDefaultsButNormalizationOnlyKeepsEnteredValues() {
        val fields = listOf(
            PluginOverlayVariableField("token", isRequired = true),
            PluginOverlayVariableField("region", isRequired = true, defaultValue = "us"),
            PluginOverlayVariableField("note", isRequired = false),
        )
        assertEquals(
            listOf("token"),
            missingRequiredPluginOverlayFields(
                fields,
                mapOf("token" to " ", "region" to null),
            ).map { it.key },
        )
        assertEquals(
            mapOf("token" to "secret", "note" to "hello"),
            normalizePluginOverlaySetupValues(
                fields,
                mapOf("token" to " secret ", "region" to " ", "note" to " hello "),
            ),
        )
    }

    @Test
    fun accountGroupingKeepsDefaultFirstAndChoosesAuthenticateVersusAdd() {
        val accounts = listOf(
            PluginOverlayAccount("github", "work"),
            PluginOverlayAccount("github", PLUGIN_DEFAULT_ACCOUNT_KEY),
            PluginOverlayAccount("drive", "team"),
        )
        assertEquals(
            listOf(PLUGIN_DEFAULT_ACCOUNT_KEY, "work"),
            groupPluginOverlayAccounts(accounts).getValue("github").map { it.accountKey },
        )
        assertEquals("Default", pluginOverlayAccountLabel(PLUGIN_DEFAULT_ACCOUNT_KEY))
        assertEquals("authenticate", pluginOverlayAccountAction(accounts, " work "))
        assertEquals("add", pluginOverlayAccountAction(accounts, "personal"))
        assertNull(pluginOverlayAccountAction(accounts, " "))
    }

    @Test
    fun authenticationNoticesMatchDesktopMeanings() {
        assertEquals(
            PluginOverlayNotice(false, "Already authenticated"),
            pluginOverlayAuthenticationNotice("already-authenticated"),
        )
        assertEquals(
            PluginOverlayNotice(true, "Server is not configured"),
            pluginOverlayAuthenticationNotice("not-configured"),
        )
        assertEquals(
            PluginOverlayNotice(true, "Couldn't start sign-in: offline"),
            pluginOverlayAuthenticationNotice("unreachable", "offline"),
        )
        assertFalse(pluginOverlayAuthenticationNotice("started").isError)
    }

    @Test
    fun serverToolsProjectionOptimisticallyTogglesAndFencesStaleCompletions() {
        val projector = PluginServerToolsProjector()
        val (generation, loadRequest) = projector.open()!!
        assertTrue(
            projector.settleLoad(
                generation,
                loadRequest,
                listOf(
                    PluginOverlayTool("read", isDisabled = false),
                    PluginOverlayTool("write", isDisabled = true),
                ),
            ),
        )
        val (_, toggleRequest) = projector.beginToggle("read")!!
        assertTrue(projector.snapshot().tools.first { it.name == "read" }.isDisabled)
        assertEquals("read", projector.snapshot().pendingTool)

        assertFalse(
            projector.settleToggle(
                generation,
                loadRequest,
                listOf(PluginOverlayTool("read", isDisabled = false)),
            ),
        )
        assertTrue(projector.settleToggle(generation, toggleRequest, tools = null, failure = "network"))
        assertEquals("network", projector.snapshot().failure)
        assertTrue(projector.snapshot().tools.first { it.name == "read" }.isDisabled)
        assertNull(projector.snapshot().pendingTool)

        val retry = projector.retry()!!
        projector.dispose()
        assertEquals(PluginServerToolsStatus.DISPOSED, projector.snapshot().status)
        assertFalse(
            projector.settleLoad(
                retry.first,
                retry.second,
                listOf(PluginOverlayTool("late", isDisabled = false)),
            ),
        )
    }
}
