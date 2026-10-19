package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidFindInChatEntry

internal data class FindInChatMatch(
    val messageId: String,
    val occurrence: Int,
)

/**
 * Projects one user-visible searchable payload per transcript row.
 *
 * This mirrors Grok's find-in-chat contract: structured rows expose their primary visible payload
 * (for example a widget prompt) rather than concatenating every rendered metadata field. That
 * prevents one semantic value from becoming multiple matches merely because it is repeated in
 * auxiliary UI such as poll options or forwarding labels.
 */
internal fun findInChatSearchText(message: ChatMessage): String =
    when (message.contentType) {
        "contact" -> message.contactName ?: message.text
        "poll" -> message.pollQuestion ?: message.text
        "voice", "audio", "photo", "video", "document" ->
            message.mediaFileName ?: message.text
        else -> message.text
    }.replace(Regex("\\s+"), " ").trim()

internal fun findInChatMatches(
    messages: List<ChatMessage>,
    query: String,
): List<FindInChatMatch> {
    val needle = query.trim().lowercase()
    if (needle.isEmpty()) return emptyList()
    return buildList {
        messages.forEach { message ->
            val text = findInChatSearchText(message).lowercase()
            var from = 0
            var occurrence = 0
            while (from <= text.length - needle.length) {
                val at = text.indexOf(needle, from)
                if (at < 0) break
                add(FindInChatMatch(message.id, occurrence++))
                from = at + needle.length.coerceAtLeast(1)
            }
        }
    }
}

internal fun stepFindInChatIndex(current: Int, delta: Int, count: Int): Int {
    if (count <= 0) return -1
    val base = current.takeIf { it in 0 until count } ?: 0
    return ((base + delta) % count + count) % count
}


/**
 * Agent-workspace search projection.
 *
 * Desktop searches the semantic transcript payload, not arbitrary rendered chrome. The current
 * Android Agent projection can faithfully expose message text and widget prompt. Action/tool rows
 * stay excluded until their source entry kinds become searchable in Desktop as well.
 */
internal fun findInAgentChatSearchText(message: MobileChatMessage): String =
    when (message.kind) {
        MobileChatEntryKind.MESSAGE -> message.text
        MobileChatEntryKind.WIDGET -> message.widget?.prompt ?: message.text
        else -> ""
    }

internal fun findInAgentChatMatches(
    messages: List<MobileChatMessage>,
    query: String,
): List<FindInChatMatch> {
    if (query.trim().isEmpty()) return emptyList()
    val needle = query.lowercase()
    return buildList {
        messages.forEach { message ->
            val text = findInAgentChatSearchText(message).lowercase()
            var at = text.indexOf(needle)
            var occurrence = 0
            while (at >= 0) {
                add(FindInChatMatch(message.id, occurrence))
                occurrence += 1
                at = text.indexOf(needle, at + needle.length.coerceAtLeast(1))
            }
        }
    }
}

/**
 * Mirrors Desktop controller stepping from a null current selection: next enters the first match;
 * previous uses the same wrap formula from the virtual last position.
 */
internal fun stepAgentFindInChatIndex(current: Int, delta: Int, count: Int): Int {
    if (count <= 0) return -1
    val base = current.takeIf { it in 0 until count } ?: (count - 1)
    return ((base + delta) % count + count) % count
}


internal fun findInCanonicalAgentChatMatches(
    entries: List<AndroidFindInChatEntry>,
    query: String,
): List<FindInChatMatch> {
    if (query.trim().isEmpty()) return emptyList()
    val needle = query.lowercase()
    return buildList {
        entries.forEach { entry ->
            val text = entry.searchText.lowercase()
            var at = text.indexOf(needle)
            var occurrence = 0
            while (at >= 0) {
                add(FindInChatMatch(entry.entryId, occurrence))
                occurrence += 1
                at = text.indexOf(needle, at + needle.length.coerceAtLeast(1))
            }
        }
    }
}
