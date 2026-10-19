package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FeedbackDialogModelTest {
    @Test
    fun desktopBoundsAndSendingLockArePreserved() {
        assertFalse(feedbackCanSend("   ", FeedbackPhase.IDLE))
        assertTrue(feedbackCanSend("x".repeat(FEEDBACK_MESSAGE_MAX_CHARS), FeedbackPhase.IDLE))
        assertFalse(feedbackCanSend("x".repeat(FEEDBACK_MESSAGE_MAX_CHARS + 1), FeedbackPhase.IDLE))
        assertFalse(feedbackCanSend("feedback", FeedbackPhase.SENDING))
        assertFalse(feedbackCanDismiss(FeedbackPhase.SENDING))
        assertTrue(feedbackCanDismiss(FeedbackPhase.IDLE))
        assertFalse(feedbackCanSend("feedback", FeedbackPhase.SENT))
    }

    @Test
    fun processRestoreNeverTreatsAnInFlightSideEffectAsFailedOrUnsent() {
        assertEquals(
            FeedbackPhase.OUTCOME_UNKNOWN,
            feedbackPhaseAfterProcessRestore(FeedbackPhase.SENDING.name),
        )
        assertEquals(
            FeedbackPhase.SENT,
            feedbackPhaseAfterProcessRestore(FeedbackPhase.SENT.name),
        )
        assertEquals(
            FeedbackPhase.FAILED,
            feedbackPhaseAfterProcessRestore(FeedbackPhase.FAILED.name),
        )
        assertEquals(
            FeedbackPhase.IDLE,
            feedbackPhaseAfterProcessRestore("unknown-future-phase"),
        )
        assertTrue(feedbackCanSend("retry", FeedbackPhase.OUTCOME_UNKNOWN))
        assertTrue(feedbackRetryKeepsSubmissionIdentity(FeedbackPhase.OUTCOME_UNKNOWN))
        assertTrue(feedbackRetryKeepsSubmissionIdentity(FeedbackPhase.FAILED))
        assertFalse(feedbackRetryKeepsSubmissionIdentity(FeedbackPhase.IDLE))
    }

    @Test
    fun typedFailureCodesMatchDesktopFallbackContract() {
        assertEquals(
            "Feedback is not available for this account.",
            feedbackErrorMessage("access-denied"),
        )
        assertEquals(
            "Feedback could not be sent because the message is invalid.",
            feedbackErrorMessage("invalid-feedback"),
        )
        assertEquals(
            "Sign in again before sending feedback.",
            feedbackErrorMessage("not-signed-in"),
        )
        assertEquals(
            "Too many feedback requests. Try again later.",
            feedbackErrorMessage("rate-limited"),
        )
        assertEquals(
            "Feedback requires an active subscription.",
            feedbackErrorMessage("subscription-required"),
        )
        assertEquals(
            "Feedback could not be sent. Try again when the service is available.",
            feedbackErrorMessage("future-code"),
        )
    }

    @Test
    fun outcomeUnknownCopyRequiresExplicitSameIdentityRetry() {
        assertEquals(
            "The previous submission may have completed. Retry will reuse the same submission identity.",
            feedbackOutcomeUnknownMessage(),
        )
    }
}
