package com.ombhrum.fabushi.androidmain.coordinator

import org.json.JSONObject

internal data class RemoteBoxCapabilitySnapshot(
    val available: Boolean,
    val hasDesktop: Boolean,
) {
    companion object {
        val Unavailable = RemoteBoxCapabilitySnapshot(false, false)
    }
}

/**
 * Trusted process-owned projection of capabilities that may shape one Agent turn.
 * Presentation JSON is never authority for generated-subagent type exposure.
 */
internal object AgentTurnCapabilityProjection {
    const val FieldName = "coordinatorSubagentCapabilities"

    fun forFeatureExecute(
        params: JSONObject,
        availableProcessors: Int = Runtime.getRuntime().availableProcessors(),
        remoteBox: RemoteBoxCapabilitySnapshot = RemoteBoxCapabilitySnapshot.Unavailable,
    ): JSONObject {
        val projected = JSONObject(params.toString())
        val command = projected.optJSONObject("command") ?: return projected
        if (command.optString("type") == "chat.send") {
            command.put(FieldName, snapshot(availableProcessors, remoteBox))
        }
        return projected
    }

    fun forSubagentTool(
        params: JSONObject,
        availableProcessors: Int = Runtime.getRuntime().availableProcessors(),
        remoteBox: RemoteBoxCapabilitySnapshot = RemoteBoxCapabilitySnapshot.Unavailable,
    ): JSONObject = JSONObject(params.toString()).put(FieldName, snapshot(availableProcessors, remoteBox))

    internal fun snapshot(
        availableProcessors: Int,
        remoteBox: RemoteBoxCapabilitySnapshot = RemoteBoxCapabilitySnapshot.Unavailable,
    ): JSONObject =
        JSONObject()
            .put("multitaskEnabled", availableProcessors.coerceAtLeast(1) > 1)
            .put("remoteBoxAvailable", remoteBox.available)
            .put("remoteBoxHasDesktop", remoteBox.available && remoteBox.hasDesktop)
            // A paired desktop does not imply a registered Browser adapter.
            .put("browserUseEnabled", false)
}
