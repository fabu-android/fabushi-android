package com.ombhrum.fabushi

import android.util.Log
import org.json.JSONObject

data class AndroidUpdaterTelemetryEvent(
    val category: String,
    val action: String,
    val result: String,
    val trigger: String? = null,
    val track: String? = null,
    val currentVersion: String? = null,
    val targetVersion: String? = null,
    val phase: String? = null,
    val durationMs: Long? = null,
    val errorClass: String? = null,
)

class AndroidUpdaterTelemetry(private val limit: Int = 64) {
    private val events = ArrayDeque<AndroidUpdaterTelemetryEvent>()

    @Synchronized
    fun record(event: AndroidUpdaterTelemetryEvent) {
        val bounded = limit.coerceAtLeast(1)
        while (events.size >= bounded) events.removeFirst()
        events.addLast(event)
        val json = JSONObject()
            .put("category", event.category)
            .put("action", event.action)
            .put("result", event.result)
        event.trigger?.let { json.put("trigger", it) }
        event.track?.let { json.put("track", it) }
        event.currentVersion?.let { json.put("currentVersion", it) }
        event.targetVersion?.let { json.put("targetVersion", it) }
        event.phase?.let { json.put("phase", it) }
        event.durationMs?.let { json.put("durationMs", it.coerceAtLeast(0L)) }
        event.errorClass?.let { json.put("errorClass", it.take(96)) }
        Log.i("FabushiUpdate", json.toString())
    }

    @Synchronized
    fun snapshot(): List<AndroidUpdaterTelemetryEvent> = events.toList()
}
