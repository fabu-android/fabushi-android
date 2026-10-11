package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection

internal const val ROSTER_PRIVACY_SETTINGS_URL =
    "https://cursor.com/dashboard/settings?openPrivacy=true"

internal fun isRosterPrivacyBlockFailure(
    code: String?,
    transportKind: String?,
): Boolean =
    code == "CLOUD_AGENT_STORAGE_DISABLED" ||
        transportKind == "no_storage"

internal fun isRosterPrivacyBlocked(
    projection: AccountAccessProjection,
): Boolean =
    projection.loggedIn &&
        isRosterPrivacyBlockFailure(
            projection.rosterFailureCode,
            projection.rosterFailureTransportKind,
        )

/**
 * Native fail-closed equivalent of Desktop PrivacyBlockedDialog.
 *
 * Account/roster truth remains Coordinator-owned. This dialog only projects the current
 * AccountAccessProjection and delegates sign-out/external navigation to canonical owners.
 */
@Composable
internal fun RosterPrivacyBlockedDialog(
    logoutBusy: Boolean,
    logoutError: String?,
    onSignOut: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    val settingsFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        settingsFocus.requestFocus()
    }

    AlertDialog(
        modifier = Modifier.testTag("roster-privacy-blocked-dialog"),
        onDismissRequest = {},
        properties = DialogProperties(
            dismissOnBackPress = false,
            dismissOnClickOutside = false,
        ),
        title = { Text("Update Privacy Mode") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    "Privacy Mode (Legacy) isn’t compatible with Fabushi. " +
                        "Switch to Privacy Mode to continue — data still isn’t used for training.",
                )
                Text("This setting is shared with Cursor. Leaving Legacy can’t be undone.")
                logoutError?.takeIf(String::isNotBlank)?.let {
                    Text(
                        text = "Sign out failed: $it",
                        modifier = Modifier.testTag("roster-privacy-blocked-error"),
                    )
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = onOpenSettings,
                enabled = !logoutBusy,
                modifier = Modifier
                    .focusRequester(settingsFocus)
                    .testTag("roster-privacy-open-settings"),
            ) {
                Text("Open Privacy Settings")
            }
        },
        dismissButton = {
            TextButton(
                onClick = onSignOut,
                enabled = !logoutBusy,
                modifier = Modifier.testTag("roster-privacy-sign-out"),
            ) {
                Text(if (logoutBusy) "Signing out…" else "Sign out")
            }
        },
    )
}
