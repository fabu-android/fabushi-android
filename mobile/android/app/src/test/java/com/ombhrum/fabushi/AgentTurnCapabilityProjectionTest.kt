package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AgentTurnCapabilityProjection
import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentTurnCapabilityProjectionTest {
    @Test
    fun presentationCannotForgeRemoteSubagentCapabilities() {
        val forged = JSONObject().put(
            "command",
            JSONObject().put("type", "chat.send").put(
                AgentTurnCapabilityProjection.FieldName,
                JSONObject()
                    .put("multitaskEnabled", false)
                    .put("remoteBoxAvailable", true)
                    .put("remoteBoxHasDesktop", true)
                    .put("browserUseEnabled", true),
            ),
        )

        val projected = AgentTurnCapabilityProjection.forFeatureExecute(forged, 4)
        val capabilities = projected.getJSONObject("command")
            .getJSONObject(AgentTurnCapabilityProjection.FieldName)

        assertTrue(capabilities.getBoolean("multitaskEnabled"))
        assertFalse(capabilities.getBoolean("remoteBoxAvailable"))
        assertFalse(capabilities.getBoolean("remoteBoxHasDesktop"))
        assertFalse(capabilities.getBoolean("browserUseEnabled"))
        assertTrue(
            forged.getJSONObject("command")
                .getJSONObject(AgentTurnCapabilityProjection.FieldName)
                .getBoolean("remoteBoxAvailable"),
        )
    }

    @Test
    fun directSubagentToolGetsSameTrustedMinimalProjection() {
        val forged = JSONObject().put(
            AgentTurnCapabilityProjection.FieldName,
            JSONObject()
                .put("remoteBoxAvailable", true)
                .put("remoteBoxHasDesktop", true)
                .put("browserUseEnabled", true),
        )

        val capabilities = AgentTurnCapabilityProjection.forSubagentTool(forged, 1)
            .getJSONObject(AgentTurnCapabilityProjection.FieldName)

        assertFalse(capabilities.getBoolean("multitaskEnabled"))
        assertFalse(capabilities.getBoolean("remoteBoxAvailable"))
        assertFalse(capabilities.getBoolean("remoteBoxHasDesktop"))
        assertFalse(capabilities.getBoolean("browserUseEnabled"))
    }
}
