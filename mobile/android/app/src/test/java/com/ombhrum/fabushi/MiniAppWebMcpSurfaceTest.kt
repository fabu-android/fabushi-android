package com.ombhrum.fabushi

import org.json.JSONArray
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
        val result = miniAppCapabilityAnswer(
            domains.map { MiniAppToolContract(it, it, "none") },
        )

        assertEquals("fabushi.miniapp.tools.v1", result.getString("contract"))
        assertEquals("android", result.getString("platform"))
        assertEquals(
            JSONArray(
                listOf(
                    "compat", "config", "diagnostics", "hotspot", "offline", "platform",
                    "scan", "selection", "state", "transfer", "update",
                ),
            ).toString(),
            result.getJSONArray("capabilities").toString(),
        )
    }
}
