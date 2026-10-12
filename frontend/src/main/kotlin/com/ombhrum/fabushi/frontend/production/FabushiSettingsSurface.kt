package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidAccountUsageSummary
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAutoReviewInstructionsSnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidTimeZoneSettings

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
import androidx.compose.material3.Button
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
    CREDENTIALS,
    UPDATES,
}

internal const val ACCOUNT_AVATAR_TIMEOUT_MS = 10_000
internal const val ACCOUNT_AVATAR_MAX_BYTES = 1024 * 1024
internal const val ACCOUNT_AVATAR_CACHE_MAX_ENTRIES = 32
private const val ACCOUNT_AVATAR_MAX_EDGE_PX = 4096
private const val ACCOUNT_AVATAR_MAX_PIXELS = 16_000_000L

internal class AccountAvatarByteCache(
    private val maxEntries: Int = ACCOUNT_AVATAR_CACHE_MAX_ENTRIES,
) {
    init {
        require(maxEntries > 0) { "account avatar cache must retain at least one entry" }
    }

    private val values = object : java.util.LinkedHashMap<String, ByteArray>(maxEntries, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ByteArray>?): Boolean =
            size > maxEntries
    }

    @Synchronized
    fun get(url: String): ByteArray? = values[url]?.copyOf()

    @Synchronized
    fun put(url: String, bytes: ByteArray) {
        require(bytes.isNotEmpty() && bytes.size <= ACCOUNT_AVATAR_MAX_BYTES) {
            "account avatar cache value is outside bounded size"
        }
        values[url] = bytes.copyOf()
    }

    @Synchronized
    fun size(): Int = values.size
}

private val accountAvatarByteCache = AccountAvatarByteCache()

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
    accountAvatarByteCache.get(rawUrl)?.let(::decodeBoundedAccountAvatar)?.let { return it }

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
        val decoded = decodeBoundedAccountAvatar(bytes) ?: return null
        accountAvatarByteCache.put(rawUrl, bytes)
        decoded
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
    accountUsageSummary: AndroidAccountUsageSummary?,
    accountUsageBusy: Boolean,
    accountUsageError: String?,
    timeZoneSettings: AndroidTimeZoneSettings?,
    timeZoneBusy: Boolean,
    timeZoneError: String?,
    onSaveTimeZoneOverride: (String?) -> Unit,
    processMetricsLocalEnabled: Boolean,
    processMetricsBusy: Boolean,
    processMetricsError: String?,
    onSetProcessMetricsLocalEnabled: (Boolean) -> Unit,
    autoReviewSnapshot: AndroidAutoReviewInstructionsSnapshot?,
    autoReviewBusy: Boolean,
    autoReviewError: String?,
    onRefreshAutoReviewInstructions: () -> Unit,
    onSaveAutoReviewInstructions: (Long, Boolean, List<String>, List<String>) -> Unit,
    credentialVaultItems: List<CredentialVaultMetadata>,
    credentialVaultBusy: Boolean,
    credentialVaultError: String?,
    onRefreshCredentialVault: () -> Unit,
    onSaveCredential: (String, String, String, List<String>, Boolean) -> Unit,
    onRevokeCredential: (String) -> Unit,
    onUpdateAccountDisplayName: (String) -> Unit,
    updateState: AndroidUpdateUiState,
    updateTrackPolicy: AndroidUpdateTrackPolicy,
    autoUpdateWhenIdleOptIn: Boolean,
    autoUpdateWhenIdleGateEnabled: Boolean,
    onSetAutoUpdateWhenIdleOptIn: (Boolean) -> Unit,
    onRefreshAccountPrivacyMode: () -> Unit,
    onRefreshAccountUsage: () -> Unit,
    onCancelSandTrial: () -> Unit,
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
    onOpenFeedback: () -> Unit,
    onClose: () -> Unit,
) {
    var section by remember { mutableStateOf(FabushiSettingsSection.GENERAL) }
    var accountNameDraft by remember(accountName) { mutableStateOf(accountName) }
    var credentialNameDraft by remember { mutableStateOf("") }
    var credentialLabelDraft by remember { mutableStateOf("") }
    var credentialSecretDraft by remember { mutableStateOf("") }
    var credentialOriginsDraft by remember { mutableStateOf("https://") }
    var credentialRotate by remember { mutableStateOf(false) }
    var timeZoneDraft by remember(timeZoneSettings?.overrideTimeZone) {
        mutableStateOf(timeZoneSettings?.overrideTimeZone.orEmpty())
    }
    var autoReviewEnabledDraft by remember(
        autoReviewSnapshot?.accountFence,
        autoReviewSnapshot?.revision,
    ) {
        mutableStateOf(autoReviewSnapshot?.instructions?.isEnabled ?: true)
    }
    var autoReviewAllowDraft by remember(
        autoReviewSnapshot?.accountFence,
        autoReviewSnapshot?.revision,
    ) {
        mutableStateOf(autoReviewSnapshot?.instructions?.allowInstructions?.joinToString("\n").orEmpty())
    }
    var autoReviewBlockDraft by remember(
        autoReviewSnapshot?.accountFence,
        autoReviewSnapshot?.revision,
    ) {
        mutableStateOf(autoReviewSnapshot?.instructions?.blockInstructions?.joinToString("\n").orEmpty())
    }
    val theme = LocalFabushiThemeController.current
    LaunchedEffect(accountLoggedIn) {
        if (accountLoggedIn) {
            onRefreshAccountPrivacyMode()
            onRefreshAccountUsage()
            onRefreshCredentialVault()
        } else {
            credentialSecretDraft = ""
        }
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
                    TextButton(onClick = { section = FabushiSettingsSection.CREDENTIALS }) {
                        Text(if (section == FabushiSettingsSection.CREDENTIALS) "✓ Credentials" else "Credentials")
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
                            accountUsageSummary?.let { usage ->
                                val percent = usage.sandUsagePercent?.let { "%.1f%%".format(it) } ?: "Unavailable"
                                Text(
                                    "Usage: $percent",
                                    modifier = Modifier.testTag("settings-account-usage"),
                                )
                                usage.onDemand?.let { onDemand ->
                                    Text(
                                        "On-demand: ${onDemand.usedCents} / ${onDemand.limitCents} cents",
                                        modifier = Modifier.testTag("settings-account-on-demand"),
                                    )
                                }
                                if (usage.isSandTrial) {
                                    Text("Trial active", modifier = Modifier.testTag("settings-account-trial"))
                                } else if (usage.hasEndedSandTrial) {
                                    Text("Trial ended", modifier = Modifier.testTag("settings-account-trial"))
                                }
                                if (usage.canCancelSandTrial) {
                                    Button(
                                        onClick = onCancelSandTrial,
                                        enabled = !accountUsageBusy,
                                        modifier = Modifier.testTag("settings-account-cancel-trial"),
                                    ) { Text(if (accountUsageBusy) "Cancelling…" else "Cancel trial") }
                                }
                            }
                            accountUsageError?.takeIf(String::isNotBlank)?.let { error ->
                                Text(
                                    "Usage unavailable: $error",
                                    modifier = Modifier.testTag("settings-account-usage-error"),
                                )
                            }
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
                            Text("Auto Review")
                            Text(
                                "Canonical instructions are account-scoped and owned by the Rust Host. " +
                                    "One instruction per line; duplicates and limits are normalized by the Host.",
                                modifier = Modifier.testTag("settings-auto-review-description"),
                            )
                            if (autoReviewBusy) {
                                LinearProgressIndicator(
                                    modifier = Modifier.testTag("settings-auto-review-busy"),
                                )
                            }
                            TextButton(
                                onClick = { autoReviewEnabledDraft = !autoReviewEnabledDraft },
                                enabled = !autoReviewBusy && autoReviewSnapshot != null,
                                modifier = Modifier.testTag("settings-auto-review-enabled"),
                            ) {
                                Text(if (autoReviewEnabledDraft) "✓ Auto Review enabled" else "Auto Review disabled")
                            }
                            OutlinedTextField(
                                value = autoReviewAllowDraft,
                                onValueChange = { autoReviewAllowDraft = it },
                                enabled = !autoReviewBusy && autoReviewSnapshot != null,
                                label = { Text("Always allow") },
                                modifier = Modifier.testTag("settings-auto-review-allow"),
                            )
                            OutlinedTextField(
                                value = autoReviewBlockDraft,
                                onValueChange = { autoReviewBlockDraft = it },
                                enabled = !autoReviewBusy && autoReviewSnapshot != null,
                                label = { Text("Always block") },
                                modifier = Modifier.testTag("settings-auto-review-block"),
                            )
                            TextButton(
                                onClick = {
                                    val snapshot = autoReviewSnapshot ?: return@TextButton
                                    onSaveAutoReviewInstructions(
                                        snapshot.revision,
                                        autoReviewEnabledDraft,
                                        autoReviewAllowDraft.lines(),
                                        autoReviewBlockDraft.lines(),
                                    )
                                },
                                enabled = !autoReviewBusy && autoReviewSnapshot != null,
                                modifier = Modifier.testTag("settings-auto-review-save"),
                            ) {
                                Text(if (autoReviewBusy) "Saving Auto Review…" else "Save Auto Review")
                            }
                            autoReviewError?.takeIf(String::isNotBlank)?.let { error ->
                                Text(
                                    error,
                                    color = MaterialTheme.colorScheme.error,
                                    modifier = Modifier.testTag("settings-auto-review-error"),
                                )
                            }
                        }
                        Text("Time zone")
                        Text(
                            "System: " + (timeZoneSettings?.detectedTimeZone ?: "Unavailable") +
                                " · Effective: " + (timeZoneSettings?.effectiveTimeZone ?: "Unavailable"),
                            modifier = Modifier.testTag("settings-time-zone-summary"),
                        )
                        OutlinedTextField(
                            value = timeZoneDraft,
                            onValueChange = { timeZoneDraft = it.take(128) },
                            enabled = !timeZoneBusy && timeZoneSettings != null,
                            singleLine = true,
                            label = { Text("Override (IANA time zone)") },
                            placeholder = { Text("America/Los_Angeles") },
                            modifier = Modifier.testTag("settings-time-zone-override"),
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(
                                onClick = {
                                    onSaveTimeZoneOverride(timeZoneDraft.trim().takeIf(String::isNotEmpty))
                                },
                                enabled = !timeZoneBusy && timeZoneSettings != null,
                                modifier = Modifier.testTag("settings-time-zone-save"),
                            ) {
                                Text(if (timeZoneBusy) "Saving…" else "Save time zone")
                            }
                            TextButton(
                                onClick = {
                                    timeZoneDraft = ""
                                    onSaveTimeZoneOverride(null)
                                },
                                enabled = !timeZoneBusy &&
                                    timeZoneSettings?.overrideTimeZone != null,
                                modifier = Modifier.testTag("settings-time-zone-system"),
                            ) {
                                Text("Use system")
                            }
                        }
                        timeZoneError?.takeIf(String::isNotBlank)?.let { error ->
                            Text(
                                error,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.testTag("settings-time-zone-error"),
                            )
                        }
                        Text("Performance diagnostics")
                        Text(
                            "Local process metrics are off by default. When enabled, Fabushi samples only " +
                                "this app's CPU time and memory on this device. Backend reporting remains disabled.",
                            modifier = Modifier.testTag("settings-process-metrics-description"),
                        )
                        TextButton(
                            onClick = { onSetProcessMetricsLocalEnabled(!processMetricsLocalEnabled) },
                            enabled = !processMetricsBusy,
                            modifier = Modifier.testTag("settings-process-metrics-toggle"),
                        ) {
                            Text(
                                when {
                                    processMetricsBusy -> "Saving…"
                                    processMetricsLocalEnabled -> "✓ Local diagnostics enabled"
                                    else -> "Enable local diagnostics"
                                },
                            )
                        }
                        processMetricsError?.takeIf(String::isNotBlank)?.let { error ->
                            Text(
                                error,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.testTag("settings-process-metrics-error"),
                            )
                        }

                        Text("Feedback")
                        Text(
                            "Send product feedback through the canonical Host. Account credentials never enter Compose.",
                            modifier = Modifier.testTag("settings-feedback-description"),
                        )
                        TextButton(
                            onClick = onOpenFeedback,
                            enabled = accountLoggedIn,
                            modifier = Modifier.testTag("settings-feedback-open"),
                        ) {
                            Text("Send feedback")
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
                    FabushiSettingsSection.CREDENTIALS -> {
                        if (!accountLoggedIn) {
                            Text("Sign in to manage account-scoped credentials.")
                        } else {
                            Text("Credential Vault")
                            Text(
                                "Secrets are encrypted with Android Keystore. Saved values cannot be revealed; " +
                                    "only metadata and origin bindings are shown.",
                                modifier = Modifier.testTag("settings-credential-vault-description"),
                            )
                            if (credentialVaultBusy) {
                                LinearProgressIndicator(
                                    modifier = Modifier.testTag("settings-credential-vault-busy"),
                                )
                            }
                            credentialVaultError?.takeIf(String::isNotBlank)?.let { error ->
                                Text(
                                    error,
                                    color = MaterialTheme.colorScheme.error,
                                    modifier = Modifier.testTag("settings-credential-vault-error"),
                                )
                            }
                            credentialVaultItems.forEach { credential ->
                                Column(
                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                    modifier = Modifier.testTag("settings-credential-${credential.name}"),
                                ) {
                                    Text(credential.label.ifBlank { credential.name })
                                    Text(credential.name)
                                    Text(
                                        "Allowed origins: " +
                                            credential.allowedOrigins.joinToString().ifBlank { "none" },
                                    )
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        TextButton(
                                            onClick = {
                                                credentialNameDraft = credential.name
                                                credentialLabelDraft = credential.label
                                                credentialOriginsDraft =
                                                    credential.allowedOrigins.joinToString("\n")
                                                credentialSecretDraft = ""
                                                credentialRotate = true
                                            },
                                            enabled = !credentialVaultBusy,
                                            modifier = Modifier.testTag(
                                                "settings-credential-rotate-${credential.name}",
                                            ),
                                        ) { Text("Rotate") }
                                        TextButton(
                                            onClick = { onRevokeCredential(credential.name) },
                                            enabled = !credentialVaultBusy,
                                            modifier = Modifier.testTag(
                                                "settings-credential-revoke-${credential.name}",
                                            ),
                                        ) { Text("Revoke") }
                                    }
                                }
                            }
                            OutlinedTextField(
                                value = credentialNameDraft,
                                onValueChange = { credentialNameDraft = it.take(512) },
                                enabled = !credentialVaultBusy && !credentialRotate,
                                singleLine = true,
                                label = { Text("Secret reference") },
                                modifier = Modifier.testTag("settings-credential-name"),
                            )
                            OutlinedTextField(
                                value = credentialLabelDraft,
                                onValueChange = { credentialLabelDraft = it.take(200) },
                                enabled = !credentialVaultBusy,
                                singleLine = true,
                                label = { Text("Label") },
                                modifier = Modifier.testTag("settings-credential-label"),
                            )
                            OutlinedTextField(
                                value = credentialSecretDraft,
                                onValueChange = { credentialSecretDraft = it },
                                enabled = !credentialVaultBusy,
                                singleLine = true,
                                label = { Text(if (credentialRotate) "New secret value" else "Secret value") },
                                modifier = Modifier.testTag("settings-credential-secret"),
                            )
                            OutlinedTextField(
                                value = credentialOriginsDraft,
                                onValueChange = { credentialOriginsDraft = it.take(4096) },
                                enabled = !credentialVaultBusy,
                                label = { Text("Allowed origins (one per line)") },
                                modifier = Modifier.testTag("settings-credential-origins"),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                TextButton(
                                    onClick = {
                                        val name = credentialNameDraft.trim()
                                        val secret = credentialSecretDraft
                                        val origins = credentialOriginsDraft
                                            .lineSequence()
                                            .map(String::trim)
                                            .filter(String::isNotEmpty)
                                            .toList()
                                        if (name.isNotEmpty() && secret.isNotEmpty() && origins.isNotEmpty()) {
                                            onSaveCredential(
                                                name,
                                                credentialLabelDraft.trim(),
                                                secret,
                                                origins,
                                                credentialRotate,
                                            )
                                            credentialSecretDraft = ""
                                            if (!credentialRotate) {
                                                credentialNameDraft = ""
                                                credentialLabelDraft = ""
                                                credentialOriginsDraft = "https://"
                                            }
                                            credentialRotate = false
                                        }
                                    },
                                    enabled = !credentialVaultBusy &&
                                        credentialNameDraft.trim().isNotEmpty() &&
                                        credentialSecretDraft.isNotEmpty() &&
                                        credentialOriginsDraft.lineSequence().any { it.trim().isNotEmpty() },
                                    modifier = Modifier.testTag("settings-credential-save"),
                                ) {
                                    Text(if (credentialRotate) "Rotate credential" else "Save credential")
                                }
                                TextButton(
                                    onClick = {
                                        credentialNameDraft = ""
                                        credentialLabelDraft = ""
                                        credentialSecretDraft = ""
                                        credentialOriginsDraft = "https://"
                                        credentialRotate = false
                                    },
                                    enabled = !credentialVaultBusy,
                                    modifier = Modifier.testTag("settings-credential-discard"),
                                ) { Text("Discard") }
                                TextButton(
                                    onClick = onRefreshCredentialVault,
                                    enabled = !credentialVaultBusy,
                                    modifier = Modifier.testTag("settings-credential-refresh"),
                                ) { Text("Refresh") }
                            }
                        }
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
