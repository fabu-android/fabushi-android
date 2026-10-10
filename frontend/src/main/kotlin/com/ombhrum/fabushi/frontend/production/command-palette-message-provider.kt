package com.ombhrum.fabushi

import org.json.JSONArray

internal const val COMMAND_PALETTE_MESSAGE_DEBOUNCE_MS = 150L

internal enum class CommandPaletteMessageStatus {
    UNAVAILABLE,
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    CANCELLED,
}

internal data class CommandPaletteMessage(
    val agentId: String,
    val entryId: String,
    val role: MobileChatRole,
    val timestampMs: Long,
    val snippet: String,
)

internal data class CommandPaletteMessageSnapshot(
    val status: CommandPaletteMessageStatus = CommandPaletteMessageStatus.UNAVAILABLE,
    val value: List<CommandPaletteMessage> = emptyList(),
)

/**
 * Explicit generation fence for debounce/cancellation races. A result may publish only if its
 * token is still current after the Host projection returns.
 */
internal class CommandPaletteMessageRequestFence {
    private var generation = 0L

    fun begin(): Long {
        generation += 1
        return generation
    }

    fun cancel() {
        generation += 1
    }

    fun accepts(token: Long): Boolean = token == generation
}

private fun commandPaletteSnippet(text: String, maxLength: Int = 160): String {
    val normalized = text.replace(Regex("\\s+"), " ").trim()
    if (normalized.length <= maxLength) return normalized
    return normalized.take(maxLength - 1).trimEnd() + "…"
}

internal fun commandPaletteMessagesFromTranscript(
    transcript: JSONArray,
    query: String,
    limit: Int = 50,
): List<CommandPaletteMessage> {
    val normalizedQuery = query.trim()
    if (normalizedQuery.isEmpty() || limit <= 0) return emptyList()

    return canonicalAgentTranscriptMessages(transcript)
        .mapNotNull { message ->
            val snippet = commandPaletteSnippet(message.text)
            if (snippet.isBlank()) return@mapNotNull null
            val score = fuzzyPaletteScore(normalizedQuery, snippet) ?: return@mapNotNull null
            Triple(
                score,
                message.timestampMs,
                CommandPaletteMessage(
                    agentId = message.agentId,
                    entryId = message.id,
                    role = message.role,
                    timestampMs = message.timestampMs,
                    snippet = snippet,
                ),
            )
        }
        .sortedWith(
            compareByDescending<Triple<Int, Long, CommandPaletteMessage>> { it.first }
                .thenByDescending { it.second },
        )
        .take(limit)
        .map { it.third }
}

internal fun commandPaletteMessageEntries(
    messages: List<CommandPaletteMessage>,
    agentNames: Map<String, String>,
    nowMs: Long = System.currentTimeMillis(),
    onOpen: (CommandPaletteMessage) -> Unit,
): List<CommandPaletteEntry> =
    messages.mapNotNull { message ->
        val agentName = agentNames[message.agentId]?.takeIf(String::isNotBlank)
            ?: return@mapNotNull null
        val detail = listOfNotNull(
            if (message.role == MobileChatRole.USER) "You to $agentName" else "$agentName to you",
            relativeCommandPaletteTime(message.timestampMs, nowMs).takeIf(String::isNotBlank),
        ).joinToString(" · ")
        CommandPaletteEntry(
            id = "message:${message.agentId}:${message.entryId}",
            kind = CommandPaletteEntryKind.MESSAGE,
            label = message.snippet,
            detail = detail,
            searchText = "${message.snippet} $agentName",
            activate = { onOpen(message) },
        )
    }

private fun relativeCommandPaletteTime(timestampMs: Long, nowMs: Long): String {
    if (timestampMs <= 0L || nowMs <= timestampMs) return if (timestampMs > 0L) "now" else ""
    val seconds = (nowMs - timestampMs) / 1_000L
    return when {
        seconds < 60L -> "now"
        seconds < 3_600L -> "${seconds / 60L}m ago"
        seconds < 86_400L -> "${seconds / 3_600L}h ago"
        seconds < 30L * 86_400L -> "${seconds / 86_400L}d ago"
        seconds < 365L * 86_400L -> "${seconds / (30L * 86_400L)}mo ago"
        else -> "${seconds / (365L * 86_400L)}y ago"
    }
}
