package com.ombhrum.fabushi.androidmain.telemetry

import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

const val ANDROID_STARTUP_STUCK_MS = 120_000L

data class AndroidStartupLifecycleReport(
    val outcome: String,
    val phase: String,
    val elapsedMs: Long,
    val phaseElapsedMs: Long? = null,
    val windowMs: Long? = null,
)

fun interface AndroidStartupWatchdogScheduler {
    fun schedule(delayMs: Long, task: () -> Unit): AutoCloseable
}

/**
 * Process-scoped Android replacement for Desktop startup lifecycle telemetry.
 *
 * It owns only startup observation. Product/runtime truth stays in the process Coordinator and
 * Host; reports are buffered until the typed Coordinator telemetry sink is available.
 */
class AndroidStartupTelemetryOwner(
    private val startedAtMs: Long,
    private val nowMs: () -> Long,
    private val scheduler: AndroidStartupWatchdogScheduler,
    private val closeScheduler: () -> Unit = {},
) : AutoCloseable {
    private val lock = Any()
    private var phase = "app_ready_wait"
    private var phaseStartedAtMs = startedAtMs
    private var windowStartedAtMs: Long? = null
    private var settled = false
    private var reporter: ((AndroidStartupLifecycleReport) -> Unit)? = null
    private val pending = ArrayDeque<AndroidStartupLifecycleReport>()
    private val watchdog = scheduler.schedule(ANDROID_STARTUP_STUCK_MS, ::reportStuckIfNeeded)

    init {
        require(startedAtMs >= 0L) { "startup time must be nonnegative" }
        enqueueLocked(
            AndroidStartupLifecycleReport(
                outcome = "start",
                phase = phase,
                elapsedMs = 0L,
            ),
        )
    }

    fun attachReporter(reporter: (AndroidStartupLifecycleReport) -> Unit) {
        val buffered = synchronized(lock) {
            check(this.reporter == null) { "startup telemetry reporter is already attached" }
            this.reporter = reporter
            buildList {
                while (pending.isNotEmpty()) add(pending.removeFirst())
            }
        }
        buffered.forEach(reporter)
    }

    fun markPhase(next: String) {
        requireValidPhase(next)
        synchronized(lock) {
            if (settled) return
            val now = monotonicNowLocked()
            phase = next
            phaseStartedAtMs = now
            if (next == "window" && windowStartedAtMs == null) windowStartedAtMs = now
        }
    }

    fun ready() = settle("ready")

    fun failed() = settle("failed")

    fun cancel() {
        val shouldCancel = synchronized(lock) {
            if (settled) false else {
                settled = true
                true
            }
        }
        if (shouldCancel) watchdog.close()
    }

    private fun settle(outcome: String) {
        val report = synchronized(lock) {
            if (settled) return
            settled = true
            val now = monotonicNowLocked()
            AndroidStartupLifecycleReport(
                outcome = outcome,
                phase = phase,
                elapsedMs = (now - startedAtMs).coerceAtLeast(0L),
                windowMs = windowStartedAtMs?.let { (now - it).coerceAtLeast(0L) },
            ).also(::enqueueLocked)
        }
        watchdog.close()
        dispatchIfAttached(report)
    }

    private fun reportStuckIfNeeded() {
        val report = synchronized(lock) {
            if (settled) return
            val now = monotonicNowLocked()
            AndroidStartupLifecycleReport(
                outcome = "stuck",
                phase = phase,
                elapsedMs = (now - startedAtMs).coerceAtLeast(0L),
                phaseElapsedMs = (now - phaseStartedAtMs).coerceAtLeast(0L),
            ).also(::enqueueLocked)
        }
        dispatchIfAttached(report)
    }

    private fun dispatchIfAttached(report: AndroidStartupLifecycleReport) {
        val sink = synchronized(lock) { reporter }
        if (sink != null) {
            synchronized(lock) { pending.remove(report) }
            sink(report)
        }
    }

    private fun enqueueLocked(report: AndroidStartupLifecycleReport) {
        if (reporter == null) {
            if (pending.size >= PRE_ATTACH_BUFFER_LIMIT) pending.removeFirst()
            pending.addLast(report)
        }
    }

    private fun monotonicNowLocked(): Long = nowMs().coerceAtLeast(startedAtMs)

    override fun close() {
        cancel()
        closeScheduler()
    }

    companion object {
        private const val PRE_ATTACH_BUFFER_LIMIT = 8
        private val PHASE = Regex("^[A-Za-z0-9._:-]{1,96}$")

        private fun requireValidPhase(value: String) {
            require(PHASE.matches(value)) { "startup phase must be a bounded token" }
        }

        fun production(startedAtMs: Long, nowMs: () -> Long): AndroidStartupTelemetryOwner {
            val executor = Executors.newSingleThreadScheduledExecutor { runnable ->
                Thread(runnable, "fabushi-startup-watchdog").apply { isDaemon = true }
            }
            val scheduler = AndroidStartupWatchdogScheduler { delayMs, task ->
                val future = executor.schedule(task, delayMs, TimeUnit.MILLISECONDS)
                AutoCloseable { future.cancel(false) }
            }
            return AndroidStartupTelemetryOwner(
                startedAtMs = startedAtMs,
                nowMs = nowMs,
                scheduler = scheduler,
                closeScheduler = { executor.shutdownNow() },
            )
        }
    }
}
