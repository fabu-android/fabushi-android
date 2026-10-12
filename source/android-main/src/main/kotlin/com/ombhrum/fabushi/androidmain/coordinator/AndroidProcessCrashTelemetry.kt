package com.ombhrum.fabushi.androidmain.coordinator

import android.content.Context
import android.os.Process
import android.os.SystemClock
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

internal data class AndroidProcessCrashMarker(
    val kind: String,
    val errorClass: String,
    val uptimeMs: Long,
    val occurredAtMs: Long,
) {
    fun toJson(): JSONObject =
        JSONObject()
            .put("version", 1)
            .put("kind", kind)
            .put("errorClass", errorClass)
            .put("uptimeMs", uptimeMs.coerceAtLeast(0L))
            .put("occurredAtMs", occurredAtMs.coerceAtLeast(0L))

    companion object {
        fun parse(raw: String): AndroidProcessCrashMarker? {
            val value = runCatching { JSONObject(raw) }.getOrNull() ?: return null
            if (value.optInt("version") != 1) return null
            val kind = value.optString("kind")
            if (kind != "uncaughtException") return null
            val errorClass = boundedCrashClass(value.optString("errorClass")) ?: return null
            val uptimeMs = value.optLong("uptimeMs", -1L)
            val occurredAtMs = value.optLong("occurredAtMs", -1L)
            if (uptimeMs < 0L || occurredAtMs < 0L) return null
            return AndroidProcessCrashMarker(kind, errorClass, uptimeMs, occurredAtMs)
        }
    }
}

/**
 * Android crash ingress. It persists exactly one bounded marker and always delegates to the
 * previous uncaught-exception handler. It never stores message, stack, request payload, account
 * identity, or credentials. Host becomes the canonical telemetry owner after next-start ingestion.
 */
internal class AndroidProcessCrashTelemetry(
    context: Context,
) : AutoCloseable {
    private val file = AtomicFile(File(context.noBackupFilesDir, "process-crash-telemetry.v1.json"))
    private val installed = AtomicBoolean(false)
    private var installedHandler: Thread.UncaughtExceptionHandler? = null
    private var previousHandler: Thread.UncaughtExceptionHandler? = null

    fun install() {
        if (!installed.compareAndSet(false, true)) return
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        val handler = Thread.UncaughtExceptionHandler { thread, throwable ->
            runCatching {
                persist(
                    AndroidProcessCrashMarker(
                        kind = "uncaughtException",
                        errorClass = boundedCrashClass(throwable.javaClass.simpleName) ?: "Throwable",
                        uptimeMs = SystemClock.elapsedRealtime(),
                        occurredAtMs = System.currentTimeMillis(),
                    ),
                )
            }
            if (previous != null) {
                previous.uncaughtException(thread, throwable)
            } else {
                Process.killProcess(Process.myPid())
            }
        }
        previousHandler = previous
        installedHandler = handler
        Thread.setDefaultUncaughtExceptionHandler(handler)
    }

    fun pending(): AndroidProcessCrashMarker? {
        val raw = try {
            file.openRead().bufferedReader(Charsets.UTF_8).use { it.readText() }
        } catch (_: java.io.FileNotFoundException) {
            return null
        } catch (_: Throwable) {
            return null
        }
        val marker = AndroidProcessCrashMarker.parse(raw)
        if (marker == null) runCatching { file.delete() }
        return marker
    }

    fun acknowledge(marker: AndroidProcessCrashMarker): Boolean {
        val current = pending() ?: return true
        if (current != marker) return false
        return runCatching {
            file.delete()
            true
        }.getOrDefault(false)
    }

    private fun persist(marker: AndroidProcessCrashMarker) {
        val stream = file.startWrite()
        try {
            stream.writer(Charsets.UTF_8).use { writer ->
                writer.write(marker.toJson().toString())
                writer.flush()
            }
            file.finishWrite(stream)
        } catch (error: Throwable) {
            file.failWrite(stream)
            throw error
        }
    }

    override fun close() {
        val handler = installedHandler
        if (handler != null && Thread.getDefaultUncaughtExceptionHandler() === handler) {
            Thread.setDefaultUncaughtExceptionHandler(previousHandler)
        }
        installedHandler = null
        previousHandler = null
        installed.set(false)
    }
}

internal fun boundedCrashClass(value: String): String? {
    val normalized = value.trim()
    if (normalized.isEmpty() || normalized.length > 96) return null
    if (!normalized.all { it.isLetterOrDigit() || it == '_' || it == '.' || it == '$' }) return null
    return normalized
}
