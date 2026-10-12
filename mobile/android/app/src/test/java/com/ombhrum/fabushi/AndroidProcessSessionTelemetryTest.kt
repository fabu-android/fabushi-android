package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionMarker
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionMarkerStore
import com.ombhrum.fabushi.androidmain.telemetry.AndroidProcessSessionTelemetry
import com.ombhrum.fabushi.androidmain.telemetry.AndroidUncleanExitSettlement
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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

    @Test fun clean_settlement_does_not_report_unclean_restart() {
        val store = MemoryStore()
        AndroidProcessSessionTelemetry(store, nowMs = { 100L }, processId = { 7 }).apply {
            assertNull(start())
            settleClean()
        }
        assertNull(store.active)
        assertNull(AndroidProcessSessionTelemetry(store, nowMs = { 200L }, processId = { 8 }).start())
    }

    @Test fun process_death_is_settled_once_on_next_process_start() {
        val store = MemoryStore()
        AndroidProcessSessionTelemetry(store, nowMs = { 100L }, processId = { 7 }).start()
        val settlement = AndroidProcessSessionTelemetry(store, nowMs = { 250L }, processId = { 8 }).start()
        assertEquals(AndroidUncleanExitSettlement(7, 100L, 250L), settlement)
        assertEquals(settlement, store.lastUnclean)
        assertEquals(AndroidProcessSessionMarker(8, 250L), store.active)
    }

    @Test fun reused_pid_does_not_hide_previous_unclean_process() {
        val store = MemoryStore().apply { active = AndroidProcessSessionMarker(7, 100L) }
        val settlement = AndroidProcessSessionTelemetry(store, nowMs = { 150L }, processId = { 7 }).start()
        assertEquals(AndroidUncleanExitSettlement(7, 100L, 150L), settlement)
        assertEquals(settlement, store.lastUnclean)
        assertEquals(AndroidProcessSessionMarker(7, 150L), store.active)
    }
}
