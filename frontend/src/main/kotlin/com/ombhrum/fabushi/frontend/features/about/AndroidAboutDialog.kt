package com.ombhrum.fabushi

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

internal fun aboutVersionInfo(state: AndroidUpdateUiState): String =
    listOf(
        "Version: ${state.currentVersion}",
        "Version Code: ${state.currentVersionCode}",
        "OS: Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})",
    ).joinToString("\n")

@Composable
internal fun AndroidAboutDialog(
    updateState: AndroidUpdateUiState,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    var copied by remember { mutableStateOf(false) }

    LaunchedEffect(copied) {
        if (!copied) return@LaunchedEffect
        delay(1_200)
        copied = false
    }

    AlertDialog(
        onDismissRequest = onClose,
        modifier = Modifier.testTag("android-about-dialog"),
        title = { Text("About Fabushi") },
        text = {
            Column {
                Text("Fabushi")
                Text(
                    "Version ${updateState.currentVersion} (${updateState.currentVersionCode})",
                    modifier = Modifier.padding(top = 8.dp),
                )
                Text(
                    "Android ${Build.VERSION.RELEASE} · API ${Build.VERSION.SDK_INT}",
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                    clipboard.setPrimaryClip(
                        ClipData.newPlainText(
                            "Fabushi version information",
                            aboutVersionInfo(updateState),
                        ),
                    )
                    copied = true
                },
                modifier = Modifier.testTag("android-about-copy-version"),
            ) {
                Text(if (copied) "Copied" else "Copy version info")
            }
        },
        dismissButton = {
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("android-about-close"),
            ) {
                Text("Close")
            }
        },
    )
}
