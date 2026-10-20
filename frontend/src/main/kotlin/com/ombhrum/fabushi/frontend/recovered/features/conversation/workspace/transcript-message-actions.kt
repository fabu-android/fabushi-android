package com.ombhrum.fabushi

/**
 * Pure presentation gate for ordinary transcript message actions.
 *
 * Desktop authority:
 * frontend/src/recovered/features/conversation/workspace/transcript.tsx
 * @ 5cf2f33c394f885a37709a01361dc516b0fb4240.
 *
 * This projection owns no message state. Canonical delivery remains in the messaging/runtime owner;
 * Compose only consumes this immutable decision when exposing reply/selection gestures.
 */
internal data class TranscriptMessageActionProjection(
    val actionable: Boolean,
    val canReply: Boolean,
    val canSelect: Boolean,
    val copyText: String?,
    val actionLabel: String,
)

private val transcriptNonActionableDeliveryStates = setOf("failed", "pending", "queued")

internal fun projectTranscriptMessageActions(
    message: ChatMessage,
): TranscriptMessageActionProjection {
    val deliveryActionable =
        message.deliveryState.trim().lowercase() !in transcriptNonActionableDeliveryStates
    val hasActionableContent =
        message.text.isNotBlank() ||
            message.mediaBlobId != null ||
            message.contentType != "text"
    val actionable = hasActionableContent && deliveryActionable
    val copyText = message.text
        .takeIf { actionable && message.contentType == "text" && it.isNotBlank() }
    return TranscriptMessageActionProjection(
        actionable = actionable,
        canReply = actionable,
        canSelect = actionable,
        copyText = copyText,
        actionLabel = if (message.outgoing) {
            "Message actions for your message (${message.id})"
        } else {
            "Message actions for message (${message.id})"
        },
    )
}
