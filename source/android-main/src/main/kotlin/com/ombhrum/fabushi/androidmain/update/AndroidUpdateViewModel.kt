package com.ombhrum.fabushi

import android.app.Application
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.os.SystemClock
import android.provider.Settings
import androidx.core.content.FileProvider
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidUpdateTrackPolicy
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

enum class AndroidUpdatePhase {
    DISABLED,
    CHECKING,
    UP_TO_DATE,
    AVAILABLE,
    DOWNLOADING,
    WAITING_FOR_PERMISSION,
    INSTALLING,
    OUTCOME_UNKNOWN,
    ERROR,
}

data class AndroidUpdateUiState(
    val phase: AndroidUpdatePhase,
    val currentVersion: String,
    val currentVersionCode: Long = BuildConfig.VERSION_CODE.toLong(),
    val availableVersion: String? = null,
    val availableVersionCode: Long? = null,
    val progressPercent: Int? = null,
    val message: String? = null,
)

data class AndroidMandatoryUpdateState(
    val currentVersion: String,
    val minimumVersion: String? = null,
    val backendRequiresUpdate: Boolean = false,
    val required: Boolean = false,
)

object AndroidMandatoryUpdatePolicy {
    fun isRequired(
        currentVersion: String,
        minimumVersion: String?,
        backendRequiresUpdate: Boolean,
    ): Boolean {
        if (backendRequiresUpdate) return true
        val minimum = minimumVersion?.trim()?.takeIf(String::isNotEmpty) ?: return false
        return runCatching { AndroidVersion.compare(currentVersion, minimum) < 0 }.getOrDefault(false)
    }
}

internal data class AndroidUpdateCandidate(
    val version: String,
    val versionCode: Long,
    val tag: String,
    val apkName: String,
    val apkUrl: String,
    val sha256: String,
    val size: Long,
    val notes: String?,
)

object AndroidVersion {
    private val releasePattern = Regex("^(\\d+)\\.(\\d+)\\.(\\d+)$")
    private val numericIdentifierPattern = Regex("^\\d+$")

    private data class Parsed(
        val release: List<java.math.BigInteger>,
        val preRelease: List<String>,
    )

    fun compare(left: String, right: String): Int {
        val leftParsed = parse(left)
            ?: throw IllegalArgumentException("Cannot compare versions \"$left\" and \"$right\"")
        val rightParsed = parse(right)
            ?: throw IllegalArgumentException("Cannot compare versions \"$left\" and \"$right\"")

        repeat(3) { index ->
            val result = leftParsed.release[index].compareTo(rightParsed.release[index])
            if (result != 0) return result.sign()
        }
        if (leftParsed.preRelease.isEmpty() && rightParsed.preRelease.isEmpty()) return 0
        if (leftParsed.preRelease.isEmpty()) return 1
        if (rightParsed.preRelease.isEmpty()) return -1

        val count = minOf(leftParsed.preRelease.size, rightParsed.preRelease.size)
        repeat(count) { index ->
            val result = compareIdentifier(
                leftParsed.preRelease[index],
                rightParsed.preRelease[index],
            )
            if (result != 0) return result
        }
        return (leftParsed.preRelease.size - rightParsed.preRelease.size).sign()
    }

    fun isNewer(
        remoteVersion: String,
        remoteVersionCode: Long,
        currentVersion: String,
        currentVersionCode: Long,
    ): Boolean {
        val versionResult = compare(remoteVersion, currentVersion)
        return versionResult > 0 || (versionResult == 0 && remoteVersionCode > currentVersionCode)
    }

    fun isPrerelease(version: String): Boolean = parse(version)?.preRelease?.isNotEmpty() == true

    private fun parse(value: String): Parsed? {
        val hyphenIndex = value.indexOf('-')
        val releasePart = if (hyphenIndex == -1) value else value.substring(0, hyphenIndex)
        val prereleasePart = if (hyphenIndex == -1) null else value.substring(hyphenIndex + 1)
        val match = releasePattern.matchEntire(releasePart) ?: return null
        val preRelease = prereleasePart?.split('.') ?: emptyList()
        if (preRelease.any(String::isEmpty)) return null
        return Parsed(
            release = match.groupValues.drop(1).map { java.math.BigInteger(it) },
            preRelease = preRelease,
        )
    }

    private fun compareIdentifier(left: String, right: String): Int {
        val leftNumeric = numericIdentifierPattern.matches(left)
        val rightNumeric = numericIdentifierPattern.matches(right)
        if (leftNumeric && rightNumeric) {
            return java.math.BigInteger(left).compareTo(java.math.BigInteger(right)).sign()
        }
        if (leftNumeric) return -1
        if (rightNumeric) return 1
        return left.compareTo(right).sign()
    }

    private fun Int.sign(): Int = when {
        this < 0 -> -1
        this > 0 -> 1
        else -> 0
    }
}

class AndroidUpdateViewModel(application: Application) : AndroidViewModel(application) {
    companion object {
        private const val RELEASES_API = "https://api.github.com/repos/bhrumom/fabushi/releases?per_page=100"
        private const val UPDATE_MANIFEST_NAME = "fabushi-android-update.json"
        private const val REPOSITORY = "bhrumom/fabushi"
        private const val UPDATE_CHECK_MIN_INTERVAL_MS = 60_000L
        private const val UPDATE_FOREGROUND_INTERVAL_MS = 5 * 60_000L
        private const val STARTUP_DELAY_MS = 4_000L
        private const val AUTO_STAGE_IDLE_THRESHOLD_MS = 10 * 60_000L
        private const val AUTO_STAGE_POLL_MS = 60_000L
        private const val UPDATE_PREFS = "fabushi.android.update"
        private const val AUTO_STAGE_OPT_IN_KEY = "auto-update-when-idle-opt-in"
        private const val MAX_METADATA_BYTES = 2 * 1024 * 1024
        private const val MAX_MANIFEST_BYTES = 128 * 1024
        private const val MAX_APK_BYTES = 768L * 1024L * 1024L
        private val APK_NAME = Regex("^[A-Za-z0-9._-]+[.]apk$")
        private val SHA256 = Regex("^[a-f0-9]{64}$")
        private const val ANDROID_BUILD_DEFAULT_TRACK = "stable"
    }

    private val applicationContext = getApplication<Application>()
    private val updatesEnabled = BuildConfig.GITHUB_UPDATES_ENABLED && !BuildConfig.DEBUG
    private val coordinator by lazy { AndroidCoordinatorBridge.presentation() }
    private val applyStore = AndroidUpdateApplyStore(applicationContext)
    private val updatePreferences = applicationContext.getSharedPreferences(UPDATE_PREFS, Context.MODE_PRIVATE)
    private val updaterTelemetry = AndroidUpdaterTelemetry()
    private val currentVersion = BuildConfig.VERSION_NAME
    private val currentVersionCode = BuildConfig.VERSION_CODE.toLong()
    private val _state = MutableStateFlow(
        AndroidUpdateUiState(
            phase = if (updatesEnabled) AndroidUpdatePhase.CHECKING else AndroidUpdatePhase.DISABLED,
            currentVersion = currentVersion,
            currentVersionCode = currentVersionCode,
            message = if (updatesEnabled) "准备检查 GitHub Release…" else null,
        ),
    )
    val state: StateFlow<AndroidUpdateUiState> = _state.asStateFlow()

    private val _mandatoryUpdate = MutableStateFlow(
        AndroidMandatoryUpdateState(currentVersion = currentVersion),
    )
    val mandatoryUpdate: StateFlow<AndroidMandatoryUpdateState> = _mandatoryUpdate.asStateFlow()

    private val _trackPolicy = MutableStateFlow(resolveUpdateTrackPolicy())
    val trackPolicy: StateFlow<AndroidUpdateTrackPolicy> = _trackPolicy.asStateFlow()

    private val _autoUpdateWhenIdleOptIn = MutableStateFlow(
        updatePreferences.getBoolean(AUTO_STAGE_OPT_IN_KEY, false),
    )
    val autoUpdateWhenIdleOptIn: StateFlow<Boolean> = _autoUpdateWhenIdleOptIn.asStateFlow()

    private val _autoUpdateWhenIdleGateEnabled = MutableStateFlow(false)
    val autoUpdateWhenIdleGateEnabled: StateFlow<Boolean> = _autoUpdateWhenIdleGateEnabled.asStateFlow()

    fun setAutoUpdateWhenIdleOptIn(enabled: Boolean) {
        updatePreferences.edit().putBoolean(AUTO_STAGE_OPT_IN_KEY, enabled).apply()
        _autoUpdateWhenIdleOptIn.value = enabled
        updaterTelemetry.record(
            AndroidUpdaterTelemetryEvent(
                category = "idle",
                action = "opt_in",
                result = if (enabled) "enabled" else "disabled",
                currentVersion = currentVersion,
            ),
        )
    }

    internal fun updaterTelemetrySnapshot(): List<AndroidUpdaterTelemetryEvent> = updaterTelemetry.snapshot()

    fun noteMinimumRequiredVersion(minimumVersion: String?) {
        publishMandatoryUpdate(
            minimumVersion = minimumVersion?.trim()?.takeIf(String::isNotEmpty),
            backendRequiresUpdate = _mandatoryUpdate.value.backendRequiresUpdate,
        )
    }

    fun noteBackendUpdateRequirement(required: Boolean) {
        publishMandatoryUpdate(
            minimumVersion = _mandatoryUpdate.value.minimumVersion,
            backendRequiresUpdate = required,
        )
    }

    private fun publishMandatoryUpdate(
        minimumVersion: String?,
        backendRequiresUpdate: Boolean,
    ) {
        _mandatoryUpdate.value = AndroidMandatoryUpdateState(
            currentVersion = currentVersion,
            minimumVersion = minimumVersion,
            backendRequiresUpdate = backendRequiresUpdate,
            required = AndroidMandatoryUpdatePolicy.isRequired(
                currentVersion = currentVersion,
                minimumVersion = minimumVersion,
                backendRequiresUpdate = backendRequiresUpdate,
            ),
        )
    }

    private var candidate: AndroidUpdateCandidate? = null
    private var pendingApk: File? = null
    private var awaitingInstallPermission = false
    private var checkJob: Job? = null
    private var downloadJob: Job? = null
    private var foregroundJob: Job? = null
    private var idleStageJob: Job? = null
    private var backgroundSinceElapsedMs: Long? = null
    private var autoUpdateGateObserved = false
    private var lastCheckAtMs = 0L

    init {
        restorePendingApply()
    }

    private fun resolveUpdateTrackPolicy(): AndroidUpdateTrackPolicy =
        coordinator.resolveUpdateTrackPolicy(
            managedTrack = null,
            unlockInternalTracks = false,
            userOverride = null,
            buildDefault = ANDROID_BUILD_DEFAULT_TRACK,
        )

    private fun refreshUpdateTrackPolicy(): AndroidUpdateTrackPolicy =
        resolveUpdateTrackPolicy().also { projection ->
            require(projection.effectiveTrack in projection.availableTracks) {
                "Rust update-track policy returned an unavailable effective track"
            }
            _trackPolicy.value = projection
        }

    private suspend fun refreshMandatoryUpdatePolicy() {
        val projection = runCatching {
            withContext(Dispatchers.IO) { coordinator.refreshMandatoryUpdatePolicy() }
        }.getOrElse {
            // Never let an earlier true experiment gate survive a failed refresh in presentation
            // memory. The next successful Coordinator projection may explicitly re-enable it.
            autoUpdateGateObserved = false
            _autoUpdateWhenIdleGateEnabled.value = false
            return
        }
        publishMandatoryUpdate(
            minimumVersion = projection.minimumVersion,
            backendRequiresUpdate = projection.backendRequiresUpdate,
        )
        autoUpdateGateObserved = projection.autoUpdateWhenIdleGateObserved
        _autoUpdateWhenIdleGateEnabled.value =
            projection.autoUpdateWhenIdleGateObserved && projection.autoUpdateWhenIdleGateEnabled
    }

    fun setForeground(foreground: Boolean) {
        if (!updatesEnabled) return
        if (!foreground) {
            foregroundJob?.cancel()
            foregroundJob = null
            backgroundSinceElapsedMs = SystemClock.elapsedRealtime()
            startIdleAutoStageMonitor()
            return
        }
        idleStageJob?.cancel()
        idleStageJob = null
        backgroundSinceElapsedMs = null
        if (_state.value.phase == AndroidUpdatePhase.INSTALLING) {
            applyStore.read()
                ?.takeIf { it.phase == AndroidUpdateApplyPhase.REQUESTED }
                ?.let { marker ->
                    // The old process only stays quiesced while Android owns the installer
                    // handoff. If the installer returns without a provable version change,
                    // restore Host turns but keep the install outcome unknown and never replay.
                    setHostUpgradeQuiescing(false)
                    _state.value = AndroidUpdateUiState(
                        phase = AndroidUpdatePhase.OUTCOME_UNKNOWN,
                        currentVersion = currentVersion,
                        currentVersionCode = currentVersionCode,
                        availableVersion = marker.targetVersion,
                        availableVersionCode = marker.targetVersionCode,
                        message = "系统安装器已返回，但当前进程无法证明安装结果。不会自动重放安装；请显式重新验证并继续。",
                    )
                }
        }
        if (awaitingInstallPermission && canInstallPackages()) {
            pendingApk?.takeIf(File::exists)?.let(::requestPackageInstall)
        }
        if (foregroundJob?.isActive == true) return
        foregroundJob = viewModelScope.launch {
            var startupDelayPending = lastCheckAtMs == 0L
            while (isActive) {
                refreshMandatoryUpdatePolicy()
                if (startupDelayPending) {
                    startupDelayPending = false
                    delay(STARTUP_DELAY_MS)
                }
                checkForUpdates()
                delay(UPDATE_FOREGROUND_INTERVAL_MS)
            }
        }
    }

    private fun startIdleAutoStageMonitor() {
        if (idleStageJob?.isActive == true) return
        idleStageJob = viewModelScope.launch {
            while (isActive) {
                delay(AUTO_STAGE_POLL_MS)
                refreshMandatoryUpdatePolicy()
                maybeAutoStageWhenIdle()
            }
        }
    }

    private suspend fun maybeAutoStageWhenIdle() {
        val backgroundSince = backgroundSinceElapsedMs ?: return
        val track = runCatching { refreshUpdateTrackPolicy() }.getOrNull() ?: return
        val userAway = SystemClock.elapsedRealtime() - backgroundSince >= AUTO_STAGE_IDLE_THRESHOLD_MS &&
            isDeviceAway()
        val hostIdle = runCatching {
            withContext(Dispatchers.IO) { coordinator.isUpdateHostIdle() }
        }.getOrDefault(false)
        val busy = pendingApk != null ||
            awaitingInstallPermission ||
            downloadJob?.isActive == true ||
            checkJob?.isActive == true ||
            _state.value.phase in setOf(
                AndroidUpdatePhase.DOWNLOADING,
                AndroidUpdatePhase.INSTALLING,
                AndroidUpdatePhase.OUTCOME_UNKNOWN,
            )
        val eligible = AndroidUpdateIdlePolicy.canAutoStage(
            AndroidUpdateIdleInputs(
                optInEnabled = _autoUpdateWhenIdleOptIn.value,
                gateObserved = autoUpdateGateObserved,
                gateEnabled = _autoUpdateWhenIdleGateEnabled.value,
                signedTransportAvailable = track.effectiveTrack == "stable",
                userAwayForThreshold = userAway,
                hostConfirmedIdle = hostIdle,
                updateOperationBusy = busy,
            ),
        )
        updaterTelemetry.record(
            AndroidUpdaterTelemetryEvent(
                category = "idle",
                action = "gate",
                result = if (eligible) "eligible" else "skipped",
                track = track.effectiveTrack,
                currentVersion = currentVersion,
                phase = _state.value.phase.name.lowercase(),
            ),
        )
        if (!eligible) return
        val found = runCatching { withContext(Dispatchers.IO) { findLatestUpdate() } }
            .getOrElse { error ->
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "check",
                        action = "result",
                        result = "error",
                        trigger = "idle",
                        track = track.effectiveTrack,
                        currentVersion = currentVersion,
                        errorClass = error.javaClass.simpleName,
                    ),
                )
                return
            } ?: return
        candidate = found
        autoStageCandidate(found)
    }

    private fun autoStageCandidate(release: AndroidUpdateCandidate) {
        if (downloadJob?.isActive == true || pendingApk != null) return
        downloadJob = viewModelScope.launch {
            val started = SystemClock.elapsedRealtime()
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.DOWNLOADING,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                availableVersion = release.version,
                availableVersionCode = release.versionCode,
                progressPercent = 0,
                message = "Fabushi 已在安全空闲条件下准备签名更新；不会在后台打开系统安装器。",
            )
            try {
                val apk = withContext(Dispatchers.IO) { downloadAndVerify(release) }
                pendingApk = apk
                applyStore.write(markerFor(release, AndroidUpdateApplyPhase.STAGED))
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.AVAILABLE,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = release.version,
                    availableVersionCode = release.versionCode,
                    message = "已在空闲时下载并验证更新。返回 Fabushi 后请显式继续系统安装。",
                )
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "stage",
                        action = "verified",
                        result = "success",
                        trigger = "idle",
                        track = "stable",
                        currentVersion = currentVersion,
                        targetVersion = release.version,
                        durationMs = SystemClock.elapsedRealtime() - started,
                    ),
                )
            } catch (error: Exception) {
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.ERROR,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = release.version,
                    availableVersionCode = release.versionCode,
                    message = "空闲更新准备失败：${error.message ?: error.javaClass.simpleName}",
                )
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "stage",
                        action = "verified",
                        result = "error",
                        trigger = "idle",
                        track = "stable",
                        currentVersion = currentVersion,
                        targetVersion = release.version,
                        durationMs = SystemClock.elapsedRealtime() - started,
                        errorClass = error.javaClass.simpleName,
                    ),
                )
            }
        }
    }

    private fun isDeviceAway(): Boolean {
        val power = applicationContext.getSystemService(PowerManager::class.java)
        val keyguard = applicationContext.getSystemService(KeyguardManager::class.java)
        return power?.isInteractive == false || keyguard?.isDeviceLocked == true
    }

    fun checkForUpdates(force: Boolean = false) {
        if (!updatesEnabled || awaitingInstallPermission || pendingApk != null) return
        if (downloadJob?.isActive == true || checkJob?.isActive == true) return
        val now = System.currentTimeMillis()
        if (!force && now - lastCheckAtMs < UPDATE_CHECK_MIN_INTERVAL_MS) return
        lastCheckAtMs = now
        checkJob = viewModelScope.launch {
            val checkStarted = SystemClock.elapsedRealtime()
            updaterTelemetry.record(
                AndroidUpdaterTelemetryEvent(
                    category = "check",
                    action = "started",
                    result = "pending",
                    trigger = if (force) "manual" else "foreground",
                    track = _trackPolicy.value.effectiveTrack,
                    currentVersion = currentVersion,
                ),
            )
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.CHECKING,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                message = "正在检查 Android GitHub Release…",
            )
            try {
                val found = withContext(Dispatchers.IO) { findLatestUpdate() }
                candidate = found
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "check",
                        action = "result",
                        result = if (found == null) "up_to_date" else "available",
                        trigger = if (force) "manual" else "foreground",
                        track = _trackPolicy.value.effectiveTrack,
                        currentVersion = currentVersion,
                        targetVersion = found?.version,
                        durationMs = SystemClock.elapsedRealtime() - checkStarted,
                    ),
                )
                _state.value = if (found == null) {
                    AndroidUpdateUiState(
                        phase = AndroidUpdatePhase.UP_TO_DATE,
                        currentVersion = currentVersion,
                        currentVersionCode = currentVersionCode,
                        message = "当前已是最新版本。",
                    )
                } else {
                    AndroidUpdateUiState(
                        phase = AndroidUpdatePhase.AVAILABLE,
                        currentVersion = currentVersion,
                        currentVersionCode = currentVersionCode,
                        availableVersion = found.version,
                        availableVersionCode = found.versionCode,
                        message = found.notes?.takeIf { it.isNotBlank() },
                    )
                }
            } catch (error: Exception) {
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "check",
                        action = "result",
                        result = "error",
                        trigger = if (force) "manual" else "foreground",
                        track = _trackPolicy.value.effectiveTrack,
                        currentVersion = currentVersion,
                        durationMs = SystemClock.elapsedRealtime() - checkStarted,
                        errorClass = error.javaClass.simpleName,
                    ),
                )
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.ERROR,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    message = "检查更新失败：${error.message ?: error.javaClass.simpleName}",
                )
            }
        }
    }

    fun downloadAndInstall() {
        if (!updatesEnabled || downloadJob?.isActive == true) return
        val staged = pendingApk
        if (staged != null && staged.exists()) {
            requestPackageInstall(staged)
            return
        }
        val release = candidate
        if (release == null) {
            checkForUpdates(force = true)
            return
        }

        downloadJob = viewModelScope.launch {
            val stageStarted = SystemClock.elapsedRealtime()
            try {
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.DOWNLOADING,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = release.version,
                    availableVersionCode = release.versionCode,
                    progressPercent = 0,
                    message = "正在从 GitHub 下载已签名 APK…",
                )
                val apk = withContext(Dispatchers.IO) { downloadAndVerify(release) }
                pendingApk = apk
                applyStore.write(markerFor(release, AndroidUpdateApplyPhase.STAGED))
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "stage",
                        action = "verified",
                        result = "success",
                        trigger = "manual",
                        track = _trackPolicy.value.effectiveTrack,
                        currentVersion = currentVersion,
                        targetVersion = release.version,
                        durationMs = SystemClock.elapsedRealtime() - stageStarted,
                    ),
                )
                requestPackageInstall(apk)
            } catch (error: Exception) {
                updaterTelemetry.record(
                    AndroidUpdaterTelemetryEvent(
                        category = "stage",
                        action = "verified",
                        result = "error",
                        trigger = "manual",
                        track = _trackPolicy.value.effectiveTrack,
                        currentVersion = currentVersion,
                        targetVersion = release.version,
                        durationMs = SystemClock.elapsedRealtime() - stageStarted,
                        errorClass = error.javaClass.simpleName,
                    ),
                )
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.ERROR,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = release.version,
                    availableVersionCode = release.versionCode,
                    message = "更新失败：${error.message ?: error.javaClass.simpleName}",
                )
            }
        }
    }

    private fun findLatestUpdate(): AndroidUpdateCandidate? {
        val track = refreshUpdateTrackPolicy()
        require(track.effectiveTrack == "stable") {
            "Android signed package transport does not publish the ${track.effectiveTrack} track yet."
        }
        val releases = JSONArray(httpGetText(RELEASES_API, MAX_METADATA_BYTES))
        var best: AndroidUpdateCandidate? = null
        for (index in 0 until releases.length()) {
            val release = releases.optJSONObject(index) ?: continue
            if (release.optBoolean("draft") || release.optBoolean("prerelease")) continue
            val tag = release.optString("tag_name")
            if (!tag.startsWith("android-v")) continue
            val assets = release.optJSONArray("assets") ?: continue
            val manifestAsset = assetNamed(assets, UPDATE_MANIFEST_NAME) ?: continue
            val manifestUrl = trustedReleaseDownloadUrl(manifestAsset.optString("browser_download_url"))
            val manifest = JSONObject(httpGetText(manifestUrl, MAX_MANIFEST_BYTES))
            if (manifest.optInt("schemaVersion") != 1 || manifest.optString("repository") != REPOSITORY) continue
            if (manifest.optString("tag") != tag) continue

            val version = manifest.optString("version").trim()
            val versionCode = manifest.optLong("versionCode", -1L)
            val apkName = manifest.optString("apk").trim()
            val sha256 = manifest.optString("sha256").trim().lowercase()
            val manifestSize = manifest.optLong("size", -1L)
            if (version.isEmpty() || versionCode <= 0L || !APK_NAME.matches(apkName) || !SHA256.matches(sha256)) continue
            if (!AndroidVersion.isNewer(version, versionCode, currentVersion, currentVersionCode)) continue

            val apkAsset = assetNamed(assets, apkName) ?: continue
            val apkUrl = trustedReleaseDownloadUrl(apkAsset.optString("browser_download_url"))
            val assetSize = apkAsset.optLong("size", -1L)
            val size = if (manifestSize > 0L) manifestSize else assetSize
            if (size <= 0L || size > MAX_APK_BYTES || (assetSize > 0L && assetSize != size)) continue

            val candidate = AndroidUpdateCandidate(
                version = version,
                versionCode = versionCode,
                tag = tag,
                apkName = apkName,
                apkUrl = apkUrl,
                sha256 = sha256,
                size = size,
                notes = release.optString("body").trim().take(1200).ifEmpty { null },
            )
            if (best == null || AndroidVersion.isNewer(
                    candidate.version,
                    candidate.versionCode,
                    best.version,
                    best.versionCode,
                )
            ) {
                best = candidate
            }
        }
        return best
    }

    private fun assetNamed(assets: JSONArray, name: String): JSONObject? {
        for (index in 0 until assets.length()) {
            val asset = assets.optJSONObject(index) ?: continue
            if (asset.optString("name") == name) return asset
        }
        return null
    }

    private fun trustedReleaseDownloadUrl(value: String): String {
        val url = URL(value)
        require(url.protocol == "https" && url.host == "github.com") { "Untrusted GitHub release URL." }
        require(url.path.startsWith("/bhrumom/fabushi/releases/download/")) { "Unexpected GitHub release path." }
        return url.toString()
    }

    private fun httpGetText(urlValue: String, maxBytes: Int): String {
        val connection = openConnection(urlValue, "application/vnd.github+json, application/json")
        try {
            val status = connection.responseCode
            require(status in 200..299) { "HTTP $status while reading update metadata." }
            val declared = connection.contentLengthLong
            require(declared < 0L || declared <= maxBytes.toLong()) { "Update metadata is too large." }
            val output = ByteArrayOutputStream()
            connection.inputStream.use { input ->
                val buffer = ByteArray(16 * 1024)
                var total = 0
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    total += count
                    require(total <= maxBytes) { "Update metadata exceeded its size limit." }
                    output.write(buffer, 0, count)
                }
            }
            return output.toString(Charsets.UTF_8.name())
        } finally {
            connection.disconnect()
        }
    }

    private fun downloadAndVerify(release: AndroidUpdateCandidate): File {
        val directory = File(applicationContext.cacheDir, "updates").apply { mkdirs() }
        require(directory.isDirectory) { "Unable to create update cache directory." }
        val target = File(directory, release.apkName)
        if (target.exists() && target.length() == release.size && sha256File(target) == release.sha256) {
            verifyDownloadedPackage(target, release)
            return target
        }
        target.delete()
        val temporary = File(directory, "${release.apkName}.download")
        temporary.delete()

        val connection = openConnection(release.apkUrl, "application/vnd.android.package-archive, application/octet-stream")
        try {
            val status = connection.responseCode
            require(status in 200..299) { "HTTP $status while downloading the APK." }
            val declared = connection.contentLengthLong
            require(declared < 0L || declared == release.size) { "APK size changed before download." }
            require(release.size <= MAX_APK_BYTES) { "APK exceeds the download size limit." }

            val digest = MessageDigest.getInstance("SHA-256")
            var copied = 0L
            var lastPercent = -1
            temporary.outputStream().buffered().use { output ->
                connection.inputStream.buffered().use { input ->
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        copied += count
                        require(copied <= MAX_APK_BYTES && copied <= release.size) { "APK download exceeded its declared size." }
                        digest.update(buffer, 0, count)
                        output.write(buffer, 0, count)
                        val percent = ((copied * 100L) / release.size).toInt().coerceIn(0, 100)
                        if (percent != lastPercent) {
                            lastPercent = percent
                            _state.value = _state.value.copy(progressPercent = percent)
                        }
                    }
                }
            }
            require(copied == release.size) { "APK download was incomplete." }
            val actualSha256 = digest.digest().toHex()
            require(actualSha256 == release.sha256) { "APK checksum verification failed." }
            if (!temporary.renameTo(target)) {
                temporary.copyTo(target, overwrite = true)
                temporary.delete()
            }
            verifyDownloadedPackage(target, release)
            return target
        } catch (error: Exception) {
            temporary.delete()
            throw error
        } finally {
            connection.disconnect()
        }
    }

    private fun openConnection(urlValue: String, accept: String): HttpURLConnection {
        return (URL(urlValue).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 15_000
            readTimeout = 30_000
            requestMethod = "GET"
            setRequestProperty("Accept", accept)
            setRequestProperty("User-Agent", "Fabushi-Android/${BuildConfig.VERSION_NAME}")
        }
    }

    private fun sha256File(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().buffered().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        }
        return digest.digest().toHex()
    }

    @Suppress("DEPRECATION")
    private fun verifyDownloadedPackage(file: File, release: AndroidUpdateCandidate) {
        val packageManager = applicationContext.packageManager
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            PackageManager.GET_SIGNING_CERTIFICATES
        } else {
            PackageManager.GET_SIGNATURES
        }
        val archive = packageManager.getPackageArchiveInfo(file.absolutePath, flags)
            ?: error("Downloaded APK is not a readable Android package.")
        require(archive.packageName == applicationContext.packageName) { "Downloaded APK has the wrong package name." }
        require(archive.versionName == release.version) { "Downloaded APK versionName does not match the release manifest." }
        require(packageVersionCode(archive) == release.versionCode) { "Downloaded APK versionCode does not match the release manifest." }

        val installed = packageManager.getPackageInfo(applicationContext.packageName, flags)
        val installedSigners = signerDigests(installed)
        val archiveSigners = signerDigests(archive)
        require(installedSigners.isNotEmpty() && archiveSigners == installedSigners) {
            "Downloaded APK signing identity does not match the installed app."
        }
    }

    @Suppress("DEPRECATION")
    private fun packageVersionCode(info: PackageInfo): Long {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) info.longVersionCode else info.versionCode.toLong()
    }

    @Suppress("DEPRECATION")
    private fun signerDigests(info: PackageInfo): Set<String> {
        val signatures = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            info.signingInfo?.apkContentsSigners.orEmpty()
        } else {
            info.signatures.orEmpty()
        }
        return signatures.map { signature ->
            MessageDigest.getInstance("SHA-256").digest(signature.toByteArray()).toHex()
        }.toSet()
    }

    private fun canInstallPackages(): Boolean {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.O || applicationContext.packageManager.canRequestPackageInstalls()
    }

    private fun requestPackageInstall(apk: File) {
        val release = candidate ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !canInstallPackages()) {
            awaitingInstallPermission = true
            updaterTelemetry.record(
                AndroidUpdaterTelemetryEvent(
                    category = "apply",
                    action = "permission",
                    result = "required",
                    trigger = "manual",
                    track = _trackPolicy.value.effectiveTrack,
                    currentVersion = currentVersion,
                    targetVersion = release.version,
                ),
            )
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.WAITING_FOR_PERMISSION,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                availableVersion = release.version,
                availableVersionCode = release.versionCode,
                message = "安装包已下载并验证。请允许全球法布施安装未知应用，返回后会自动继续。",
            )
            try {
                applicationContext.startActivity(
                    Intent(
                        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:${applicationContext.packageName}"),
                    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            } catch (error: Exception) {
                _state.value = _state.value.copy(
                    phase = AndroidUpdatePhase.ERROR,
                    message = "无法打开安装权限设置：${error.message ?: error.javaClass.simpleName}",
                )
            }
            return
        }

        awaitingInstallPermission = false
        val uri = FileProvider.getUriForFile(
            applicationContext,
            "${applicationContext.packageName}.media",
            apk,
        )
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        if (!setHostUpgradeQuiescing(true)) {
            updaterTelemetry.record(
                AndroidUpdaterTelemetryEvent(
                    category = "apply",
                    action = "host_quiesce",
                    result = "failed",
                    trigger = "manual",
                    track = _trackPolicy.value.effectiveTrack,
                    currentVersion = currentVersion,
                    targetVersion = release.version,
                ),
            )
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.ERROR,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                availableVersion = release.version,
                availableVersionCode = release.versionCode,
                message = "更新前无法安全暂停 Host 新任务；安装器未启动，可稍后重试。",
            )
            return
        }

        val requested = markerFor(release, AndroidUpdateApplyPhase.REQUESTED)
        applyStore.write(requested)
        updaterTelemetry.record(
            AndroidUpdaterTelemetryEvent(
                category = "apply",
                action = "installer_request",
                result = "requested",
                trigger = "manual",
                track = _trackPolicy.value.effectiveTrack,
                currentVersion = currentVersion,
                targetVersion = release.version,
            ),
        )
        try {
            applicationContext.startActivity(intent)
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.INSTALLING,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                availableVersion = release.version,
                availableVersionCode = release.versionCode,
                message = "安装包已验证，系统安装器已打开。确认安装后 Android 会完成升级。",
            )
        } catch (error: Exception) {
            setHostUpgradeQuiescing(false)
            updaterTelemetry.record(
                AndroidUpdaterTelemetryEvent(
                    category = "apply",
                    action = "installer_request",
                    result = "spawn_failed",
                    trigger = "manual",
                    track = _trackPolicy.value.effectiveTrack,
                    currentVersion = currentVersion,
                    targetVersion = release.version,
                    errorClass = error.javaClass.simpleName,
                ),
            )
            applyStore.write(
                requested.copy(
                    phase = AndroidUpdateApplyPhase.SPAWN_FAILED,
                    recordedAtMs = System.currentTimeMillis(),
                ),
            )
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.ERROR,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                availableVersion = release.version,
                availableVersionCode = release.versionCode,
                message = "无法打开系统安装器：${error.message ?: error.javaClass.simpleName}",
            )
        }
    }

    private fun setHostUpgradeQuiescing(quiescing: Boolean): Boolean =
        runCatching { coordinator.setUpgradeQuiescing(quiescing) == quiescing }
            .getOrDefault(false)

    private fun markerFor(
        release: AndroidUpdateCandidate,
        phase: AndroidUpdateApplyPhase,
    ): AndroidUpdateApplyMarker =
        AndroidUpdateApplyMarker(
            phase = phase,
            targetVersion = release.version,
            targetVersionCode = release.versionCode,
            fromVersion = currentVersion,
            fromVersionCode = currentVersionCode,
            apkName = release.apkName,
            sha256 = release.sha256,
            size = release.size,
            recordedAtMs = System.currentTimeMillis(),
        )

    private fun candidateFrom(marker: AndroidUpdateApplyMarker): AndroidUpdateCandidate =
        AndroidUpdateCandidate(
            version = marker.targetVersion,
            versionCode = marker.targetVersionCode,
            tag = "recovered-apply",
            apkName = marker.apkName,
            apkUrl = "",
            sha256 = marker.sha256,
            size = marker.size,
            notes = null,
        )

    private fun recoverVerifiedApk(marker: AndroidUpdateApplyMarker): Boolean {
        val file = File(File(applicationContext.cacheDir, "updates"), marker.apkName)
        if (!file.isFile || file.length() != marker.size || sha256File(file) != marker.sha256) return false
        val recovered = candidateFrom(marker)
        if (runCatching { verifyDownloadedPackage(file, recovered) }.isFailure) return false
        candidate = recovered
        pendingApk = file
        return true
    }

    private fun restorePendingApply() {
        val marker = applyStore.read() ?: return
        val decision = AndroidUpdateRecoveryPolicy.decide(
            marker = marker,
            currentVersion = currentVersion,
            currentVersionCode = currentVersionCode,
            nowMs = System.currentTimeMillis(),
        )
        updaterTelemetry.record(
            AndroidUpdaterTelemetryEvent(
                category = "recovery",
                action = "apply_marker",
                result = decision.name.lowercase(),
                currentVersion = currentVersion,
                targetVersion = marker.targetVersion,
                phase = marker.phase.name.lowercase(),
            ),
        )
        when (decision) {
            AndroidUpdateRecoveryDecision.CONFIRMED -> {
                applyStore.clear()
                _state.value = AndroidUpdateUiState(
                    phase = if (updatesEnabled) AndroidUpdatePhase.UP_TO_DATE else AndroidUpdatePhase.DISABLED,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    message = if (updatesEnabled) "上次更新已确认安装成功。" else null,
                )
            }
            AndroidUpdateRecoveryDecision.RECOVER_STAGED -> {
                if (recoverVerifiedApk(marker)) {
                    _state.value = AndroidUpdateUiState(
                        phase = AndroidUpdatePhase.AVAILABLE,
                        currentVersion = currentVersion,
                        currentVersionCode = currentVersionCode,
                        availableVersion = marker.targetVersion,
                        availableVersionCode = marker.targetVersionCode,
                        message = "已恢复并重新验证上次下载的安装包；需要你显式继续安装。",
                    )
                } else {
                    applyStore.clear()
                    _state.value = AndroidUpdateUiState(
                        phase = AndroidUpdatePhase.ERROR,
                        currentVersion = currentVersion,
                        currentVersionCode = currentVersionCode,
                        message = "上次已暂存的安装包已丢失或验证失败；请重新检查更新。",
                    )
                }
            }
            AndroidUpdateRecoveryDecision.OUTCOME_UNKNOWN -> {
                recoverVerifiedApk(marker)
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.OUTCOME_UNKNOWN,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = marker.targetVersion,
                    availableVersionCode = marker.targetVersionCode,
                    message = "上次安装请求的结果无法证明。Fabushi 不会自动重放；请显式重新验证并继续。",
                )
            }
            AndroidUpdateRecoveryDecision.RETRYABLE_FAILURE -> {
                val recovered = recoverVerifiedApk(marker)
                if (!recovered) applyStore.clear()
                _state.value = AndroidUpdateUiState(
                    phase = if (recovered) AndroidUpdatePhase.AVAILABLE else AndroidUpdatePhase.ERROR,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    availableVersion = marker.targetVersion,
                    availableVersionCode = marker.targetVersionCode,
                    message = if (recovered) {
                        "上次无法打开系统安装器；已重新验证安装包，可显式重试。"
                    } else {
                        "上次无法打开系统安装器，且暂存包已不可用；请重新检查更新。"
                    },
                )
            }
            AndroidUpdateRecoveryDecision.ORPHANED -> {
                applyStore.clear()
                _state.value = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.ERROR,
                    currentVersion = currentVersion,
                    currentVersionCode = currentVersionCode,
                    message = "检测到过期或不属于当前安装版本的更新记录，已安全丢弃；请重新检查更新。",
                )
            }
        }
    }

    private fun ByteArray.toHex(): String = joinToString("") { byte -> "%02x".format(byte) }
}
