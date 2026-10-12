package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.telemetry.ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionMarker
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionMarkerStore
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionSettlementKind
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionTelemetry
import com.ombhrum.fabushi.androidmain.telemetry.ANDROID_STARTUP_STUCK_MS
import com.ombhrum.fabushi.androidmain.telemetry.AndroidStartupTelemetryEvent
import com.ombhrum.fabushi.androidmain.telemetry.AndroidStartupTelemetryTracker
import com.ombhrum.fabushi.androidmain.telemetry.AndroidUncleanExitSettlement
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidProcessSessionTelemetryTest {
    private class MemoryStore : AndroidProcessSessionMarkerStore {
        var active: AndroidProcessSessionMarker? = null
        var lastUnclean: AndroidUncleanExitSettlement? = null
        override fun readActive() = active
        override fun writeActive(marker: AndroidProcessSessionMarker) { active = marker }
        override fun clearActive() { active = null }
        override fun writeLastUnclean(settlement: AndroidUncleanExitSettlement) { lastUnclean = settlement }
        override fun readLastUnclean() = lastUnclean
    }

    @Test
    fun cleanSettlementDoesNotReportUncleanRestart() {
        val store = MemoryStore()
        AndroidProcessSessionTelemetry(
            store,
            nowMs = { 100L },
            processId = { 7 },
            currentVersion = { "1.2.3" },
        ).apply {
            assertNull(start())
            settleClean()
        }
        assertNull(store.active)
        assertNull(
            AndroidProcessSessionTelemetry(
                store,
                nowMs = { 200L },
                processId = { 8 },
                currentVersion = { "1.2.3" },
            ).start(),
        )
    }

    @Test
    fun processDeathPreservesHeartbeatVersionAndBoundedUptime() {
        val store = MemoryStore()
        var now = 100L
        val first = AndroidProcessSessionTelemetry(
            store,
            nowMs = { now },
            processId = { 7 },
            currentVersion = { "1.2.3" },
        )
        first.start()
        now = 70_100L
        first.heartbeat()
        first.noteCrashSeen()

        now = 90_000L
        val settlement = AndroidProcessSessionTelemetry(
            store,
            nowMs = { now },
            processId = { 8 },
            currentVersion = { "1.2.3" },
        ).start()
        requireNotNull(settlement)
        assertEquals(AndroidProcessSessionSettlementKind.UNCLEAN_EXIT, settlement.kind)
        assertEquals(300_000L, settlement.previousUptimeBucketMs)
        assertEquals("1.2.3", settlement.previousVersion)
        assertTrue(settlement.sameVersion)
        assertTrue(settlement.crashSeen)
        assertEquals(7, settlement.previousProcessId)
        assertEquals(100L, settlement.previousStartedAtMs)
        assertEquals(AndroidProcessSessionMarker(8, now, "1.2.3", now, false), store.active)
    }

    @Test
    fun reusedPidDoesNotHidePreviousUncleanProcess() {
        val store = MemoryStore().apply {
            active = AndroidProcessSessionMarker(
                processId = 7,
                startedAtMs = 100L,
                appVersion = "1.0",
                aliveAtMs = 100L,
            )
        }
        val settlement = AndroidProcessSessionTelemetry(
            store,
            nowMs = { 150L },
            processId = { 7 },
            currentVersion = { "2.0" },
        ).start()
        requireNotNull(settlement)
        assertEquals(AndroidProcessSessionSettlementKind.UNCLEAN_EXIT, settlement.kind)
        assertFalse(settlement.sameVersion)
        assertEquals("1.0", settlement.previousVersion)
        assertEquals(AndroidProcessSessionMarker(7, 150L, "2.0", 150L, false), store.active)
    }

    @Test
    fun staleMarkerIsOrphanedInsteadOfPagedAsRecentCrash() {
        val store = MemoryStore().apply {
            active = AndroidProcessSessionMarker(
                processId = 7,
                startedAtMs = 100L,
                appVersion = "1.0",
                aliveAtMs = 100L,
            )
        }
        val now = 100L + ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS + 1L
        val settlement = AndroidProcessSessionTelemetry(
            store,
            nowMs = { now },
            processId = { 8 },
            currentVersion = { "2.0" },
        ).start()
        requireNotNull(settlement)
        assertEquals(AndroidProcessSessionSettlementKind.MARKER_ORPHANED, settlement.kind)
        assertEquals("1.0", settlement.previousVersion)
        assertFalse(settlement.sameVersion)
    }
    @Test
    fun bufferedStartupTelemetryFlushesBeforeReady() {
        var now = 100L
        var delay = -1L
        var stuckCallback: (() -> Unit)? = null
        val tracker = AndroidStartupTelemetryTracker(
            monotonicNowMs = { now },
            scheduleStuck = { scheduledDelay, callback ->
                delay = scheduledDelay
                stuckCallback = callback
            },
        )
        tracker.markPhase("coordinator")
        tracker.armStuckWatchdog()
        val events = mutableListOf<AndroidStartupTelemetryEvent>()
        tracker.attach(events::add)
        assertEquals(ANDROID_STARTUP_STUCK_MS, delay)
        assertEquals(listOf("start"), events.map { it.outcome })

        now = 120_200L
        stuckCallback!!.invoke()
        tracker.markPhase("window")
        now = 120_260L
        tracker.noteReady()

        assertEquals(listOf("start", "stuck", "ready"), events.map { it.outcome })
        assertEquals("coordinator", events[1].phase)
        assertEquals(120_100L, events[1].elapsedMs)
        assertEquals(120_100L, events[1].phaseElapsedMs)
        assertEquals(60L, events[2].windowMs)
    }

    @Test
    fun startupFailureSettlesOnceAndSuppressesLateWatchdog() {
        var now = 5L
        var stuckCallback: (() -> Unit)? = null
        val tracker = AndroidStartupTelemetryTracker(
            monotonicNowMs = { now },
            scheduleStuck = { _, callback -> stuckCallback = callback },
        )
        tracker.armStuckWatchdog()
        tracker.markPhase("services")
        val events = mutableListOf<AndroidStartupTelemetryEvent>()
        tracker.attach(events::add)
        now = 25L
        tracker.noteFailed()
        tracker.noteReady()
        now = 130_000L
        stuckCallback!!.invoke()

        assertEquals(listOf("start", "failed"), events.map { it.outcome })
        assertEquals("services", events.last().phase)
        assertEquals(20L, events.last().elapsedMs)
    }

}
