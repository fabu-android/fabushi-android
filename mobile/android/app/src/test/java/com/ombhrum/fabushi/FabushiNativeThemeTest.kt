package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class FabushiNativeThemeTest {
    @Test
    fun themePreferenceParsingFailsClosedToSystem() {
        assertEquals(FabushiThemePreference.SYSTEM, parseFabushiThemePreference(null))
        assertEquals(FabushiThemePreference.SYSTEM, parseFabushiThemePreference("unknown"))
        assertEquals(FabushiThemePreference.SYSTEM, parseFabushiThemePreference("system"))
        assertEquals(FabushiThemePreference.LIGHT, parseFabushiThemePreference("light"))
        assertEquals(FabushiThemePreference.DARK, parseFabushiThemePreference("dark"))
    }

    @Test
    fun explicitThemePreferenceOverridesSystemOnlyWhenSelected() {
        assertEquals(true, resolveFabushiThemeDark(FabushiThemePreference.SYSTEM, systemDark = true))
        assertEquals(false, resolveFabushiThemeDark(FabushiThemePreference.SYSTEM, systemDark = false))
        assertEquals(false, resolveFabushiThemeDark(FabushiThemePreference.LIGHT, systemDark = true))
        assertEquals(true, resolveFabushiThemeDark(FabushiThemePreference.DARK, systemDark = false))
    }

    @Test
    fun lightThemeProjectsDesktopBaseTextAndAccentIntent() {
        val value = projectFabushiNativeTheme(systemDark = false)
        assertEquals(FabushiNativeThemeMode.LIGHT, value.mode)
        assertEquals(0xFFFCFCFC.toInt(), value.backgroundArgb)
        assertEquals(0xFF141414.toInt(), value.foregroundArgb)
        assertEquals(0xFF0C64C1.toInt(), value.accentArgb)
        assertNotEquals(value.backgroundArgb, value.foregroundArgb)
    }

    @Test
    fun darkThemeProjectsDesktopBaseTextAndAccentIntent() {
        val value = projectFabushiNativeTheme(systemDark = true)
        assertEquals(FabushiNativeThemeMode.DARK, value.mode)
        assertEquals(0xFF070707.toInt(), value.backgroundArgb)
        assertEquals(0xFFFCFCFC.toInt(), value.foregroundArgb)
        assertEquals(0xFF459FFE.toInt(), value.accentArgb)
        assertNotEquals(value.backgroundArgb, value.foregroundArgb)
    }
}
