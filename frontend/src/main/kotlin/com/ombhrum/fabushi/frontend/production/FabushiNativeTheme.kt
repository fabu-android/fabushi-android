package com.ombhrum.fabushi

import android.content.Context
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

internal enum class FabushiThemePreference(val wireValue: String) {
    SYSTEM("system"),
    LIGHT("light"),
    DARK("dark"),
}

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

internal data class FabushiThemeController(
    val preference: FabushiThemePreference,
    val setPreference: (FabushiThemePreference) -> Unit,
)

internal val LocalFabushiThemeController = staticCompositionLocalOf<FabushiThemeController> {
    error("Fabushi theme controller is only available inside FabushiNativeTheme")
}

private const val THEME_PREFS_NAME = "fabushi-native-settings-v1"
private const val THEME_SCHEMA_KEY = "schema_version"
private const val THEME_SCHEMA_VERSION = 1
private const val THEME_PREFERENCE_KEY = "theme_preference"

internal fun parseFabushiThemePreference(value: String?): FabushiThemePreference =
    FabushiThemePreference.entries.firstOrNull { it.wireValue == value } ?: FabushiThemePreference.SYSTEM

internal fun resolveFabushiThemeDark(
    preference: FabushiThemePreference,
    systemDark: Boolean,
): Boolean = when (preference) {
    FabushiThemePreference.SYSTEM -> systemDark
    FabushiThemePreference.LIGHT -> false
    FabushiThemePreference.DARK -> true
}

private fun readFabushiThemePreference(context: Context): FabushiThemePreference {
    val preferences = context.getSharedPreferences(THEME_PREFS_NAME, Context.MODE_PRIVATE)
    val schema = preferences.getInt(THEME_SCHEMA_KEY, THEME_SCHEMA_VERSION)
    if (schema != THEME_SCHEMA_VERSION) return FabushiThemePreference.SYSTEM
    return parseFabushiThemePreference(preferences.getString(THEME_PREFERENCE_KEY, null))
}

private fun writeFabushiThemePreference(
    context: Context,
    preference: FabushiThemePreference,
) {
    context.getSharedPreferences(THEME_PREFS_NAME, Context.MODE_PRIVATE)
        .edit()
        .putInt(THEME_SCHEMA_KEY, THEME_SCHEMA_VERSION)
        .putString(THEME_PREFERENCE_KEY, preference.wireValue)
        .apply()
}

/**
 * Android replacement for Desktop runtime-theme-token-installer.ts.
 *
 * Desktop mutates DOM data-theme/style nodes and emits CSS variables. Android has no DOM owner:
 * Compose consumes the same canonical light/dark base, text and accent intent as immutable theme
 * tokens. The durable owner here is a device-scoped, versioned Android preference; presentation
 * never owns account/runtime truth.
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
    val context = LocalContext.current.applicationContext
    var preference by remember(context) { mutableStateOf(readFabushiThemePreference(context)) }
    val systemDark = isSystemInDarkTheme()
    val tokens = projectFabushiNativeTheme(
        systemDark = resolveFabushiThemeDark(preference, systemDark),
    )
    val controller = remember(context, preference) {
        FabushiThemeController(
            preference = preference,
            setPreference = { next ->
                if (next != preference) {
                    writeFabushiThemePreference(context, next)
                    preference = next
                }
            },
        )
    }
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
    CompositionLocalProvider(LocalFabushiThemeController provides controller) {
        MaterialTheme(
            colorScheme = scheme,
            content = content,
        )
    }
}
