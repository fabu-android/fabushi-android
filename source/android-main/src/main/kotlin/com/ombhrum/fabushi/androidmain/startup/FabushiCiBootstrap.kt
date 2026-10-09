package com.ombhrum.fabushi

import android.content.Context
import android.content.Intent
import android.system.Os
import org.json.JSONObject
import java.io.File

/**
 * Imports a bounded refresh-token-free GitHub Actions session before the native
 * Mahayana host is created.
 * Only the CI acceptance variant enables this path. Public release variants fail closed.
 * The installed App validates the staged session, moves it into private storage,
 * and sets the process-local variables consumed by the shared Rust product host.
 */
internal object FabushiCiBootstrap {
    private const val SessionFileName = "fabushi-ci-session.json"
    private const val MaxSessionBytes = 64 * 1024L
    private const val MaxLifetimeSeconds = 5 * 60 * 60L
    private val DeviceIdPattern = Regex("^gha-([0-9]+)-([0-9]+)-interactive$")
    private val SessionIdPattern = Regex("^ci-runner:([0-9]+):([0-9]+)$")

    const val ExtraRepository = "fabushi.ci.repository"
    const val ExtraWorkflow = "fabushi.ci.workflow"
    const val ExtraJob = "fabushi.ci.job"
    const val ExtraRunId = "fabushi.ci.run-id"
    const val ExtraRunAttempt = "fabushi.ci.run-attempt"
    const val ExtraSha = "fabushi.ci.sha"
    const val ExtraRunnerName = "fabushi.ci.runner-name"
    const val ExtraRunnerOs = "fabushi.ci.runner-os"
    const val ExtraRunnerArch = "fabushi.ci.runner-arch"
    const val ExtraDeviceName = "fabushi.ci.device-name"

    fun prepare(context: Context): Boolean {
        if (!BuildConfig.CI_ACCOUNT_SESSION_IMPORT_ENABLED) return false
        val nowEpochSeconds = System.currentTimeMillis() / 1000L
        val privateFile = File(context.filesDir, SessionFileName)
        val externalDirectory = context.getExternalFilesDir(null)
        val staged = externalDirectory?.let { File(it, SessionFileName) }

        val stagedDocument = staged
            ?.takeIf { it.isFile && it.length() in 1..MaxSessionBytes }
            ?.let { runCatching { JSONObject(it.readText(Charsets.UTF_8)) }.getOrNull() }
            ?.takeIf { isValidSession(it, nowEpochSeconds) }

        val privateDocument = privateFile
            .takeIf { it.isFile && it.length() in 1..MaxSessionBytes }
            ?.let { runCatching { JSONObject(it.readText(Charsets.UTF_8)) }.getOrNull() }
            ?.takeIf { isValidSession(it, nowEpochSeconds) }

        if (stagedDocument != null && staged != null) {
            runCatching {
                staged.inputStream().use { input ->
                    privateFile.outputStream().use { output -> input.copyTo(output) }
                }
                check(privateFile.length() in 1..MaxSessionBytes)
                Os.chmod(privateFile.absolutePath, 0x180) // 0600
                check(staged.delete() || !staged.exists())
            }.getOrElse {
                privateFile.delete()
                return false
            }
        } else if (privateDocument != null) {
            runCatching { Os.chmod(privateFile.absolutePath, 0x180) }.getOrElse {
                privateFile.delete()
                return false
            }
        } else {
            if (privateFile.exists()) privateFile.delete()
            return false
        }

        val activeDocument = runCatching {
            JSONObject(privateFile.readText(Charsets.UTF_8))
        }.getOrNull()?.takeIf { isValidSession(it, nowEpochSeconds) } ?: run {
            privateFile.delete()
            return false
        }

        return runCatching {
            Os.setenv("GITHUB_ACTIONS", "true", true)
            Os.setenv("FABUSHI_CI_ACCOUNT_SESSION_FILE", privateFile.absolutePath, true)
            check(
                context.getSharedPreferences("fabushi.mobile", 0)
                    .edit()
                    .putBoolean("onboarding-complete", true)
                    .commit(),
            )
            activeDocument.optString("sessionId").isNotBlank()
        }.getOrElse {
            privateFile.delete()
            false
        }
    }

    fun gatewayMetadata(intent: Intent?, ciBootstrapActive: Boolean): Map<String, String> {
        val metadata = linkedMapOf(
            "kind" to if (ciBootstrapActive) "github-actions-android-app" else "fabushi-android",
        )
        if (!ciBootstrapActive || intent == null) return metadata
        val mapping = listOf(
            ExtraRepository to "repository",
            ExtraWorkflow to "workflow",
            ExtraJob to "job",
            ExtraRunId to "runId",
            ExtraRunAttempt to "runAttempt",
            ExtraSha to "sha",
            ExtraRunnerName to "runnerName",
            ExtraRunnerOs to "runnerOs",
            ExtraRunnerArch to "runnerArch",
        )
        mapping.forEach { (extra, destination) ->
            intent.getStringExtra(extra)
                ?.trim()
                ?.takeIf { it.isNotEmpty() }
                ?.let { metadata[destination] = it.take(300) }
        }
        return metadata
    }

    fun configuredDeviceName(intent: Intent?, ciBootstrapActive: Boolean): String? {
        if (!ciBootstrapActive || intent == null) return null
        return intent.getStringExtra(ExtraDeviceName)
            ?.trim()
            ?.takeIf { it.isNotEmpty() }
            ?.take(200)
    }

    internal fun isValidSession(document: JSONObject, nowEpochSeconds: Long): Boolean {
        val accessToken = document.optString("accessToken").trim()
        val deviceId = document.optString("deviceId").trim()
        val sessionId = document.optString("sessionId").trim()
        val tokenType = document.optString("tokenType", "Bearer")
        val provider = document.optString("provider")
        val expiry = document.optLong("accessTokenExpiresAt", 0L)
        val deviceMatch = DeviceIdPattern.matchEntire(deviceId) ?: return false
        val sessionMatch = SessionIdPattern.matchEntire(sessionId) ?: return false

        if (accessToken.length !in 24..(16 * 1024) || accessToken.any(Char::isWhitespace)) return false
        if (tokenType != "Bearer" || provider != "github-actions" || !document.optBoolean("ciRunner", false)) return false
        if (document.has("refreshToken")) return false
        if (deviceMatch.groupValues[1] != sessionMatch.groupValues[1] || deviceMatch.groupValues[2] != sessionMatch.groupValues[2]) return false
        if (expiry <= nowEpochSeconds + 30L || expiry > nowEpochSeconds + MaxLifetimeSeconds) return false
        return true
    }
}
