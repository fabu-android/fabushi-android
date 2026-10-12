package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class AccountPrivacyModePresentationTest {
    @Test
    fun canonicalModesHaveStableReadOnlyLabels() {
        assertEquals("No storage", accountPrivacyModeLabel("no-storage"))
        assertEquals("No training", accountPrivacyModeLabel("no-training"))
        assertEquals(
            "Usage data training allowed",
            accountPrivacyModeLabel("usage-data-training-allowed"),
        )
        assertEquals(
            "Codebase training allowed",
            accountPrivacyModeLabel("usage-codebase-training-allowed"),
        )
        assertEquals("Unspecified", accountPrivacyModeLabel("unspecified"))
    }

    @Test
    fun missingOrUnknownPolicyIsNotInvented() {
        assertEquals("Unavailable", accountPrivacyModeLabel(null))
        assertEquals("Unavailable", accountPrivacyModeLabel(""))
        assertEquals("Unavailable", accountPrivacyModeLabel("unknown"))
        assertEquals("Unavailable", accountPrivacyModeLabel("future-mode"))
    }
}
