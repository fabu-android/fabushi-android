package com.ombhrum.fabushi

internal const val FEEDBACK_MESSAGE_MAX_CHARS = 10_000

internal enum class FeedbackPhase {
    IDLE,
    SENDING,
    SENT,
    FAILED,
    OUTCOME_UNKNOWN,
}

internal fun feedbackPhaseFromSaved(value: String?): FeedbackPhase =
    runCatching { FeedbackPhase.valueOf(value.orEmpty()) }
        .getOrDefault(FeedbackPhase.IDLE)

/**
 * A restored in-flight feedback request is not safe to classify as failed.
 *
 * The backend consumes a stable submission id, so the user may explicitly retry the exact same
 * logical operation. Presentation must never auto-replay an indeterminate side effect.
 */
internal fun feedbackPhaseAfterProcessRestore(value: String?): FeedbackPhase =
    when (feedbackPhaseFromSaved(value)) {
        FeedbackPhase.SENDING -> FeedbackPhase.OUTCOME_UNKNOWN
        else -> feedbackPhaseFromSaved(value)
    }

internal fun feedbackCanSend(message: String, phase: FeedbackPhase): Boolean =
    phase != FeedbackPhase.SENDING &&
        phase != FeedbackPhase.SENT &&
        message.trim().isNotEmpty() &&
        message.length <= FEEDBACK_MESSAGE_MAX_CHARS

internal fun feedbackCanDismiss(phase: FeedbackPhase): Boolean =
    phase != FeedbackPhase.SENDING

internal fun feedbackRetryKeepsSubmissionIdentity(phase: FeedbackPhase): Boolean =
    phase == FeedbackPhase.FAILED || phase == FeedbackPhase.OUTCOME_UNKNOWN

internal fun feedbackErrorMessage(code: String?): String = when (code) {
    "access-denied" -> "Feedback is not available for this account."
    "invalid-feedback" -> "Feedback could not be sent because the message is invalid."
    "not-signed-in" -> "Sign in again before sending feedback."
    "rate-limited" -> "Too many feedback requests. Try again later."
    "subscription-required" -> "Feedback requires an active subscription."
    else -> "Feedback could not be sent. Try again when the service is available."
}

internal fun feedbackOutcomeUnknownMessage(): String =
    "The previous submission may have completed. Retry will reuse the same submission identity."
