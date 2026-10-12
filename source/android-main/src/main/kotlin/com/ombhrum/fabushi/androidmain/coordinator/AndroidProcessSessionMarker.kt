package com.ombhrum.fabushi.androidmain.coordinator

import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.Properties

internal const val PROCESS_SESSION_HEARTBEAT_INTERVAL_MS = 60_000L
internal const val PROCESS_SESSION_ORPHAN_BOUND_MS = 30L * 24L * 60L * 60L * 1_000L

internal enum class AndroidProcessSessionSettlementKind {
    UNCLEAN_EXIT,
    MARKER_ORPHANED,
}

internal data class AndroidProcessSessionSettlement(
    val kind: AndroidProcessSessionSettlementKind,
    val previousUptimeBucketMs: Long,
    val previousVersion: String,
    val sameVersion: Boolean,
    val crashSeen: Boolean,
    val previousStartedAtMs: Long,
)

internal class AndroidProcessSessionMarker(
    private val markerFile: File,
    appVersion: String,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {
    private data class Marker(
        val version: Int,
        val appVersion: String,
        val startedAtMs: Long,
        val aliveAtMs: Long,
        val crashSeen: Boolean,
    )

    private enum class State { IDLE, ARMED, SETTLED }

    private val currentVersion = appVersion.takeIf(::isValidVersion) ?: "unknown"
    private var state = State.IDLE
    private var armed: Marker? = null

    fun settlePriorAndArm(): AndroidProcessSessionSettlement? {
        check(state == State.IDLE) { "process session marker already settled" }
        val now = nowMs()
        val prior = readMarker()
        val settlement = prior?.let { marker ->
            val aliveAt = maxOf(marker.startedAtMs, marker.aliveAtMs)
            val ageMs = (now - aliveAt).coerceAtLeast(0L)
            AndroidProcessSessionSettlement(
                kind = if (ageMs > PROCESS_SESSION_ORPHAN_BOUND_MS) {
                    AndroidProcessSessionSettlementKind.MARKER_ORPHANED
                } else {
                    AndroidProcessSessionSettlementKind.UNCLEAN_EXIT
                },
                previousUptimeBucketMs = relaunchGapBucketMs(
                    (aliveAt - marker.startedAtMs).coerceAtLeast(0L),
                ),
                previousVersion = marker.appVersion,
                sameVersion = marker.appVersion == currentVersion,
                crashSeen = marker.crashSeen,
                previousStartedAtMs = marker.startedAtMs,
            )
        }
        val current = Marker(
            version = 1,
            appVersion = currentVersion,
            startedAtMs = now,
            aliveAtMs = now,
            crashSeen = false,
        )
        writeMarker(current)
        armed = current
        state = State.ARMED
        return settlement
    }

    fun heartbeat() {
        if (state != State.ARMED) return
        val current = armed ?: return
        val next = current.copy(aliveAtMs = maxOf(current.aliveAtMs, nowMs()))
        writeMarker(next)
        armed = next
    }

    fun noteCrashSeen() {
        if (state != State.ARMED) return
        val current = armed ?: return
        if (current.crashSeen) return
        val next = current.copy(aliveAtMs = maxOf(current.aliveAtMs, nowMs()), crashSeen = true)
        writeMarker(next)
        armed = next
    }

    fun settleClean() {
        if (state != State.ARMED) return
        state = State.SETTLED
        armed = null
        Files.deleteIfExists(markerFile.toPath())
        Files.deleteIfExists(temporaryFile().toPath())
    }

    private fun readMarker(): Marker? {
        if (!markerFile.exists()) return null
        val properties = Properties()
        return runCatching {
            FileInputStream(markerFile).use(properties::load)
            val version = properties.getProperty("version")?.toIntOrNull()
                ?: error("process session marker version is missing")
            val appVersion = properties.getProperty("appVersion")
                ?.takeIf(::isValidVersion)
                ?: error("process session marker appVersion is invalid")
            val startedAtMs = properties.getProperty("startedAtMs")?.toLongOrNull()
                ?.takeIf { it >= 0L }
                ?: error("process session marker startedAtMs is invalid")
            val aliveAtMs = properties.getProperty("aliveAtMs")?.toLongOrNull()
                ?.takeIf { it >= startedAtMs }
                ?: error("process session marker aliveAtMs is invalid")
            val crashSeen = properties.getProperty("crashSeen")?.let { value ->
                when (value) {
                    "true" -> true
                    "false" -> false
                    else -> error("process session marker crashSeen is invalid")
                }
            } ?: error("process session marker crashSeen is missing")
            check(version == 1) { "unsupported process session marker version" }
            Marker(version, appVersion, startedAtMs, aliveAtMs, crashSeen)
        }.getOrElse {
            Files.deleteIfExists(markerFile.toPath())
            null
        }
    }

    private fun writeMarker(marker: Marker) {
        markerFile.parentFile?.mkdirs()
        val properties = Properties().apply {
            setProperty("version", marker.version.toString())
            setProperty("appVersion", marker.appVersion)
            setProperty("startedAtMs", marker.startedAtMs.toString())
            setProperty("aliveAtMs", marker.aliveAtMs.toString())
            setProperty("crashSeen", marker.crashSeen.toString())
        }
        val temporary = temporaryFile()
        try {
            FileOutputStream(temporary).use { stream ->
                properties.store(stream, null)
                stream.fd.sync()
            }
            try {
                Files.move(
                    temporary.toPath(),
                    markerFile.toPath(),
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(
                    temporary.toPath(),
                    markerFile.toPath(),
                    StandardCopyOption.REPLACE_EXISTING,
                )
            }
        } catch (error: Throwable) {
            Files.deleteIfExists(temporary.toPath())
            throw error
        }
    }

    private fun temporaryFile(): File = File(markerFile.parentFile, markerFile.name + ".tmp")

    private fun relaunchGapBucketMs(uptimeMs: Long): Long {
        val ceilings = longArrayOf(
            60_000L,
            300_000L,
            1_800_000L,
            21_600_000L,
            86_400_000L,
            PROCESS_SESSION_ORPHAN_BOUND_MS,
        )
        return ceilings.firstOrNull { uptimeMs < it } ?: PROCESS_SESSION_ORPHAN_BOUND_MS
    }

    private fun isValidVersion(value: String): Boolean =
        value.length in 1..64 &&
            value.all { character ->
                character.isLetterOrDigit() || character == '.' || character == '+' || character == '-'
            }
}
