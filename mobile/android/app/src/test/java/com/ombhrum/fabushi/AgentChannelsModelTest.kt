package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AgentChannelsModelTest {
    @Test
    fun currentDesktopCatalogKeepsDiscordAndSlackComingSoon() {
        assertEquals(listOf("discord", "slack"), desktopMainAgentChannelManifests.map { it.platform })
        assertEquals(
            listOf(AgentChannelAvailability.COMING_SOON, AgentChannelAvailability.COMING_SOON),
            desktopMainAgentChannelManifests.map { it.availability },
        )
        desktopMainAgentChannelManifests.forEach { manifest ->
            val row = projectAgentChannelRow(manifest, connection = null)
            assertEquals(AgentChannelRowKind.COMING_SOON, row.kind)
            assertEquals("Soon", row.statusLabel)
            assertEquals(manifest.blurb, row.detail)
        }
    }

    @Test
    fun comingSoonWinsOverLegacyConnectionAndConnectableRowsPreserveDesktopStatusOrder() {
        val comingSoon = desktopMainAgentChannelManifests.first()
        val staleLegacy = AgentChannelConnection(
            platform = comingSoon.platform,
            label = "legacy",
            status = "connected",
        )
        assertEquals(
            AgentChannelRowKind.COMING_SOON,
            projectAgentChannelRow(comingSoon, staleLegacy).kind,
        )

        val available = comingSoon.copy(availability = AgentChannelAvailability.AVAILABLE)
        val connected = projectAgentChannelRow(
            available,
            AgentChannelConnection(available.platform, "workspace", "connected"),
        )
        assertEquals(AgentChannelRowKind.CONNECTED, connected.kind)
        assertEquals("Connected", connected.statusLabel)
        assertEquals("Connected as workspace", connected.detail)

        val failed = projectAgentChannelRow(
            available,
            AgentChannelConnection(available.platform, "workspace", "error", "revoked"),
        )
        assertEquals(AgentChannelRowKind.ERROR, failed.kind)
        assertEquals("Needs attention", failed.statusLabel)
        assertEquals("revoked", failed.detail)

        val connecting = projectAgentChannelRow(
            available,
            AgentChannelConnection(available.platform, "workspace", "pending"),
        )
        assertEquals(AgentChannelRowKind.CONNECTING, connecting.kind)
        assertEquals("Connecting", connecting.statusLabel)

        val availableOnly = projectAgentChannelRow(available, null)
        assertEquals(AgentChannelRowKind.AVAILABLE, availableOnly.kind)
        assertNull(availableOnly.statusLabel)
    }
}
