package com.ombhrum.fabushi

/**
 * Android counterpart of the renderer draft-state boundary.
 *
 * Unlike desktop, Android does not own a second client-persistence store. Messaging drafts are
 * canonical Host/Coordinator state and arrive through syncBatch/draftChanged. This adapter only
 * projects that canonical state into the Compose workspace and decides whether a write is needed.
 */
internal data class CoordinatorComposerDraftSnapshot(
    val conversationId: String,
    val text: String,
    val replyToMessageId: String?,
    val updatedAtMs: Long,
)

internal fun coordinatorComposerDraftSnapshot(
    conversationId: String,
    sharedDraft: MessagingDraft?,
): CoordinatorComposerDraftSnapshot {
    val accepted = sharedDraft?.takeIf { it.conversationId == conversationId }
    return CoordinatorComposerDraftSnapshot(
        conversationId = conversationId,
        text = accepted?.text.orEmpty(),
        replyToMessageId = accepted?.replyToMessageId,
        updatedAtMs = accepted?.updatedAtMs ?: 0L,
    )
}

internal fun composerDraftIsEmpty(text: String, replyToMessageId: String?): Boolean =
    text.isBlank() && replyToMessageId == null

internal fun shouldPersistCoordinatorDraft(
    snapshot: CoordinatorComposerDraftSnapshot,
    text: String,
    replyToMessageId: String?,
): Boolean =
    snapshot.text != text || snapshot.replyToMessageId != replyToMessageId
