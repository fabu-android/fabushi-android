package com.ombhrum.fabushi.androidmain.coordinator

import org.json.JSONObject

/**
 * Trusted process-owned projection of capabilities that may shape one Agent turn.
 * Presentation JSON is never authority for generated-subagent type exposure.
 */
internal object AgentTurnCapabilityProjection {
    const val FieldName = "coordinatorSubagentCapabilities"

    fun forFeatureExecute(
        params: JSONObject,
        availableProcessors: Int = Runtime.getRuntime().availableProcessors(),
    ): JSONObject {
        val projected = JSONObject(params.toString())
        val command = projected.optJSONObject("command") ?: return projected
        if (command.optString("type") == "chat.send") {
            command.put(FieldName, snapshot(availableProcessors))
        }
        return projected
    }

    fun forSubagentTool(
        params: JSONObject,
        availableProcessors: Int = Runtime.getRuntime().availableProcessors(),
    ): JSONObject = JSONObject(params.toString()).put(FieldName, snapshot(availableProcessors))

    internal fun snapshot(availableProcessors: Int): JSONObject =
        JSONObject()
            .put("multitaskEnabled", availableProcessors.coerceAtLeast(1) > 1)
            .put("remoteBoxAvailable", false)
            .put("remoteBoxHasDesktop", false)
            .put("browserUseEnabled", false)
}
