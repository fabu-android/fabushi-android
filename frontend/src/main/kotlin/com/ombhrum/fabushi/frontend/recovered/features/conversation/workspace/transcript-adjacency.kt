package com.ombhrum.fabushi

/**
 * Immutable six-field transcript adjacency projection.
 *
 * Desktop authority:
 * frontend/src/recovered/features/conversation/workspace/transcript-adjacency.ts
 * @ 6fa8e4ccbe3e0b16806d168861ce9046d0132693.
 *
 * This file owns no transcript state. It only projects neighboring immutable rows so Compose can
 * render the same grouping seams without becoming a second transcript owner.
 */
internal data class TranscriptAdjacency(
    val isContinuedFromPrev: Boolean = false,
    val isContinuedToNext: Boolean = false,
    val isGroupStart: Boolean = false,
    val isRunStart: Boolean = false,
    val isFollowedByThreadChip: Boolean = false,
    val isGroupEnd: Boolean = false,
)

internal enum class TranscriptAdjacencyRole {
    ASSISTANT,
    USER,
    OTHER,
}

internal enum class TranscriptAdjacencyKind {
    MESSAGE,
    SEND_MESSAGE,
    OTHER,
}

internal data class TranscriptAdjacencyInput(
    val kind: TranscriptAdjacencyKind,
    val role: TranscriptAdjacencyRole = TranscriptAdjacencyRole.OTHER,
    val author: String = "",
    val sourceKind: String? = null,
    val text: String = "",
    val attachmentCount: Int = 0,
    val presentationKind: String? = null,
    val hasReaction: Boolean = false,
    val hasThreadChip: Boolean = false,
)

private data class TranscriptAdjacencySemantics(
    val role: TranscriptAdjacencyRole,
    val groupKey: String?,
    val isBubble: Boolean,
    val hasReaction: Boolean,
)

private val imageOnlyMarkdownPattern = Regex("""!\[[^\]]*]\([^)]*\)""")

private fun isImageOnlyMarkdown(value: String): Boolean {
    if (value.trim().isEmpty()) return false
    return imageOnlyMarkdownPattern.replace(value, "").trim().isEmpty()
}

/**
 * JVM-safe approximation of the RGI single-emoji predicate used by Desktop. It accepts one emoji
 * grapheme assembled from emoji bases, variation selectors, skin tones, ZWJ and keycap/flag
 * components while rejecting ordinary text and bare ASCII keycap bases.
 */
internal fun isStandaloneTranscriptEmoji(value: String): Boolean {
    val text = value.trim()
    if (text.isEmpty()) return false

    val codePoints = text.codePoints().toArray()
    var sawEmojiBase = false
    var sawKeycap = false
    var regionalIndicators = 0
    var ordinaryKeycapBases = 0
    for (codePoint in codePoints) {
        when {
            codePoint == 0x200D || codePoint == 0xFE0E || codePoint == 0xFE0F -> Unit
            codePoint in 0x1F3FB..0x1F3FF -> Unit
            codePoint == 0x20E3 -> sawKeycap = true
            codePoint in 0x1F1E6..0x1F1FF -> {
                regionalIndicators += 1
                sawEmojiBase = true
            }
            codePoint == 0x23 || codePoint == 0x2A || codePoint in 0x30..0x39 -> {
                ordinaryKeycapBases += 1
            }
            isEmojiBaseCodePoint(codePoint) -> sawEmojiBase = true
            else -> return false
        }
    }

    if (ordinaryKeycapBases > 0 && !sawKeycap) return false
    if (sawKeycap && ordinaryKeycapBases != 1) return false
    if (regionalIndicators > 0 && regionalIndicators != 2) return false
    return sawEmojiBase || sawKeycap
}

private fun isEmojiBaseCodePoint(codePoint: Int): Boolean =
    codePoint in 0x1F000..0x1FAFF ||
        codePoint in 0x2600..0x27FF ||
        codePoint in 0x2190..0x21FF ||
        codePoint in 0x2300..0x23FF ||
        codePoint in 0x2B00..0x2BFF ||
        codePoint in setOf(
            0x00A9,
            0x00AE,
            0x203C,
            0x2049,
            0x2122,
            0x2139,
            0x3030,
            0x303D,
            0x3297,
            0x3299,
        )

private fun transcriptAdjacencySemantics(entry: TranscriptAdjacencyInput): TranscriptAdjacencySemantics =
    when (entry.kind) {
        TranscriptAdjacencyKind.MESSAGE -> {
            val userAttachment = entry.sourceKind == "user-attachment"
            val role = if (userAttachment) TranscriptAdjacencyRole.USER else entry.role
            val roleKey = when (role) {
                TranscriptAdjacencyRole.ASSISTANT -> "assistant"
                TranscriptAdjacencyRole.USER -> "user"
                TranscriptAdjacencyRole.OTHER -> "other"
            }
            val groupKey = if (userAttachment) "user" else roleKey + ":" + entry.author
            val isBubble =
                !userAttachment &&
                    entry.attachmentCount == 0 &&
                    entry.text.trim().isNotEmpty() &&
                    !isImageOnlyMarkdown(entry.text) &&
                    !(role == TranscriptAdjacencyRole.USER && isStandaloneTranscriptEmoji(entry.text)) &&
                    entry.presentationKind != "url-card"
            TranscriptAdjacencySemantics(
                role = role,
                groupKey = groupKey,
                isBubble = isBubble,
                hasReaction = entry.hasReaction,
            )
        }

        TranscriptAdjacencyKind.SEND_MESSAGE ->
            TranscriptAdjacencySemantics(
                role = TranscriptAdjacencyRole.ASSISTANT,
                groupKey = "assistant",
                isBubble = false,
                hasReaction = entry.hasReaction,
            )

        TranscriptAdjacencyKind.OTHER ->
            TranscriptAdjacencySemantics(
                role = TranscriptAdjacencyRole.OTHER,
                groupKey = null,
                isBubble = false,
                hasReaction = false,
            )
    }

internal fun projectTranscriptAdjacency(
    entries: List<TranscriptAdjacencyInput>,
    isIndicatorSeamingBubble: Boolean = true,
): List<TranscriptAdjacency> =
    entries.mapIndexed { index, entry ->
        val current = transcriptAdjacencySemantics(entry)
        if (current.role == TranscriptAdjacencyRole.OTHER || current.groupKey == null) {
            return@mapIndexed TranscriptAdjacency()
        }

        val previousEntry = entries.getOrNull(index - 1)
        val nextEntry = entries.getOrNull(index + 1)
        val previous = previousEntry?.let(::transcriptAdjacencySemantics)
        val next = nextEntry?.let(::transcriptAdjacencySemantics)
        val entryHasThreadChip = entry.hasThreadChip
        val prevHasThreadChip = previousEntry?.hasThreadChip == true
        val isIndicatorSeaming =
            isIndicatorSeamingBubble &&
                current.role == TranscriptAdjacencyRole.ASSISTANT &&
                !entryHasThreadChip &&
                !current.hasReaction
        val isAssistantGroup = current.role == TranscriptAdjacencyRole.ASSISTANT

        TranscriptAdjacency(
            isContinuedFromPrev =
                current.isBubble &&
                    previous?.groupKey == current.groupKey &&
                    previous.isBubble &&
                    !prevHasThreadChip,
            isContinuedToNext =
                current.isBubble &&
                    (
                        (next?.groupKey == current.groupKey && next.isBubble) ||
                            isIndicatorSeaming
                    ),
            isGroupStart = previousEntry != null && previous?.groupKey != current.groupKey,
            isRunStart = previousEntry == null || previous?.groupKey != current.groupKey,
            isFollowedByThreadChip =
                current.isBubble && entryHasThreadChip && !current.hasReaction,
            isGroupEnd =
                !isAssistantGroup &&
                    (nextEntry == null || next?.groupKey != current.groupKey),
        )
    }

internal fun emptyTranscriptAdjacency(): TranscriptAdjacency = TranscriptAdjacency()

/**
 * Production adapter for Host-owned Agent transcript rows. Unsupported action/thinking/tool rows
 * intentionally break adjacency. Widget rows are the Android projection of Desktop send-message
 * cards and therefore retain the assistant fallback group while never becoming a message bubble.
 *
 * Thread ownership remains external: callers may provide ids from the canonical thread-summary
 * owner, while a row-projected flag is consumed when that owner already attached it upstream.
 */
internal fun projectAgentTranscriptAdjacency(
    messages: List<MobileChatMessage>,
    assistantAuthor: String,
    threadChipMessageIds: Set<String> = emptySet(),
): List<TranscriptAdjacency> =
    projectTranscriptAdjacency(
        messages.map { message ->
            val kind = when {
                message.kind == MobileChatEntryKind.MESSAGE && message.transcriptSourceKind == "send-message" ->
                    TranscriptAdjacencyKind.SEND_MESSAGE
                message.kind == MobileChatEntryKind.MESSAGE -> TranscriptAdjacencyKind.MESSAGE
                message.kind == MobileChatEntryKind.WIDGET -> TranscriptAdjacencyKind.SEND_MESSAGE
                else -> TranscriptAdjacencyKind.OTHER
            }
            val role = when (message.role) {
                MobileChatRole.USER -> TranscriptAdjacencyRole.USER
                MobileChatRole.ASSISTANT -> TranscriptAdjacencyRole.ASSISTANT
            }
            TranscriptAdjacencyInput(
                kind = kind,
                role = role,
                author = message.transcriptAuthor?.trim()?.takeIf(String::isNotEmpty)
                    ?: if (role == TranscriptAdjacencyRole.USER) "user" else assistantAuthor,
                sourceKind = message.transcriptSourceKind,
                text = message.text,
                attachmentCount = message.attachments.size,
                presentationKind = message.transcriptPresentationKind,
                hasReaction = message.hasTranscriptReaction,
                hasThreadChip = message.hasTranscriptThreadChip || message.id in threadChipMessageIds,
            )
        },
    )

private val attachmentContentTypes = setOf("photo", "video", "document", "voice", "audio")

/**
 * Production adapter for the current native Human transcript projection. Incoming rows use the
 * assistant-side presentation bucket solely for seam behavior; senderId remains the group identity,
 * so different remote authors never merge. No state is persisted here.
 */
internal fun projectChatMessageTranscriptAdjacency(
    messages: List<ChatMessage>,
    threadChipMessageIds: Set<String> = emptySet(),
): List<TranscriptAdjacency> =
    projectTranscriptAdjacency(
        messages.map { message ->
            TranscriptAdjacencyInput(
                kind = TranscriptAdjacencyKind.MESSAGE,
                role = if (message.outgoing) {
                    TranscriptAdjacencyRole.USER
                } else {
                    TranscriptAdjacencyRole.ASSISTANT
                },
                author = message.senderId.ifBlank {
                    if (message.outgoing) "user" else "remote"
                },
                text = message.text,
                attachmentCount = if (message.contentType in attachmentContentTypes) 1 else 0,
                presentationKind = message.contentType.takeIf { it == "url-card" },
                hasReaction = message.reactions.isNotEmpty(),
                hasThreadChip = message.id in threadChipMessageIds,
            )
        },
    )
