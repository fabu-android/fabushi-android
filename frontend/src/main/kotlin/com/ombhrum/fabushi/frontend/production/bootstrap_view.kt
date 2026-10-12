package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidPresentationDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import kotlinx.coroutines.flow.SharedFlow

/**
 * Android production bootstrap corresponding to Grok frontend/src/production/bootstrap.tsx.
 *
 * MainActivity owns Android lifecycle/system callbacks. Bootstrap wires those platform inputs into
 * the production renderer and does not own Coordinator/Host state.
 */
@Composable
internal fun FabushiApplicationRoot(
    activity: ComponentActivity,
    keyboardShortcutController: GlobalKeyboardShortcutController,
    deepLinks: SharedFlow<AndroidPresentationDeepLink>,
    updateModel: AndroidUpdateViewModel,
    runtimePort: AndroidPresentationRuntimePort,
) {
    // Desktop reserves renderer space for native title-bar/window controls. On Android the
    // system window owns those controls, while edge-to-edge content must explicitly consume
    // display cutout/status/navigation safe drawing insets. Keep that platform responsibility
    // at the root instead of duplicating inset state in individual feature surfaces.
    Box(
        modifier = Modifier
            .fillMaxSize()
            .safeDrawingPadding(),
    ) {
        ProductionRenderer(
            activity = activity,
            keyboardShortcutController = keyboardShortcutController,
            deepLinks = deepLinks,
            updateModel = updateModel,
            runtimePort = runtimePort,
        )
    }
}


internal data class FabushiRootFailure(
    val title: String,
    val detail: String,
    val diagnostics: String,
)

private fun redactRootFailureText(value: String): String =
    value.replace(
        Regex("""(?i)(bearer|token|secret|password|authorization)\s*[:=]\s*[^\s,;]+"""),
        "$1=<redacted>",
    )

internal fun projectFabushiRootFailure(error: Throwable): FabushiRootFailure {
    val type = error::class.java.simpleName.ifBlank { "RuntimeException" }.take(120)
    val message = error.message?.trim().orEmpty().ifBlank { "The application runtime could not start." }
    val detail = redactRootFailureText(message).take(480)
    val stack = redactRootFailureText(error.stackTraceToString()).take(8_192)
    return FabushiRootFailure(
        title = "Fabushi couldn’t start",
        detail = detail,
        diagnostics = buildString {
            append("Fabushi Android root failure\n")
            append("type=").append(type).append('\n')
            append("detail=").append(detail).append('\n')
            if (stack.isNotBlank()) append(stack)
        }.take(8_192),
    )
}

@Composable
internal fun FabushiRootErrorSurface(
    failure: FabushiRootFailure,
    onRetry: () -> Unit,
) {
    val context = LocalContext.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp)
            .semantics { liveRegion = LiveRegionMode.Assertive },
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text = failure.title,
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.semantics { heading() },
        )
        Text(text = failure.detail, style = MaterialTheme.typography.bodyMedium)
        Button(onClick = onRetry) { Text("Retry") }
        OutlinedButton(
            onClick = {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(ClipData.newPlainText("Fabushi diagnostics", failure.diagnostics))
            },
        ) {
            Text("Copy diagnostics")
        }
    }
}
