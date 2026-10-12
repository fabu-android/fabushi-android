package com.ombhrum.fabushi.androidmain.coordinator

import android.os.Debug
import org.json.JSONObject

internal data class AndroidProcessMetricsSample(
    val capturedAtMs: Long,
    val totalPssBytes: Long,
    val heapUsedBytes: Long,
    val heapLimitBytes: Long,
    val threadCount: Int,
)

internal class AndroidProcessMetricsOwner(
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val sampler: () -> AndroidProcessMetricsSample = {
        val runtime = Runtime.getRuntime()
        AndroidProcessMetricsSample(
            capturedAtMs = System.currentTimeMillis(),
            totalPssBytes = Debug.getPss().coerceAtLeast(0L) * 1024L,
            heapUsedBytes = (runtime.totalMemory() - runtime.freeMemory()).coerceAtLeast(0L),
            heapLimitBytes = runtime.maxMemory().coerceAtLeast(0L),
            threadCount = Thread.activeCount().coerceAtLeast(0),
        )
    },
    private val maxSamples: Int = 30,
) {
    init {
        require(maxSamples in 1..300) { "process metric history must remain bounded" }
    }

    private val lock = Any()
    private val samples = ArrayDeque<AndroidProcessMetricsSample>()
    private var generation = 0L
    private var disposed = false
    private var lastFailureClass: String? = null

    fun sample(): Boolean {
        val captured = runCatching { sampler() }
            .onFailure { error ->
                synchronized(lock) {
                    if (!disposed) lastFailureClass = error.javaClass.simpleName.take(80)
                }
            }
            .getOrNull() ?: return false
        require(captured.totalPssBytes >= 0L)
        require(captured.heapUsedBytes >= 0L)
        require(captured.heapLimitBytes >= 0L)
        require(captured.threadCount >= 0)
        synchronized(lock) {
            if (disposed) return false
            generation += 1
            samples.addLast(captured)
            while (samples.size > maxSamples) samples.removeFirst()
            lastFailureClass = null
        }
        return true
    }

    fun snapshot(): JSONObject = synchronized(lock) {
        val latest = samples.lastOrNull()
        JSONObject()
            .put("generation", generation)
            .put("sampleCount", samples.size)
            .put("capturedAtMs", latest?.capturedAtMs ?: JSONObject.NULL)
            .put("totalPssBytes", latest?.totalPssBytes ?: JSONObject.NULL)
            .put("heapUsedBytes", latest?.heapUsedBytes ?: JSONObject.NULL)
            .put("heapLimitBytes", latest?.heapLimitBytes ?: JSONObject.NULL)
            .put("threadCount", latest?.threadCount ?: JSONObject.NULL)
            .put("lastFailureClass", lastFailureClass ?: JSONObject.NULL)
            .put("backendReportingEnabled", false)
            .put("scope", "current-android-app-process")
    }

    fun dispose() {
        synchronized(lock) {
            disposed = true
            samples.clear()
            lastFailureClass = null
        }
    }

    fun isDisposed(): Boolean = synchronized(lock) { disposed }
}
