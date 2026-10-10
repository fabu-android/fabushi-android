package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AgentTurnCapabilityProjection
import com.ombhrum.fabushi.androidmain.coordinator.RemoteBoxCapabilitySnapshot
import org.json.JSONArray
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
    @Test
    fun remoteComputerSubagentRequiresConcreteComputerExecutor() {
        val externalOnly = RemoteBoxCapabilitySnapshot.fromBindingStatus(
            JSONObject()
                .put("ready", true)
                // This coarse flag is intentionally ignored for capability grant.
                .put("hasDesktop", true)
                .put("executors", JSONArray().put("external-shell").put("external-read")),
        )
        assertTrue(externalOnly.available)
        assertFalse(externalOnly.hasDesktop)

        val screenshotOnly = RemoteBoxCapabilitySnapshot.fromBindingStatus(
            JSONObject()
                .put("ready", true)
                .put("hasDesktop", true)
                .put("executors", JSONArray().put("screenshot")),
        )
        assertTrue(screenshotOnly.available)
        assertFalse(screenshotOnly.hasDesktop)

        val computer = RemoteBoxCapabilitySnapshot.fromBindingStatus(
            JSONObject()
                .put("ready", true)
                .put("hasDesktop", true)
                .put("executors", JSONArray().put("computer").put("screenshot")),
        )
        assertTrue(computer.available)
        assertTrue(computer.hasDesktop)

        val notReady = RemoteBoxCapabilitySnapshot.fromBindingStatus(
            JSONObject()
                .put("ready", false)
                .put("executors", JSONArray().put("computer")),
        )
        assertFalse(notReady.available)
        assertFalse(notReady.hasDesktop)

        val malformed = RemoteBoxCapabilitySnapshot.fromBindingStatus(
            JSONObject()
                .put("ready", true)
                .put("executors", JSONArray().put(JSONObject().put("executor", "computer"))),
        )
        assertFalse(malformed.available)
        assertFalse(malformed.hasDesktop)
    }

}
