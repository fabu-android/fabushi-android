package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.diagnostics.AndroidProcessMetricReading
import com.ombhrum.fabushi.androidmain.diagnostics.AndroidProcessMetricsRuntime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.ArrayDeque
import java.util.concurrent.AbstractExecutorService
import java.util.concurrent.Callable
import java.util.concurrent.Delayed
import java.util.concurrent.RunnableScheduledFuture
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

class AndroidProcessMetricsRuntimeTest {
    @Test
    fun samplesCpuAndPssWithSessionPeakAndBoundedHistory() {
        val readings = ArrayDeque(
            listOf(
                AndroidProcessMetricReading(1_000, 100, 1024),
                AndroidProcessMetricReading(2_000, 350, 4096),
                AndroidProcessMetricReading(3_000, 450, 2048),
            ),
        )
        val runtime = AndroidProcessMetricsRuntime(
            readingProvider = { readings.removeFirst() },
            schedulePeriodMs = 10_000,
            maxSamples = 2,
            executorFactory = { NoopScheduledExecutor() },
        )

        runtime.sampleNowForTesting()
        runtime.sampleNowForTesting()
        runtime.sampleNowForTesting()

        val snapshot = runtime.snapshot()
        assertEquals(2, snapshot.size)
        assertEquals(25.0, snapshot[0].cpuPercent, 0.0001)
        assertEquals(4.0, snapshot[0].totalPssMb, 0.0001)
        assertEquals(4.0, snapshot[1].sessionPeakPssMb, 0.0001)
        assertEquals(10.0, snapshot[1].cpuPercent, 0.0001)
        runtime.close()
        assertTrue(runtime.snapshot().isEmpty())
    }

    @Test
    fun invalidOrRegressingCountersFailClosedWithoutNegativeCpu() {
        val readings = ArrayDeque(
            listOf(
                AndroidProcessMetricReading(1_000, 500, 1024),
                AndroidProcessMetricReading(1_000, 700, 2048),
                AndroidProcessMetricReading(2_000, 400, 3072),
            ),
        )
        val runtime = AndroidProcessMetricsRuntime(
            readingProvider = { readings.removeFirst() },
            executorFactory = { NoopScheduledExecutor() },
        )
        repeat(3) { runtime.sampleNowForTesting() }
        assertEquals(listOf(0.0, 0.0, 0.0), runtime.snapshot().map { it.cpuPercent })
        runtime.close()
    }

    private class NoopScheduledExecutor : AbstractExecutorService(), ScheduledExecutorService {
        private var shutdown = false
        override fun shutdown() { shutdown = true }
        override fun shutdownNow(): MutableList<Runnable> { shutdown = true; return mutableListOf() }
        override fun isShutdown(): Boolean = shutdown
        override fun isTerminated(): Boolean = shutdown
        override fun awaitTermination(timeout: Long, unit: TimeUnit): Boolean = true
        override fun execute(command: Runnable) = Unit
        override fun schedule(command: Runnable, delay: Long, unit: TimeUnit): ScheduledFuture<*> = NoopFuture<Unit>()
        override fun <V> schedule(callable: Callable<V>, delay: Long, unit: TimeUnit): ScheduledFuture<V> = NoopFuture()
        override fun scheduleAtFixedRate(command: Runnable, initialDelay: Long, period: Long, unit: TimeUnit): ScheduledFuture<*> = NoopFuture<Unit>()
        override fun scheduleWithFixedDelay(command: Runnable, initialDelay: Long, delay: Long, unit: TimeUnit): ScheduledFuture<*> = NoopFuture<Unit>()
    }

    private class NoopFuture<V> : RunnableScheduledFuture<V> {
        override fun isPeriodic(): Boolean = false
        override fun getDelay(unit: TimeUnit): Long = 0
        override fun compareTo(other: Delayed): Int = 0
        override fun run() = Unit
        override fun cancel(mayInterruptIfRunning: Boolean): Boolean = true
        override fun isCancelled(): Boolean = false
        override fun isDone(): Boolean = false
        override fun get(): V = throw UnsupportedOperationException()
        override fun get(timeout: Long, unit: TimeUnit): V = throw UnsupportedOperationException()
    }
}
