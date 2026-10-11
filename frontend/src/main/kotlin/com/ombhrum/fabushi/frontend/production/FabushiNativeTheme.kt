package com.ombhrum.fabushi

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

internal enum class FabushiNativeThemeMode {
    LIGHT,
    DARK,
}

internal data class FabushiNativeThemeTokens(
    val mode: FabushiNativeThemeMode,
    val backgroundArgb: Int,
    val surfaceArgb: Int,
    val foregroundArgb: Int,
    val secondaryTextArgb: Int,
    val accentArgb: Int,
)

/**
 * Android replacement for Desktop runtime-theme-token-installer.ts.
 *
 * Desktop mutates DOM data-theme/style nodes and emits CSS variables. Android has no DOM owner:
 * Compose consumes the same canonical light/dark base, text and accent intent as immutable theme
 * tokens. This is presentation-only and must never own account or durable product settings.
 */
internal fun projectFabushiNativeTheme(
    systemDark: Boolean,
): FabushiNativeThemeTokens =
    if (systemDark) {
        FabushiNativeThemeTokens(
            mode = FabushiNativeThemeMode.DARK,
            backgroundArgb = 0xFF070707.toInt(),
            surfaceArgb = 0xFF181818.toInt(),
            foregroundArgb = 0xFFFCFCFC.toInt(),
            secondaryTextArgb = 0x99FCFCFC.toInt(),
            accentArgb = 0xFF459FFE.toInt(),
        )
    } else {
        FabushiNativeThemeTokens(
            mode = FabushiNativeThemeMode.LIGHT,
            backgroundArgb = 0xFFFCFCFC.toInt(),
            surfaceArgb = 0xFFFCFCFC.toInt(),
            foregroundArgb = 0xFF141414.toInt(),
            secondaryTextArgb = 0x99141414.toInt(),
            accentArgb = 0xFF0C64C1.toInt(),
        )
    }

@Composable
internal fun FabushiNativeTheme(
    content: @Composable () -> Unit,
) {
    val tokens = projectFabushiNativeTheme(isSystemInDarkTheme())
    val scheme = when (tokens.mode) {
        FabushiNativeThemeMode.LIGHT -> lightColorScheme(
            primary = Color(tokens.accentArgb),
            background = Color(tokens.backgroundArgb),
            surface = Color(tokens.surfaceArgb),
            onBackground = Color(tokens.foregroundArgb),
            onSurface = Color(tokens.foregroundArgb),
        )
        FabushiNativeThemeMode.DARK -> darkColorScheme(
            primary = Color(tokens.accentArgb),
            background = Color(tokens.backgroundArgb),
            surface = Color(tokens.surfaceArgb),
            onBackground = Color(tokens.foregroundArgb),
            onSurface = Color(tokens.foregroundArgb),
        )
    }
    MaterialTheme(
        colorScheme = scheme,
        content = content,
    )
}
