package com.ombhrum.fabushi.androidmain.coordinator

import org.json.JSONObject

internal data class RemoteBoxCapabilitySnapshot(
    val available: Boolean,
    val hasDesktop: Boolean,
    val hasBrowser: Boolean = false,
) {
    companion object {
        val Unavailable = RemoteBoxCapabilitySnapshot(false, false, false)

        /**
         * Project only capabilities backed by the exact protected Remote executor set.
         *
         * Desktop exposes Computer only when a concrete ComputerToolExecutor is present.
         * The coarse Host hasDesktop bit also covers screenshot/external-machine surfaces,
         * so it must never grant the computeruse subagent type by itself.
         */
        fun fromBindingStatus(status: JSONObject): RemoteBoxCapabilitySnapshot {
            if (!status.optBoolean("ready", false)) return Unavailable
            val rawExecutors = status.optJSONArray("executors") ?: return Unavailable
            if (rawExecutors.length() == 0) return Unavailable

            val executors = buildSet {
                for (index in 0 until rawExecutors.length()) {
                    val value = rawExecutors.opt(index)
                    if (value !is String || value.isBlank()) return Unavailable
                    add(value)
                }
            }
            return RemoteBoxCapabilitySnapshot(
                available = true,
                // screenshot/external-shell/external-read/browser are not a Computer executor.
                hasDesktop = "computer" in executors,
                // Browser is a separate Desktop BrowserToolExecutor replacement.
                hasBrowser = "browser" in executors,
            )
        }
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
            // A paired desktop or Computer executor does not imply a Browser executor.
            .put("browserUseEnabled", remoteBox.available && remoteBox.hasBrowser)
}
