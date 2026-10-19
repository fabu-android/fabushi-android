package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class MiniAppWebMcpSurfaceTest {
    @Test
    fun capabilityProbeProjectsEveryDeclaredContractDomain() {
        val domains = listOf(
            "offline.capabilities", "selection.pick", "transfer.send", "scan.metadata",
            "state.inspect", "config.check", "platform.time", "hotspot.plan",
            "update.check", "diagnostics.status", "compat.packetInspect", "private.tool",
        )
        assertEquals(
            listOf(
                "compat", "config", "diagnostics", "hotspot", "offline", "platform",
                "scan", "selection", "state", "transfer", "update",
            ),
            miniAppCapabilityDomains(domains.map { MiniAppToolContract(it, it, "none") }),
        )
    }
}
