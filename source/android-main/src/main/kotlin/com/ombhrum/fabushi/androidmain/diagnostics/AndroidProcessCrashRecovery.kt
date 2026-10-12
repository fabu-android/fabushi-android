package com.ombhrum.fabushi.androidmain.diagnostics

import android.content.Context
import android.os.Process
import android.os.SystemClock
import org.json.JSONObject
import java.io.File
import java.nio.charset.StandardCharsets
import kotlin.system.exitProcess

internal data class AndroidProcessCrashMarker(
    val kind: String,
    val occurredAtMs: Long,
    val uptimeMs: Long,
)

/**
 * Minimal app-private crash handoff.
 *
 * The marker deliberately excludes Throwable message/stack/content. The next process clears it
 * only after the canonical Host accepts the bounded crash projection.
 */
internal class AndroidProcessCrashRecovery(context: Context) {
    private val directory = File(context.filesDir, "telemetry")
    private val markerFile = File(directory, "android-process-crash.v1.json")

    @Volatile
    private var installedHandler: Thread.UncaughtExceptionHandler? = null

    fun install() {
        if (installedHandler != null) return
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        val handler = Thread.UncaughtExceptionHandler { thread, throwable ->
            runCatching {
                persist(
                    AndroidProcessCrashMarker(
                        kind = "uncaught_exception",
                        occurredAtMs = System.currentTimeMillis().coerceAtLeast(0L),
                        uptimeMs = SystemClock.elapsedRealtime().coerceAtLeast(0L),
                    ),
                )
            }
            if (previous != null) {
                previous.uncaughtException(thread, throwable)
            } else {
                Process.killProcess(Process.myPid())
                exitProcess(10)
            }
        }
        installedHandler = handler
        Thread.setDefaultUncaughtExceptionHandler(handler)
    }

    fun pending(): AndroidProcessCrashMarker? {
        val raw = runCatching { markerFile.readText(StandardCharsets.UTF_8) }.getOrNull() ?: return null
        val parsed = runCatching { JSONObject(raw) }.getOrNull() ?: return null
        if (parsed.optInt("schemaVersion", 0) != 1) return null
        val kind = parsed.optString("kind").takeIf { it == "uncaught_exception" } ?: return null
        val occurredAtMs = parsed.optLong("occurredAtMs", -1L).takeIf { it >= 0L } ?: return null
        val uptimeMs = parsed.optLong("uptimeMs", -1L).takeIf { it >= 0L } ?: return null
        return AndroidProcessCrashMarker(kind, occurredAtMs, uptimeMs)
    }

    fun acknowledge(marker: AndroidProcessCrashMarker): Boolean {
        val current = pending() ?: return true
        if (current != marker) return false
        return !markerFile.exists() || markerFile.delete()
    }

    internal fun persist(marker: AndroidProcessCrashMarker) {
        require(marker.kind == "uncaught_exception") { "unsupported process crash kind" }
        require(marker.occurredAtMs >= 0L && marker.uptimeMs >= 0L) {
            "process crash timestamps must be nonnegative"
        }
        directory.mkdirs()
        check(directory.isDirectory) { "process crash marker directory unavailable" }
        val temporary = File(directory, markerFile.name + ".tmp")
        val payload = JSONObject()
            .put("schemaVersion", 1)
            .put("kind", marker.kind)
            .put("occurredAtMs", marker.occurredAtMs)
            .put("uptimeMs", marker.uptimeMs)
            .toString()
            .toByteArray(StandardCharsets.UTF_8)
        temporary.outputStream().use { stream ->
            stream.write(payload)
            stream.flush()
            stream.fd.sync()
        }
        if (!temporary.renameTo(markerFile)) {
            temporary.delete()
            error("failed to atomically publish Android process crash marker")
        }
    }
}
