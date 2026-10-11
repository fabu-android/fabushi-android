package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandPaletteUpdateCommandTest {
    @Test
    fun outcomeUnknownRequiresExplicitUserActivationAndUsesInstallPathOnly() {
        var checks = 0
        var installs = 0
        var opens = 0
        val entry = commandPaletteUpdateCommand(
            state = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.OUTCOME_UNKNOWN,
                currentVersion = "1.2.3",
                availableVersion = "1.2.4",
                message = "Prior installer outcome is unknown.",
            ),
            check = { checks += 1 },
            install = { installs += 1 },
            openUpdates = { opens += 1 },
        )

        assertNotNull(entry)
        assertEquals("Verify and Continue Update…", entry!!.label)
        assertEquals(0, checks)
        assertEquals(0, installs)
        assertEquals(0, opens)

        entry.activate()

        assertEquals(0, checks)
        assertEquals(1, installs)
        assertEquals(0, opens)
    }

    @Test
    fun disabledUpdaterStillOmitsPaletteCommand() {
        val entry = commandPaletteUpdateCommand(
            state = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.DISABLED,
                currentVersion = "1.2.3",
            ),
            check = { throw AssertionError("check must not run") },
            install = { throw AssertionError("install must not run") },
            openUpdates = { throw AssertionError("open updates must not run") },
        )

        assertTrue(entry == null)
    }
}
