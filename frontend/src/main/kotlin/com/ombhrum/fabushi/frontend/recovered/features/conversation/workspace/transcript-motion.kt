package com.ombhrum.fabushi

import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext

/**
 * Native replacement for transcript-utility-parity.css prefers-reduced-motion.
 *
 * Android's global animator scale is the platform user preference. Transcript scrolling uses this
 * only for presentation motion; canonical transcript state and navigation targets remain unchanged.
 */
internal fun transcriptShouldReduceMotion(animatorDurationScale: Float): Boolean =
    !animatorDurationScale.isFinite() || animatorDurationScale <= 0f

@Composable
internal fun rememberTranscriptReducedMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        val scale = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f)
        transcriptShouldReduceMotion(scale)
    }
}
