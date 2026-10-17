package com.ombhrum.fabushi.androidmain.coordinator

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Android replacement for Desktop's Node event-loop delay monitor.
 *
 * This owner measures only main-Looper scheduling delay. It does not own telemetry persistence:
 * completed windows are handed to the Coordinator, which forwards bounded aggregates to Rust Host.
 */
internal class AndroidMainThreadPressureSampler(
    private val onWindow: (p50Ms: Long, p95Ms: Long, maxMs: Long, windowMs: Long) -> Unit,
    private val mainHandler: Handler = Handler(Looper.getMainLooper()),
    private val nowMs: () -> Long = { SystemClock.elapsedRealtime() },
) : AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val executor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-main-thread-pressure").apply { isDaemon = true }
    }
    private val lock = Any()
    private val delaysMs = ArrayList<Long>()
    private var windowStartedAtMs = nowMs()

    fun start() {
        executor.scheduleWithFixedDelay(::probe, 0L, PROBE_INTERVAL_MS, TimeUnit.MILLISECONDS)
        executor.scheduleWithFixedDelay(
            ::flushWindow,
            REPORT_WINDOW_MS,
            REPORT_WINDOW_MS,
            TimeUnit.MILLISECONDS,
        )
    }

    private fun probe() {
        if (closed.get()) return
        val postedAtMs = nowMs()
        mainHandler.post {
            if (closed.get()) return@post
            val delayMs = (nowMs() - postedAtMs).coerceIn(0L, MAX_REPORTED_DELAY_MS)
            synchronized(lock) {
                delaysMs += delayMs
                if (delaysMs.size > MAX_SAMPLES) {
                    delaysMs.removeAt(0)
                }
            }
        }
    }

    private fun flushWindow() {
        if (closed.get()) return
        val finishedAtMs = nowMs()
        val snapshot = synchronized(lock) {
            val values = delaysMs.toLongArray()
            delaysMs.clear()
            val windowMs = (finishedAtMs - windowStartedAtMs).coerceAtLeast(0L)
            windowStartedAtMs = finishedAtMs
            values to windowMs
        }
        val values = snapshot.first
        if (values.isEmpty()) return
        values.sort()
        onWindow(
            percentile(values, 0.50),
            percentile(values, 0.95),
            values.last(),
            snapshot.second,
        )
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        executor.shutdownNow()
        synchronized(lock) { delaysMs.clear() }
        mainHandler.removeCallbacksAndMessages(null)
    }

    private fun percentile(sorted: LongArray, fraction: Double): Long {
        val index = ((sorted.size - 1) * fraction).toInt().coerceIn(0, sorted.lastIndex)
        return sorted[index]
    }

    companion object {
        private const val PROBE_INTERVAL_MS = 1_000L
        private const val REPORT_WINDOW_MS = 60_000L
        private const val MAX_REPORTED_DELAY_MS = 60_000L
        private const val MAX_SAMPLES = 120
    }
}
