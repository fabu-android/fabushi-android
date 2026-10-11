package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class FabushiNativeThemeTest {
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
