package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
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
