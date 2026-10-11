package com.ombhrum.fabushi

import android.content.Context
import org.json.JSONObject

enum class AndroidUpdateApplyPhase(val wireName: String) {
    STAGED("staged"),
    REQUESTED("requested"),
    SPAWN_FAILED("spawn-failed");

    companion object {
        fun fromWire(value: String): AndroidUpdateApplyPhase? =
            entries.firstOrNull { it.wireName == value }
    }
}

data class AndroidUpdateApplyMarker(
    val phase: AndroidUpdateApplyPhase,
    val targetVersion: String,
    val targetVersionCode: Long,
    val fromVersion: String,
    val fromVersionCode: Long,
    val apkName: String,
    val sha256: String,
    val size: Long,
    val recordedAtMs: Long,
)

enum class AndroidUpdateRecoveryDecision {
    CONFIRMED,
    RECOVER_STAGED,
    OUTCOME_UNKNOWN,
    RETRYABLE_FAILURE,
    ORPHANED,
}

object AndroidUpdateRecoveryPolicy {
    private const val MAX_MARKER_AGE_MS = 30L * 24L * 60L * 60L * 1_000L

    fun decide(
        marker: AndroidUpdateApplyMarker,
        currentVersion: String,
        currentVersionCode: Long,
        nowMs: Long,
    ): AndroidUpdateRecoveryDecision {
        val ageMs = nowMs - marker.recordedAtMs
        if (ageMs < 0L || ageMs > MAX_MARKER_AGE_MS) return AndroidUpdateRecoveryDecision.ORPHANED
        if (currentVersion == marker.targetVersion && currentVersionCode >= marker.targetVersionCode) {
            return AndroidUpdateRecoveryDecision.CONFIRMED
        }
        if (currentVersion != marker.fromVersion || currentVersionCode != marker.fromVersionCode) {
            return AndroidUpdateRecoveryDecision.ORPHANED
        }
        return when (marker.phase) {
            AndroidUpdateApplyPhase.STAGED -> AndroidUpdateRecoveryDecision.RECOVER_STAGED
            AndroidUpdateApplyPhase.REQUESTED -> AndroidUpdateRecoveryDecision.OUTCOME_UNKNOWN
            AndroidUpdateApplyPhase.SPAWN_FAILED -> AndroidUpdateRecoveryDecision.RETRYABLE_FAILURE
        }
    }
}

internal class AndroidUpdateApplyStore(context: Context) {
    private val preferences =
        context.getSharedPreferences("fabushi-app-update-apply-v1", Context.MODE_PRIVATE)

    fun read(): AndroidUpdateApplyMarker? {
        val raw = preferences.getString(KEY, null) ?: return null
        return runCatching {
            val value = JSONObject(raw)
            require(value.optInt("schemaVersion") == 1)
            val phase = AndroidUpdateApplyPhase.fromWire(value.getString("phase"))
                ?: error("Unknown update apply phase")
            val marker = AndroidUpdateApplyMarker(
                phase = phase,
                targetVersion = value.getString("targetVersion"),
                targetVersionCode = value.getLong("targetVersionCode"),
                fromVersion = value.getString("fromVersion"),
                fromVersionCode = value.getLong("fromVersionCode"),
                apkName = value.getString("apkName"),
                sha256 = value.getString("sha256"),
                size = value.getLong("size"),
                recordedAtMs = value.getLong("recordedAtMs"),
            )
            require(marker.targetVersion.isNotBlank() && marker.fromVersion.isNotBlank())
            require(marker.targetVersionCode > 0L && marker.fromVersionCode > 0L)
            require(marker.apkName.matches(Regex("^[A-Za-z0-9._-]+[.]apk$")))
            require(marker.sha256.matches(Regex("^[a-f0-9]{64}$")))
            require(marker.size > 0L)
            marker
        }.getOrElse {
            clear()
            null
        }
    }

    fun write(marker: AndroidUpdateApplyMarker) {
        val raw = JSONObject()
            .put("schemaVersion", 1)
            .put("phase", marker.phase.wireName)
            .put("targetVersion", marker.targetVersion)
            .put("targetVersionCode", marker.targetVersionCode)
            .put("fromVersion", marker.fromVersion)
            .put("fromVersionCode", marker.fromVersionCode)
            .put("apkName", marker.apkName)
            .put("sha256", marker.sha256)
            .put("size", marker.size)
            .put("recordedAtMs", marker.recordedAtMs)
            .toString()
        check(preferences.edit().putString(KEY, raw).commit()) {
            "Unable to persist update apply marker before external installer handoff"
        }
    }

    fun clear() {
        check(preferences.edit().remove(KEY).commit()) {
            "Unable to clear update apply marker"
        }
    }

    private companion object {
        const val KEY = "marker"
    }
}
