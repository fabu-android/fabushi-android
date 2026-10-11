package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.ombhrum.fabushi.androidpreload.runtime.AndroidUpdateTrackPolicy

private enum class FabushiSettingsSection {
    GENERAL,
    UPDATES,
}

@Composable
internal fun FabushiSettingsSurface(
    updateState: AndroidUpdateUiState,
    updateTrackPolicy: AndroidUpdateTrackPolicy,
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
    onClose: () -> Unit,
) {
    var section by remember { mutableStateOf(FabushiSettingsSection.GENERAL) }
    val theme = LocalFabushiThemeController.current
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
