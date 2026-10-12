package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RosterPrivacyBlockedTest {
    @Test
    fun exactDesktopFailureCodeBlocks() {
        assertTrue(isRosterPrivacyBlockFailure("CLOUD_AGENT_STORAGE_DISABLED", null))
    }

    @Test
    fun exactDesktopNoStorageTransportBlocks() {
        assertTrue(isRosterPrivacyBlockFailure("other", "no_storage"))
    }

    @Test
    fun unrelatedFailuresDoNotBlockAndLoggedOutProjectionStaysHidden() {
        assertFalse(isRosterPrivacyBlockFailure("network", "gateway"))
        val loggedOut = AccountAccessProjection.initial().copy(
            rosterFailureCode = "CLOUD_AGENT_STORAGE_DISABLED",
        )
        assertFalse(isRosterPrivacyBlocked(loggedOut))
    }

    @Test
    fun currentLoggedInProjectionDrivesBlockingSurface() {
        val projection = AccountAccessProjection.initial(9L).copy(
            loggedIn = true,
            rosterFailureTransportKind = "no_storage",
        )
        assertTrue(isRosterPrivacyBlocked(projection))
    }

    @Test
    fun settingsDestinationMatchesDesktopAuthority() {
        assertEquals(
            "https://cursor.com/dashboard/settings?openPrivacy=true",
            ROSTER_PRIVACY_SETTINGS_URL,
        )
    }
}
