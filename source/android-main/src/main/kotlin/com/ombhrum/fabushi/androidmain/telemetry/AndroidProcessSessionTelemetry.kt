package com.ombhrum.fabushi.androidmain.telemetry

import android.content.Context
import android.os.Process
import android.os.SystemClock

data class AndroidProcessSessionMarker(
    val processId: Int,
    val startedAtMs: Long,
)

data class AndroidUncleanExitSettlement(
    val previousProcessId: Int,
    val previousStartedAtMs: Long,
    val observedAtMs: Long,
)

interface AndroidProcessSessionMarkerStore {
    fun readActive(): AndroidProcessSessionMarker?
    fun writeActive(marker: AndroidProcessSessionMarker)
    fun clearActive()
    fun writeLastUnclean(settlement: AndroidUncleanExitSettlement)
    fun readLastUnclean(): AndroidUncleanExitSettlement?
}

class AndroidProcessSessionTelemetry(
    private val store: AndroidProcessSessionMarkerStore,
    private val nowMs: () -> Long,
    private val processId: () -> Int,
) {
    private var started = false
    private var settledClean = false

    fun start(): AndroidUncleanExitSettlement? {
        check(!started) { "process session telemetry is already started" }
        started = true
        val currentPid = processId()
        val now = nowMs()
        val settlement = store.readActive()
            ?.takeIf { it.processId != currentPid }
            ?.let {
                AndroidUncleanExitSettlement(
                    previousProcessId = it.processId,
                    previousStartedAtMs = it.startedAtMs,
                    observedAtMs = now,
                )
            }
        settlement?.let(store::writeLastUnclean)
        store.writeActive(AndroidProcessSessionMarker(currentPid, now))
        return settlement
    }

    fun settleClean() {
        if (!started || settledClean) return
        settledClean = true
        store.clearActive()
    }
}

class SharedPreferencesAndroidProcessSessionMarkerStore(
    context: Context,
) : AndroidProcessSessionMarkerStore {
    private val preferences =
        context.getSharedPreferences("fabushi-process-session-telemetry-v1", Context.MODE_PRIVATE)

    override fun readActive(): AndroidProcessSessionMarker? {
        if (!preferences.contains(KEY_ACTIVE_PID) || !preferences.contains(KEY_ACTIVE_STARTED_AT)) return null
        val pid = preferences.getInt(KEY_ACTIVE_PID, -1)
        val startedAt = preferences.getLong(KEY_ACTIVE_STARTED_AT, -1L)
        if (pid <= 0 || startedAt < 0L) {
            preferences.edit().remove(KEY_ACTIVE_PID).remove(KEY_ACTIVE_STARTED_AT).commit()
            return null
        }
        return AndroidProcessSessionMarker(pid, startedAt)
    }

    override fun writeActive(marker: AndroidProcessSessionMarker) {
        check(marker.processId > 0) { "process id must be positive" }
        check(marker.startedAtMs >= 0L) { "process start must be non-negative" }
        check(preferences.edit()
            .putInt(KEY_ACTIVE_PID, marker.processId)
            .putLong(KEY_ACTIVE_STARTED_AT, marker.startedAtMs)
            .commit()) { "failed to persist active process session marker" }
    }

    override fun clearActive() {
        check(preferences.edit()
            .remove(KEY_ACTIVE_PID)
            .remove(KEY_ACTIVE_STARTED_AT)
            .commit()) { "failed to clear active process session marker" }
    }

    override fun writeLastUnclean(settlement: AndroidUncleanExitSettlement) {
        check(preferences.edit()
            .putInt(KEY_LAST_PID, settlement.previousProcessId)
            .putLong(KEY_LAST_STARTED_AT, settlement.previousStartedAtMs)
            .putLong(KEY_LAST_OBSERVED_AT, settlement.observedAtMs)
            .commit()) { "failed to persist unclean process settlement" }
    }

    override fun readLastUnclean(): AndroidUncleanExitSettlement? {
        if (!preferences.contains(KEY_LAST_PID) ||
            !preferences.contains(KEY_LAST_STARTED_AT) ||
            !preferences.contains(KEY_LAST_OBSERVED_AT)) return null
        val pid = preferences.getInt(KEY_LAST_PID, -1)
        val startedAt = preferences.getLong(KEY_LAST_STARTED_AT, -1L)
        val observedAt = preferences.getLong(KEY_LAST_OBSERVED_AT, -1L)
        if (pid <= 0 || startedAt < 0L || observedAt < startedAt) return null
        return AndroidUncleanExitSettlement(pid, startedAt, observedAt)
    }

    companion object {
        private const val KEY_ACTIVE_PID = "activePid"
        private const val KEY_ACTIVE_STARTED_AT = "activeStartedAtMs"
        private const val KEY_LAST_PID = "lastUncleanPid"
        private const val KEY_LAST_STARTED_AT = "lastUncleanStartedAtMs"
        private const val KEY_LAST_OBSERVED_AT = "lastUncleanObservedAtMs"

        fun production(context: Context): AndroidProcessSessionTelemetry =
            AndroidProcessSessionTelemetry(
                store = SharedPreferencesAndroidProcessSessionMarkerStore(context),
                nowMs = SystemClock::elapsedRealtime,
                processId = Process::myPid,
            )
    }
}
