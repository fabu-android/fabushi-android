package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
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
    deepLinks: SharedFlow<AndroidPresentationDeepLink>,
    updateModel: AndroidUpdateViewModel,
    runtimePort: AndroidPresentationRuntimePort,
) {
    ProductionRenderer(
        activity = activity,
        deepLinks = deepLinks,
        updateModel = updateModel,
        runtimePort = runtimePort,
    )
}
