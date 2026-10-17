package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SignedInOnboardingModelTest {
    @Test
    fun onboardingOnlyOpensForSignedInUnseenEmptyRoster() {
        assertTrue(signedInOnboardingShouldOpen(true, false, 0))
        assertTrue(signedInOnboardingShouldOpen(true, false, null))
        assertFalse(signedInOnboardingShouldOpen(false, false, 0))
        assertFalse(signedInOnboardingShouldOpen(true, true, 0))
        assertFalse(signedInOnboardingShouldOpen(true, false, 1))
    }

    @Test
    fun stepOrderMatchesDesktopSignedInFlow() {
        assertEquals(SignedInOnboardingStep.COMPUTER_DEMO, nextSignedInOnboardingStep(SignedInOnboardingStep.MEET))
        assertEquals(SignedInOnboardingStep.JOBS, nextSignedInOnboardingStep(SignedInOnboardingStep.COMPUTER_DEMO))
        assertEquals(SignedInOnboardingStep.TOOLS, nextSignedInOnboardingStep(SignedInOnboardingStep.JOBS))
        assertEquals(SignedInOnboardingStep.CREATE, nextSignedInOnboardingStep(SignedInOnboardingStep.TOOLS))
        assertEquals(SignedInOnboardingStep.HAND_OFF, nextSignedInOnboardingStep(SignedInOnboardingStep.CREATE))
        assertEquals(SignedInOnboardingStep.TOOLS, previousSignedInOnboardingStep(SignedInOnboardingStep.CREATE))
        assertNull(previousSignedInOnboardingStep(SignedInOnboardingStep.MEET))
    }

    @Test
    fun telemetryStepNamesMatchDesktopAnalyticsContract() {
        val expected = listOf(
            "meet" to 0,
            "computer-demo" to 1,
            "jobs" to 2,
            "tools" to 3,
            "create" to 4,
            "hand-off" to 5,
        )
        assertEquals(
            expected,
            SignedInOnboardingStep.entries.map { step ->
                signedInOnboardingTelemetryStep(step).let { it.name to it.index }
            },
        )
    }

    @Test
    fun telemetryStepMappingMatchesDesktopAnalyticsContract() {
        assertEquals(
            listOf("meet", "computer-demo", "jobs", "tools", "create", "hand-off"),
            SignedInOnboardingStep.entries.map { signedInOnboardingTelemetryStep(it).name },
        )
        assertEquals(
            listOf(0, 1, 2, 3, 4, 5),
            SignedInOnboardingStep.entries.map { signedInOnboardingTelemetryStep(it).index },
        )
    }

    @Test
    fun invalidPersonaDefaultsToDesktopBlueBlob() {
        val normalized = normalizeSignedInOnboardingDraft(
            SignedInOnboardingDraft(name = "Agent", color = "purple-ish", shape = "triangle"),
        )
        assertEquals("blue", normalized.color)
        assertEquals("blob", normalized.shape)
        assertTrue(canSubmitSignedInOnboardingDraft(normalized))
        assertFalse(canSubmitSignedInOnboardingDraft(normalized.copy(name = "  ")))
    }

    @Test
    fun selectedDailyToolsAreCanonicalOrderedAndAppendedToDescription() {
        assertEquals(
            "base The user works with Slack, GitHub every day — start with those tools when suggesting connectors or taking on work.",
            signedInOnboardingDescription("base", setOf("GitHub", "Slack", "unknown")),
        )
        assertEquals(listOf("GitHub"), filterSignedInOnboardingTools("git"))
    }

    @Test
    fun suggestionRankingUsesSelectedToolThenUniversalWithoutDuplicates() {
        val rows = selectSignedInOnboardingSuggestions(listOf("GitHub"), limit = 3)
        assertEquals(listOf("QA Engineer", "Night Shift", "Inbox Triage"), rows.map { it.name })
        assertEquals("Clicks through every new GitHub deploy and reports what breaks", rows.first().description)
        assertEquals(rows.map { it.templateId }.distinct().size, rows.size)
    }

    @Test
    fun handoffStatusMatchesDesktopPriority() {
        assertEquals("Getting your team ready…", signedInOnboardingHandoffStatus(true, 12, "hibernated"))
        assertEquals("Setting up your Grok Bot… 42%", signedInOnboardingHandoffStatus(false, 42, "hibernated"))
        assertEquals("Waking your computer…", signedInOnboardingHandoffStatus(false, null, "hibernated"))
        assertEquals("Setting up your Grok Bot…", signedInOnboardingHandoffStatus(false, null, null))
    }

    @Test
    fun operationFenceRejectsDuplicateAndOldAccountCallbacks() {
        val fence = SignedInOnboardingOperationFence()
        fence.setAccount("account-a")
        val first = requireNotNull(fence.begin())
        assertTrue(fence.accepts(first, "account-a"))
        assertNull(fence.begin())
        fence.setAccount("account-b")
        assertFalse(fence.accepts(first, "account-a"))
        val second = requireNotNull(fence.begin())
        assertFalse(fence.accepts(second, "account-a"))
        assertTrue(fence.accepts(second, "account-b"))
        fence.finish(second)
        assertTrue(fence.begin() != null)
    }
    @Test
    fun createFailureMapsTransportToDesktopConnectionCopyWithoutMaskingOtherErrors() {
        assertEquals(
            "Can't reach your computer right now. Check your connection and try again.",
            signedInOnboardingCreateError(IllegalStateException("transport unavailable")),
        )
        assertEquals(
            "invalid Agent name",
            signedInOnboardingCreateError(IllegalArgumentException("invalid Agent name")),
        )
    }

}
