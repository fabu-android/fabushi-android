package com.ombhrum.fabushi.androidmain.telemetry

import android.content.Context
import android.os.Process

const val ANDROID_PROCESS_SESSION_HEARTBEAT_INTERVAL_MS = 60_000L
const val ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS = 30L * 24L * 60L * 60L * 1_000L

enum class AndroidProcessSessionSettlementKind {
    UNCLEAN_EXIT,
    MARKER_ORPHANED,
}

data class AndroidProcessSessionMarker(
    val processId: Int,
    val startedAtMs: Long,
    val appVersion: String = "unknown",
    val aliveAtMs: Long = startedAtMs,
    val crashSeen: Boolean = false,
)

data class AndroidUncleanExitSettlement(
    val previousProcessId: Int,
    val previousStartedAtMs: Long,
    val observedAtMs: Long,
    val kind: AndroidProcessSessionSettlementKind = AndroidProcessSessionSettlementKind.UNCLEAN_EXIT,
    val previousUptimeBucketMs: Long = 60_000L,
    val previousVersion: String = "unknown",
    val sameVersion: Boolean = true,
    val crashSeen: Boolean = false,
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
    private val currentVersion: () -> String = { "unknown" },
) {
    private var started = false
    private var settledClean = false
    private var active: AndroidProcessSessionMarker? = null

    fun start(): AndroidUncleanExitSettlement? {
        check(!started) { "process session telemetry is already started" }
        started = true
        val currentPid = processId()
        val now = nowMs().coerceAtLeast(0L)
        val version = normalizeVersion(currentVersion())
        val settlement = store.readActive()?.let { marker ->
            val aliveAt = maxOf(marker.startedAtMs, marker.aliveAtMs)
            val ageMs = (now - aliveAt).coerceAtLeast(0L)
            AndroidUncleanExitSettlement(
                previousProcessId = marker.processId,
                previousStartedAtMs = marker.startedAtMs,
                observedAtMs = now,
                kind = if (ageMs > ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS) {
                    AndroidProcessSessionSettlementKind.MARKER_ORPHANED
                } else {
                    AndroidProcessSessionSettlementKind.UNCLEAN_EXIT
                },
                previousUptimeBucketMs = uptimeBucketMs(
                    (aliveAt - marker.startedAtMs).coerceAtLeast(0L),
                ),
                previousVersion = marker.appVersion,
                sameVersion = marker.appVersion == version,
                crashSeen = marker.crashSeen,
            )
        }
        settlement?.let(store::writeLastUnclean)
        val marker = AndroidProcessSessionMarker(
            processId = currentPid,
            startedAtMs = now,
            appVersion = version,
            aliveAtMs = now,
            crashSeen = false,
        )
        store.writeActive(marker)
        active = marker
        return settlement
    }

    fun heartbeat() {
        if (!started || settledClean) return
        val marker = active ?: return
        val next = marker.copy(aliveAtMs = maxOf(marker.aliveAtMs, nowMs().coerceAtLeast(0L)))
        store.writeActive(next)
        active = next
    }

    fun noteCrashSeen() {
        if (!started || settledClean) return
        val marker = active ?: return
        if (marker.crashSeen) return
        val next = marker.copy(
            aliveAtMs = maxOf(marker.aliveAtMs, nowMs().coerceAtLeast(0L)),
            crashSeen = true,
        )
        store.writeActive(next)
        active = next
    }

    fun settleClean() {
        if (!started || settledClean) return
        settledClean = true
        active = null
        store.clearActive()
    }

    companion object {
        private val VERSION_SHAPE = Regex("^[0-9A-Za-z.+-]{1,64}$")
        private val UPTIME_BUCKET_CEILINGS_MS = longArrayOf(
            60_000L,
            300_000L,
            1_800_000L,
            21_600_000L,
            86_400_000L,
            ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS,
        )

        private fun normalizeVersion(value: String): String =
            value.trim().takeIf(VERSION_SHAPE::matches) ?: "unknown"

        private fun uptimeBucketMs(uptimeMs: Long): Long =
            UPTIME_BUCKET_CEILINGS_MS.firstOrNull { uptimeMs < it }
                ?: ANDROID_PROCESS_SESSION_ORPHAN_BOUND_MS
    }
}

class SharedPreferencesAndroidProcessSessionMarkerStore(
    context: Context,
) : AndroidProcessSessionMarkerStore {
    private val applicationContext = context.applicationContext
    private val preferences =
        applicationContext.getSharedPreferences("fabushi-process-session-telemetry-v1", Context.MODE_PRIVATE)

    override fun readActive(): AndroidProcessSessionMarker? {
        if (!preferences.contains(KEY_ACTIVE_PID) || !preferences.contains(KEY_ACTIVE_STARTED_AT)) return null
        val pid = preferences.getInt(KEY_ACTIVE_PID, -1)
        val startedAt = preferences.getLong(KEY_ACTIVE_STARTED_AT, -1L)
        val aliveAt = preferences.getLong(KEY_ACTIVE_ALIVE_AT, startedAt)
        val version = preferences.getString(KEY_ACTIVE_VERSION, "unknown")
            ?.takeIf { VERSION_SHAPE.matches(it) }
            ?: "unknown"
        val crashSeen = preferences.getBoolean(KEY_ACTIVE_CRASH_SEEN, false)
        if (pid <= 0 || startedAt < 0L || aliveAt < startedAt) {
            clearActive()
            return null
        }
        return AndroidProcessSessionMarker(pid, startedAt, version, aliveAt, crashSeen)
    }

    override fun writeActive(marker: AndroidProcessSessionMarker) {
        check(marker.processId > 0) { "process id must be positive" }
        check(marker.startedAtMs >= 0L && marker.aliveAtMs >= marker.startedAtMs) {
            "process timestamps must be monotonic"
        }
        check(VERSION_SHAPE.matches(marker.appVersion)) { "process version is invalid" }
        check(
            preferences.edit()
                .putInt(KEY_ACTIVE_PID, marker.processId)
                .putLong(KEY_ACTIVE_STARTED_AT, marker.startedAtMs)
                .putLong(KEY_ACTIVE_ALIVE_AT, marker.aliveAtMs)
                .putString(KEY_ACTIVE_VERSION, marker.appVersion)
                .putBoolean(KEY_ACTIVE_CRASH_SEEN, marker.crashSeen)
                .commit(),
        ) { "failed to persist active process session marker" }
    }

    override fun clearActive() {
        check(
            preferences.edit()
                .remove(KEY_ACTIVE_PID)
                .remove(KEY_ACTIVE_STARTED_AT)
                .remove(KEY_ACTIVE_ALIVE_AT)
                .remove(KEY_ACTIVE_VERSION)
                .remove(KEY_ACTIVE_CRASH_SEEN)
                .commit(),
        ) { "failed to clear active process session marker" }
    }

    override fun writeLastUnclean(settlement: AndroidUncleanExitSettlement) {
        check(
            preferences.edit()
                .putInt(KEY_LAST_PID, settlement.previousProcessId)
                .putLong(KEY_LAST_STARTED_AT, settlement.previousStartedAtMs)
                .putLong(KEY_LAST_OBSERVED_AT, settlement.observedAtMs)
                .putString(KEY_LAST_KIND, settlement.kind.name)
                .putLong(KEY_LAST_UPTIME_BUCKET, settlement.previousUptimeBucketMs)
                .putString(KEY_LAST_VERSION, settlement.previousVersion)
                .putBoolean(KEY_LAST_SAME_VERSION, settlement.sameVersion)
                .putBoolean(KEY_LAST_CRASH_SEEN, settlement.crashSeen)
                .commit(),
        ) { "failed to persist unclean process settlement" }
    }

    override fun readLastUnclean(): AndroidUncleanExitSettlement? {
        if (!preferences.contains(KEY_LAST_PID) ||
            !preferences.contains(KEY_LAST_STARTED_AT) ||
            !preferences.contains(KEY_LAST_OBSERVED_AT)
        ) return null
        val pid = preferences.getInt(KEY_LAST_PID, -1)
        val startedAt = preferences.getLong(KEY_LAST_STARTED_AT, -1L)
        val observedAt = preferences.getLong(KEY_LAST_OBSERVED_AT, -1L)
        if (pid <= 0 || startedAt < 0L || observedAt < startedAt) return null
        val kind = runCatching {
            AndroidProcessSessionSettlementKind.valueOf(
                preferences.getString(KEY_LAST_KIND, AndroidProcessSessionSettlementKind.UNCLEAN_EXIT.name)
                    ?: AndroidProcessSessionSettlementKind.UNCLEAN_EXIT.name,
            )
        }.getOrDefault(AndroidProcessSessionSettlementKind.UNCLEAN_EXIT)
        return AndroidUncleanExitSettlement(
            previousProcessId = pid,
            previousStartedAtMs = startedAt,
            observedAtMs = observedAt,
            kind = kind,
            previousUptimeBucketMs = preferences.getLong(KEY_LAST_UPTIME_BUCKET, 60_000L),
            previousVersion = preferences.getString(KEY_LAST_VERSION, "unknown") ?: "unknown",
            sameVersion = preferences.getBoolean(KEY_LAST_SAME_VERSION, true),
            crashSeen = preferences.getBoolean(KEY_LAST_CRASH_SEEN, false),
        )
    }

    companion object {
        private val VERSION_SHAPE = Regex("^[0-9A-Za-z.+-]{1,64}$")
        private const val KEY_ACTIVE_PID = "activePid"
        private const val KEY_ACTIVE_STARTED_AT = "activeStartedAtMs"
        private const val KEY_ACTIVE_ALIVE_AT = "activeAliveAtMs"
        private const val KEY_ACTIVE_VERSION = "activeVersion"
        private const val KEY_ACTIVE_CRASH_SEEN = "activeCrashSeen"
        private const val KEY_LAST_PID = "lastUncleanPid"
        private const val KEY_LAST_STARTED_AT = "lastUncleanStartedAtMs"
        private const val KEY_LAST_OBSERVED_AT = "lastUncleanObservedAtMs"
        private const val KEY_LAST_KIND = "lastSettlementKind"
        private const val KEY_LAST_UPTIME_BUCKET = "lastUptimeBucketMs"
        private const val KEY_LAST_VERSION = "lastVersion"
        private const val KEY_LAST_SAME_VERSION = "lastSameVersion"
        private const val KEY_LAST_CRASH_SEEN = "lastCrashSeen"

        fun production(context: Context): AndroidProcessSessionTelemetry {
            val application = context.applicationContext
            return AndroidProcessSessionTelemetry(
                store = SharedPreferencesAndroidProcessSessionMarkerStore(application),
                nowMs = System::currentTimeMillis,
                processId = Process::myPid,
                currentVersion = {
                    runCatching {
                        application.packageManager
                            .getPackageInfo(application.packageName, 0)
                            .versionName
                            ?: "unknown"
                    }.getOrDefault("unknown")
                },
            )
        }
    }
}
