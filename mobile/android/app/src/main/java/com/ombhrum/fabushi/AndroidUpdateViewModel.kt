package com.ombhrum.fabushi

import android.app.Application
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
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
    private data class Parsed(val core: List<Long>, val preRelease: List<String>)

    fun compare(left: String, right: String): Int {
        val leftParsed = parse(left)
        val rightParsed = parse(right)
        val count = maxOf(leftParsed.core.size, rightParsed.core.size)
        repeat(count) { index ->
            val lhs = leftParsed.core.getOrElse(index) { 0L }
            val rhs = rightParsed.core.getOrElse(index) { 0L }
            if (lhs != rhs) return lhs.compareTo(rhs)
        }

        if (leftParsed.preRelease.isEmpty() && rightParsed.preRelease.isNotEmpty()) return 1
        if (leftParsed.preRelease.isNotEmpty() && rightParsed.preRelease.isEmpty()) return -1
        val preCount = maxOf(leftParsed.preRelease.size, rightParsed.preRelease.size)
        repeat(preCount) { index ->
            val lhs = leftParsed.preRelease.getOrNull(index) ?: return -1
            val rhs = rightParsed.preRelease.getOrNull(index) ?: return 1
            val lhsNumber = lhs.toLongOrNull()
            val rhsNumber = rhs.toLongOrNull()
            val result = when {
                lhsNumber != null && rhsNumber != null -> lhsNumber.compareTo(rhsNumber)
                lhsNumber != null -> -1
                rhsNumber != null -> 1
                else -> lhs.compareTo(rhs, ignoreCase = true)
            }
            if (result != 0) return result
        }
        return 0
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

    private fun parse(value: String): Parsed {
        val normalized = value
            .trim()
            .removePrefix("android-v")
            .removePrefix("v")
            .substringBefore('+')
        val pieces = normalized.split('-', limit = 2)
        val core = pieces.firstOrNull().orEmpty().split('.')
            .filter { it.isNotBlank() }
            .map { it.toLongOrNull() ?: 0L }
            .ifEmpty { listOf(0L) }
        val preRelease = pieces.getOrNull(1)?.split('.')?.filter { it.isNotBlank() }.orEmpty()
        return Parsed(core, preRelease)
    }
}

internal object AndroidUpdateValidator {
    const val CANONICAL_REPOSITORY = "fabu-android/fabushi-android"
    val TRUSTED_REPOSITORIES: Set<String> = setOf(
        "fabu-android/fabushi-android",
        "bhrumom/fabushi-android",
        "bhrumom/fabushi",
    )
    private val APK_NAME_REGEX = Regex("^[A-Za-z0-9._-]+[.]apk$")
    private val SHA256_REGEX = Regex("^[a-f0-9]{64}$")

    fun isTrustedRepository(repository: String): Boolean = repository.trim() in TRUSTED_REPOSITORIES

    fun isSupportedReleaseTag(tag: String): Boolean {
        val trimmed = tag.trim()
        return (trimmed.startsWith("v") && trimmed.length > 1 && trimmed[1].isDigit()) ||
            (trimmed.startsWith("android-v") && trimmed.length > 9 && trimmed[9].isDigit())
    }

    fun isValidApkFileName(name: String): Boolean = APK_NAME_REGEX.matches(name.trim())

    fun isValidSha256Hex(sha256: String): Boolean = SHA256_REGEX.matches(sha256.trim().lowercase())

    fun trustedReleaseDownloadUrl(value: String): String {
        val url = URL(value)
        require(url.protocol == "https" && url.host == "github.com") { "Untrusted GitHub release URL." }
        val matchedRepo = TRUSTED_REPOSITORIES.any { repo ->
            url.path.startsWith("/$repo/releases/download/")
        }
        require(matchedRepo) { "Unexpected GitHub release path." }
        return url.toString()
    }
}

class AndroidUpdateViewModel(application: Application) : AndroidViewModel(application) {
    companion object {
        internal const val REPOSITORY = AndroidUpdateValidator.CANONICAL_REPOSITORY
        internal const val RELEASES_API = "https://api.github.com/repos/fabu-android/fabushi-android/releases?per_page=100"
        private const val UPDATE_MANIFEST_NAME = "fabushi-android-update.json"
        private const val UPDATE_CHECK_MIN_INTERVAL_MS = 60_000L
        private const val UPDATE_FOREGROUND_INTERVAL_MS = 5 * 60_000L
        private const val STARTUP_DELAY_MS = 4_000L
        private const val MAX_METADATA_BYTES = 2 * 1024 * 1024
        private const val MAX_MANIFEST_BYTES = 128 * 1024
        private const val MAX_APK_BYTES = 768L * 1024L * 1024L
    }

    private val applicationContext = getApplication<Application>()
    private val updatesEnabled = BuildConfig.GITHUB_UPDATES_ENABLED && !BuildConfig.DEBUG
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

    private var candidate: AndroidUpdateCandidate? = null
    private var pendingApk: File? = null
    private var awaitingInstallPermission = false
    private var checkJob: Job? = null
    private var downloadJob: Job? = null
    private var foregroundJob: Job? = null
    private var lastCheckAtMs = 0L

    fun setForeground(foreground: Boolean) {
        if (!updatesEnabled) return
        if (!foreground) {
            foregroundJob?.cancel()
            foregroundJob = null
            return
        }
        if (awaitingInstallPermission && canInstallPackages()) {
            pendingApk?.takeIf(File::exists)?.let(::requestPackageInstall)
        }
        if (foregroundJob?.isActive == true) return
        foregroundJob = viewModelScope.launch {
            if (lastCheckAtMs == 0L) delay(STARTUP_DELAY_MS)
            while (isActive) {
                checkForUpdates()
                delay(UPDATE_FOREGROUND_INTERVAL_MS)
            }
        }
    }

    fun checkForUpdates(force: Boolean = false) {
        if (!updatesEnabled || awaitingInstallPermission || pendingApk != null) return
        if (downloadJob?.isActive == true || checkJob?.isActive == true) return
        val now = System.currentTimeMillis()
        if (!force && now - lastCheckAtMs < UPDATE_CHECK_MIN_INTERVAL_MS) return
        lastCheckAtMs = now
        checkJob = viewModelScope.launch {
            _state.value = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.CHECKING,
                currentVersion = currentVersion,
                currentVersionCode = currentVersionCode,
                message = "正在检查 Android GitHub Release…",
            )
            try {
                val found = withContext(Dispatchers.IO) { findLatestUpdate() }
                candidate = found
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
                requestPackageInstall(apk)
            } catch (error: Exception) {
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
        val releases = JSONArray(httpGetText(RELEASES_API, MAX_METADATA_BYTES))
        var best: AndroidUpdateCandidate? = null
        for (index in 0 until releases.length()) {
            val release = releases.optJSONObject(index) ?: continue
            if (release.optBoolean("draft") || release.optBoolean("prerelease")) continue
            val tag = release.optString("tag_name")
            if (!AndroidUpdateValidator.isSupportedReleaseTag(tag)) continue
            val assets = release.optJSONArray("assets") ?: continue
            val manifestAsset = assetNamed(assets, UPDATE_MANIFEST_NAME) ?: continue
            val manifestUrl = trustedReleaseDownloadUrl(manifestAsset.optString("browser_download_url"))
            val manifest = JSONObject(httpGetText(manifestUrl, MAX_MANIFEST_BYTES))
            if (manifest.optInt("schemaVersion") != 1 || !AndroidUpdateValidator.isTrustedRepository(manifest.optString("repository"))) continue
            if (manifest.optString("tag") != tag) continue

            val version = manifest.optString("version").trim()
            val versionCode = manifest.optLong("versionCode", -1L)
            val apkName = manifest.optString("apk").trim()
            val sha256 = manifest.optString("sha256").trim().lowercase()
            val manifestSize = manifest.optLong("size", -1L)
            if (version.isEmpty() || versionCode <= 0L || !AndroidUpdateValidator.isValidApkFileName(apkName) || !AndroidUpdateValidator.isValidSha256Hex(sha256)) continue
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

    private fun trustedReleaseDownloadUrl(value: String): String =
        AndroidUpdateValidator.trustedReleaseDownloadUrl(value)

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

    private fun ByteArray.toHex(): String = joinToString("") { byte -> "%02x".format(byte) }
}
