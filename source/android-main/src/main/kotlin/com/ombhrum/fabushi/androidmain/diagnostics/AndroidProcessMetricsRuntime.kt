package com.ombhrum.fabushi.androidmain.diagnostics

import android.os.Debug
import android.os.Process
import android.os.SystemClock
import java.util.ArrayDeque
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

internal const val ANDROID_PROCESS_METRICS_SUBSAMPLE_MS = 10_000L
internal const val ANDROID_PROCESS_METRICS_MAX_SAMPLES = 180

internal data class AndroidProcessMetricReading(
    val observedAtMs: Long,
    val elapsedCpuMs: Long,
    val totalPssKb: Int,
)

internal data class AndroidProcessMetricSample(
    val observedAtMs: Long,
    val cpuPercent: Double,
    val totalPssMb: Double,
    val sessionPeakPssMb: Double,
)

/**
 * Android replacement for Desktop's process-tree sampler.
 *
 * Android application sandboxing does not permit the Desktop cursor-proclist process-tree scan.
 * This owner samples only the Fabushi application process. It stores bounded, non-identifying
 * numeric diagnostics in memory and never transports them by itself.
 */
internal class AndroidProcessMetricsRuntime(
    private val readingProvider: () -> AndroidProcessMetricReading = {
        val memory = Debug.MemoryInfo().also(Debug::getMemoryInfo)
        AndroidProcessMetricReading(
            observedAtMs = SystemClock.elapsedRealtime(),
            elapsedCpuMs = Process.getElapsedCpuTime(),
            totalPssKb = memory.totalPss.coerceAtLeast(0),
        )
    },
    private val schedulePeriodMs: Long = ANDROID_PROCESS_METRICS_SUBSAMPLE_MS,
    private val maxSamples: Int = ANDROID_PROCESS_METRICS_MAX_SAMPLES,
    private val executorFactory: () -> ScheduledExecutorService = {
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "fabushi-process-metrics").apply { isDaemon = true }
        }
    },
) : AutoCloseable {
    init {
        require(schedulePeriodMs > 0) { "process metrics schedule must be positive" }
        require(maxSamples in 1..10_000) { "process metrics sample bound is invalid" }
    }

    private val started = AtomicBoolean(false)
    private val closed = AtomicBoolean(false)
    private val lock = Any()
    private val samples = ArrayDeque<AndroidProcessMetricSample>()
    private var executor: ScheduledExecutorService? = null
    private var previous: AndroidProcessMetricReading? = null
    private var sessionPeakPssMb = 0.0

    fun start() {
        if (closed.get() || !started.compareAndSet(false, true)) return
        val created = executorFactory()
        executor = created
        created.scheduleWithFixedDelay(
            ::sampleSafely,
            schedulePeriodMs,
            schedulePeriodMs,
            TimeUnit.MILLISECONDS,
        )
    }

    internal fun sampleNowForTesting() {
        check(!closed.get()) { "process metrics runtime is closed" }
        sampleSafely()
    }

    fun snapshot(): List<AndroidProcessMetricSample> = synchronized(lock) {
        samples.toList()
    }

    private fun sampleSafely() {
        if (closed.get()) return
        val reading = runCatching(readingProvider).getOrNull() ?: return
        if (reading.observedAtMs < 0 || reading.elapsedCpuMs < 0 || reading.totalPssKb < 0) return
        synchronized(lock) {
            val pssMb = reading.totalPssKb / 1024.0
            sessionPeakPssMb = maxOf(sessionPeakPssMb, pssMb)
            val prior = previous
            val cpuPercent = if (prior == null) {
                0.0
            } else {
                val elapsedMs = reading.observedAtMs - prior.observedAtMs
                val cpuMs = reading.elapsedCpuMs - prior.elapsedCpuMs
                if (elapsedMs <= 0L || cpuMs <= 0L) 0.0
                else (cpuMs.toDouble() / elapsedMs.toDouble() * 100.0).coerceAtLeast(0.0)
            }
            previous = reading
            samples.addLast(
                AndroidProcessMetricSample(
                    observedAtMs = reading.observedAtMs,
                    cpuPercent = cpuPercent,
                    totalPssMb = pssMb,
                    sessionPeakPssMb = sessionPeakPssMb,
                ),
            )
            while (samples.size > maxSamples) samples.removeFirst()
        }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        started.set(false)
        executor?.shutdownNow()
        executor = null
        synchronized(lock) {
            previous = null
            samples.clear()
            sessionPeakPssMb = 0.0
        }
    }
}
