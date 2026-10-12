package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.telemetry.ANDROID_STARTUP_STUCK_MS
import com.ombhrum.fabushi.androidmain.telemetry.AndroidStartupLifecycleReport
import com.ombhrum.fabushi.androidmain.telemetry.AndroidStartupTelemetryOwner
import com.ombhrum.fabushi.androidmain.telemetry.AndroidStartupWatchdogScheduler
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidStartupTelemetryOwnerTest {
    private class FakeWatchdog : AndroidStartupWatchdogScheduler {
        var delayMs: Long? = null
        var task: (() -> Unit)? = null
        var cancelled = false

        override fun schedule(delayMs: Long, task: () -> Unit): AutoCloseable {
            this.delayMs = delayMs
            this.task = task
            return AutoCloseable { cancelled = true }
        }

        fun fire() = task?.invoke()
    }

    @Test
    fun buffersStartUntilCoordinatorAttachesAndSettlesWindowReadyOnce() {
        var now = 1_000L
        val watchdog = FakeWatchdog()
        val owner = AndroidStartupTelemetryOwner(1_000L, { now }, watchdog)
        val reports = mutableListOf<AndroidStartupLifecycleReport>()

        owner.markPhase("services")
        now = 1_040L
        owner.markPhase("coordinator")
        owner.attachReporter(reports::add)
        assertEquals(listOf("start"), reports.map { it.outcome })
        assertEquals(ANDROID_STARTUP_STUCK_MS, watchdog.delayMs)

        now = 1_100L
        owner.markPhase("window")
        now = 1_250L
        owner.ready()
        owner.ready()
        watchdog.fire()

        assertEquals(listOf("start", "ready"), reports.map { it.outcome })
        assertEquals("window", reports.last().phase)
        assertEquals(250L, reports.last().elapsedMs)
        assertEquals(150L, reports.last().windowMs)
        assertTrue(watchdog.cancelled)
    }

    @Test
    fun watchdogReportsCurrentPhaseWithoutSettlingAndReadyCanFollow() {
        var now = 10L
        val watchdog = FakeWatchdog()
        val reports = mutableListOf<AndroidStartupLifecycleReport>()
        val owner = AndroidStartupTelemetryOwner(10L, { now }, watchdog)
        owner.attachReporter(reports::add)
        now = 100L
        owner.markPhase("coordinator")
        now = 120_010L
        watchdog.fire()

        assertEquals("stuck", reports.last().outcome)
        assertEquals("coordinator", reports.last().phase)
        assertEquals(120_000L, reports.last().elapsedMs)
        assertEquals(119_910L, reports.last().phaseElapsedMs)
        assertFalse(watchdog.cancelled)

        now = 120_100L
        owner.markPhase("window")
        now = 120_200L
        owner.ready()
        assertEquals("ready", reports.last().outcome)
        assertTrue(watchdog.cancelled)
    }

    @Test
    fun failureIsTerminalAndDoesNotCarryThrowablePayload() {
        var now = 50L
        val watchdog = FakeWatchdog()
        val reports = mutableListOf<AndroidStartupLifecycleReport>()
        val owner = AndroidStartupTelemetryOwner(40L, { now }, watchdog)
        owner.attachReporter(reports::add)
        owner.markPhase("services")
        now = 80L
        owner.failed()
        owner.markPhase("window")
        owner.ready()
        watchdog.fire()

        assertEquals(listOf("start", "failed"), reports.map { it.outcome })
        assertEquals("services", reports.last().phase)
        assertEquals(40L, reports.last().elapsedMs)
        assertFalse(reports.last().toString().contains("secret failure text"))
        assertTrue(watchdog.cancelled)
    }
}
