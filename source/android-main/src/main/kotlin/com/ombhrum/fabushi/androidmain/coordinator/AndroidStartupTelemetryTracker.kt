package com.ombhrum.fabushi.androidmain.coordinator

internal const val ANDROID_STARTUP_STUCK_MS = 120_000L

internal data class AndroidStartupTelemetryEvent(
    val outcome: String,
    val phase: String,
    val elapsedMs: Long,
    val phaseElapsedMs: Long,
    val windowMs: Long? = null,
)

internal class AndroidStartupTelemetryTracker(
    private val monotonicNowMs: () -> Long,
    private val scheduleStuck: (Long, () -> Unit) -> Unit,
) {
    private val startedAtMs = monotonicNowMs().coerceAtLeast(0L)
    private var phase = "start"
    private var phaseStartedAtMs = startedAtMs
    private var attached: ((AndroidStartupTelemetryEvent) -> Unit)? = null
    private val buffered = mutableListOf<AndroidStartupTelemetryEvent>()
    private var settled = false
    private var watchdogArmed = false

    init {
        emit("start", startedAtMs)
    }

    @Synchronized
    fun attach(consumer: (AndroidStartupTelemetryEvent) -> Unit) {
        if (attached != null) return
        attached = consumer
        buffered.forEach(consumer)
        buffered.clear()
    }

    @Synchronized
    fun markPhase(nextPhase: String) {
        if (settled) return
        val normalized = nextPhase.trim().ifEmpty { "unknown" }
        phase = normalized
        phaseStartedAtMs = monotonicNowMs().coerceAtLeast(startedAtMs)
    }

    @Synchronized
    fun armStuckWatchdog() {
        if (settled || watchdogArmed) return
        watchdogArmed = true
        scheduleStuck(ANDROID_STARTUP_STUCK_MS) {
            synchronized(this) {
                if (!settled) emit("stuck", monotonicNowMs())
            }
        }
    }

    @Synchronized
    fun noteReady() {
        if (settled) return
        val now = monotonicNowMs()
        emit("ready", now, windowMs = (now - phaseStartedAtMs).coerceAtLeast(0L))
        settled = true
    }

    @Synchronized
    fun noteFailed() {
        if (settled) return
        emit("failed", monotonicNowMs())
        settled = true
    }

    private fun emit(outcome: String, rawNowMs: Long, windowMs: Long? = null) {
        val now = rawNowMs.coerceAtLeast(startedAtMs)
        val event = AndroidStartupTelemetryEvent(
            outcome = outcome,
            phase = phase,
            elapsedMs = (now - startedAtMs).coerceAtLeast(0L),
            phaseElapsedMs = (now - phaseStartedAtMs).coerceAtLeast(0L),
            windowMs = windowMs,
        )
        val consumer = attached
        if (consumer == null) buffered += event else consumer(event)
    }
}
