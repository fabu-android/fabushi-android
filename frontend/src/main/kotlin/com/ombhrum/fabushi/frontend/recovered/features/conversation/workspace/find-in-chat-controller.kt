package com.ombhrum.fabushi

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
