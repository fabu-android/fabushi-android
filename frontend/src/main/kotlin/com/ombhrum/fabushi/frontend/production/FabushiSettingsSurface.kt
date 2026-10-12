package com.ombhrum.fabushi

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.ombhrum.fabushi.androidpreload.runtime.AndroidUpdateTrackPolicy
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private enum class FabushiSettingsSection {
    GENERAL,
    UPDATES,
}

internal const val ACCOUNT_AVATAR_TIMEOUT_MS = 10_000
internal const val ACCOUNT_AVATAR_MAX_BYTES = 1024 * 1024
private const val ACCOUNT_AVATAR_MAX_EDGE_PX = 4096
private const val ACCOUNT_AVATAR_MAX_PIXELS = 16_000_000L

private fun normalizedHttpsAccountAvatarUrl(raw: String?): String? {
    val value = raw?.trim().orEmpty()
    if (value.isEmpty()) return null
    val uri = runCatching { URI(value) }.getOrNull() ?: return null
    if (!uri.scheme.equals("https", ignoreCase = true) || uri.host.isNullOrBlank() || uri.userInfo != null) {
        return null
    }
    return uri.toString()
}

internal fun githubAccountAvatarUrl(authId: String?): String? {
    val raw = authId?.trim().orEmpty()
    val separator = raw.indexOf('|')
    if (separator <= 0 || separator >= raw.lastIndex) return null
    if (raw.substring(0, separator) != "github") return null
    val subject = raw.substring(separator + 1)
    if (subject.isEmpty() || subject.any { !it.isDigit() }) return null
    return "https://avatars.githubusercontent.com/u/$subject?v=4&s=192"
}

internal fun accountAvatarCandidateUrls(
    authId: String?,
    preferredUrl: String?,
): List<String> = buildList {
    normalizedHttpsAccountAvatarUrl(preferredUrl)?.let(::add)
    githubAccountAvatarUrl(authId)?.takeIf { it !in this }?.let(::add)
}

private fun decodeBoundedAccountAvatar(bytes: ByteArray): Bitmap? {
    if (bytes.isEmpty() || bytes.size > ACCOUNT_AVATAR_MAX_BYTES) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    val width = bounds.outWidth
    val height = bounds.outHeight
    if (
        width <= 0 ||
        height <= 0 ||
        width > ACCOUNT_AVATAR_MAX_EDGE_PX ||
        height > ACCOUNT_AVATAR_MAX_EDGE_PX ||
        width.toLong() * height.toLong() > ACCOUNT_AVATAR_MAX_PIXELS
    ) {
        return null
    }
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
}

private fun fetchBoundedAccountAvatar(rawUrl: String): Bitmap? {
    val connection = (URL(rawUrl).openConnection() as? HttpURLConnection) ?: return null
    connection.instanceFollowRedirects = false
    connection.connectTimeout = ACCOUNT_AVATAR_TIMEOUT_MS
    connection.readTimeout = ACCOUNT_AVATAR_TIMEOUT_MS
    connection.requestMethod = "GET"
    connection.setRequestProperty("Accept", "image/*")
    return try {
        connection.connect()
        if (connection.responseCode !in 200..299) return null
        val contentType = connection.contentType?.substringBefore(';')?.trim().orEmpty()
        if (!contentType.startsWith("image/")) return null
        val declaredLength = connection.contentLengthLong
        if (declaredLength > ACCOUNT_AVATAR_MAX_BYTES) return null
        val bytes = connection.inputStream.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                if (output.size() + count > ACCOUNT_AVATAR_MAX_BYTES) return null
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        }
        decodeBoundedAccountAvatar(bytes)
    } catch (_: Exception) {
        null
    } finally {
        connection.disconnect()
    }
}

private suspend fun loadAccountAvatar(
    authId: String?,
    preferredUrl: String?,
): Bitmap? = withContext(Dispatchers.IO) {
    accountAvatarCandidateUrls(authId, preferredUrl)
        .firstNotNullOfOrNull(::fetchBoundedAccountAvatar)
}

@Composable
internal fun AccountAvatar(
    authId: String,
    preferredUrl: String,
    accountName: String,
    size: Dp = 48.dp,
    testTagPrefix: String = "settings-account-avatar",
) {
    val bitmap by produceState<Bitmap?>(
        initialValue = null,
        key1 = authId,
        key2 = preferredUrl,
    ) {
        value = loadAccountAvatar(authId, preferredUrl)
    }
    val resolved = bitmap
    if (resolved != null) {
        Image(
            bitmap = resolved.asImageBitmap(),
            contentDescription = "Account avatar",
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .size(size)
                .clip(CircleShape)
                .testTag(testTagPrefix),
        )
    } else {
        Box(
            modifier = Modifier
                .size(size)
                .background(MaterialTheme.colorScheme.surfaceVariant, CircleShape)
                .testTag("$testTagPrefix-fallback"),
            contentAlignment = Alignment.Center,
        ) {
            Text(accountName.trim().firstOrNull()?.uppercaseChar()?.toString() ?: "F")
        }
    }
}

internal fun accountPrivacyModeLabel(mode: String?): String = when (mode?.trim()) {
    "no-storage" -> "No storage"
    "no-training" -> "No training"
    "usage-data-training-allowed" -> "Usage data training allowed"
    "usage-codebase-training-allowed" -> "Codebase training allowed"
    "unspecified" -> "Unspecified"
    else -> "Unavailable"
}

@Composable
internal fun FabushiSettingsSurface(
    accountLoggedIn: Boolean,
    accountName: String,
    accountEmail: String,
    accountAuthId: String,
    accountAvatarUrl: String,
    accountProfileBusy: Boolean,
    accountProfileError: String?,
    accountPrivacyMode: String?,
    onUpdateAccountDisplayName: (String) -> Unit,
    updateState: AndroidUpdateUiState,
    updateTrackPolicy: AndroidUpdateTrackPolicy,
    autoUpdateWhenIdleOptIn: Boolean,
    autoUpdateWhenIdleGateEnabled: Boolean,
    onSetAutoUpdateWhenIdleOptIn: (Boolean) -> Unit,
    onRefreshAccountPrivacyMode: () -> Unit,
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
    onClose: () -> Unit,
) {
    var section by remember { mutableStateOf(FabushiSettingsSection.GENERAL) }
    var accountNameDraft by remember(accountName) { mutableStateOf(accountName) }
    val theme = LocalFabushiThemeController.current
    LaunchedEffect(accountLoggedIn) {
        if (accountLoggedIn) onRefreshAccountPrivacyMode()
    }
    AlertDialog(
        onDismissRequest = onClose,
        modifier = Modifier.testTag("fabushi-settings-dialog"),
        title = { Text("Fabushi settings") },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { section = FabushiSettingsSection.GENERAL }) {
                        Text(if (section == FabushiSettingsSection.GENERAL) "✓ General" else "General")
                    }
                    TextButton(onClick = { section = FabushiSettingsSection.UPDATES }) {
                        Text(if (section == FabushiSettingsSection.UPDATES) "✓ Updates" else "Updates")
                    }
                }
                when (section) {
                    FabushiSettingsSection.GENERAL -> {
                        if (accountLoggedIn) {
                            Text("Account")
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                modifier = Modifier.testTag("settings-account-profile"),
                            ) {
                                AccountAvatar(
                                    authId = accountAuthId,
                                    preferredUrl = accountAvatarUrl,
                                    accountName = accountName,
                                )
                                Column {
                                    Text(accountName.ifBlank { "Fabushi" })
                                    accountEmail.takeIf(String::isNotBlank)?.let { Text(it) }
                                }
                            }
                            Text(
                                "Privacy mode: ${accountPrivacyModeLabel(accountPrivacyMode)}",
                                modifier = Modifier.testTag("settings-account-privacy-mode"),
                            )
                            OutlinedTextField(
                                value = accountNameDraft,
                                onValueChange = { value -> accountNameDraft = value.take(200) },
                                enabled = !accountProfileBusy,
                                singleLine = true,
                                label = { Text("Display name") },
                                modifier = Modifier.testTag("settings-account-name"),
                            )
                            TextButton(
                                onClick = { onUpdateAccountDisplayName(accountNameDraft) },
                                enabled = !accountProfileBusy &&
                                    accountNameDraft.trim().isNotEmpty() &&
                                    accountNameDraft.trim() != accountName.trim(),
                                modifier = Modifier.testTag("settings-account-name-save"),
                            ) {
                                Text(if (accountProfileBusy) "Saving…" else "Save name")
                            }
                            accountProfileError?.takeIf(String::isNotBlank)?.let { error ->
                                Text(
                                    error,
                                    color = MaterialTheme.colorScheme.error,
                                    modifier = Modifier.testTag("settings-account-profile-error"),
                                )
                            }
                            Text(
                                "Account identity, profile mutation and privacy policy remain owned by canonical Host services.",
                            )
                        }
                        Text("Appearance")
                        Text("Theme")
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            for (preference in FabushiThemePreference.entries) {
                                val label = when (preference) {
                                    FabushiThemePreference.SYSTEM -> "System"
                                    FabushiThemePreference.LIGHT -> "Light"
                                    FabushiThemePreference.DARK -> "Dark"
                                }
                                TextButton(
                                    onClick = { theme.setPreference(preference) },
                                    modifier = Modifier.testTag("settings-theme-" + preference.wireValue),
                                ) {
                                    Text(if (theme.preference == preference) "✓ $label" else label)
                                }
                            }
                        }
                        Text(
                            "Theme preference is stored on this Android device. " +
                                "Account, Agent and runtime truth remain owned by their canonical services.",
                        )
                    }
                    FabushiSettingsSection.UPDATES -> {
                        Text("App updates")
                        Text("Current version: ${updateState.currentVersion} (${updateState.currentVersionCode})")
                        Text("Release channel: ${updateTrackPolicy.effectiveTrack}")
                        Text(
                            "Selectable by current Rust policy: " +
                                updateTrackPolicy.selectableTracks.joinToString().ifBlank { "none" },
                        )
                        updateState.availableVersion?.let {
                            Text("Available version: $it (${updateState.availableVersionCode ?: "unknown"})")
                        }
                        updateState.message?.takeIf(String::isNotBlank)?.let { Text(it) }
                        if (updateState.phase == AndroidUpdatePhase.DOWNLOADING) {
                            val progress = (updateState.progressPercent ?: 0).coerceIn(0, 100) / 100f
                            LinearProgressIndicator(progress = { progress })
                        }
                        Text(
                            "Auto-stage when idle: " +
                                if (autoUpdateWhenIdleOptIn) "On" else "Off",
                        )
                        Text(
                            "Remote safety gate: " +
                                if (autoUpdateWhenIdleGateEnabled) "Enabled" else "Unavailable or disabled",
                        )
                        TextButton(
                            onClick = { onSetAutoUpdateWhenIdleOptIn(!autoUpdateWhenIdleOptIn) },
                            modifier = Modifier.testTag("settings-update-auto-idle"),
                        ) {
                            Text(if (autoUpdateWhenIdleOptIn) "Turn off idle auto-stage" else "Turn on idle auto-stage")
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(
                                onClick = onCheckUpdate,
                                enabled = updateState.phase != AndroidUpdatePhase.CHECKING &&
                                    updateState.phase != AndroidUpdatePhase.DOWNLOADING,
                                modifier = Modifier.testTag("settings-update-check"),
                            ) {
                                Text("Check for updates")
                            }
                            if (
                                updateState.phase == AndroidUpdatePhase.AVAILABLE ||
                                updateState.phase == AndroidUpdatePhase.WAITING_FOR_PERMISSION ||
                                updateState.phase == AndroidUpdatePhase.INSTALLING ||
                                updateState.phase == AndroidUpdatePhase.OUTCOME_UNKNOWN
                            ) {
                                TextButton(
                                    onClick = onInstallUpdate,
                                    modifier = Modifier.testTag("settings-update-install"),
                                ) {
                                    Text(
                                        when (updateState.phase) {
                                            AndroidUpdatePhase.AVAILABLE -> "Download and install"
                                            AndroidUpdatePhase.OUTCOME_UNKNOWN -> "Verify and retry install"
                                            else -> "Continue install"
                                        },
                                    )
                                }
                            }
                        }
                        Text(
                            "The effective channel is resolved by the canonical Rust update policy. " +
                                "Android package distribution currently publishes signed stable artifacts only; " +
                                "non-stable delivery and auto-update-when-idle remain unavailable until their " +
                                "platform transport and safety gates are implemented.",
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("fabushi-settings-close"),
            ) {
                Text("Close")
            }
        },
    )
}
