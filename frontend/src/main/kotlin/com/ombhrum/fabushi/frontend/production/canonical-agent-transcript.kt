package com.ombhrum.fabushi

import org.json.JSONArray

/**
 * Read-only projection of the Host-owned canonical transcript.
 *
 * Every Agent message must carry explicit agentId identity. Presentation never guesses an owner
 * for legacy or malformed rows because doing so can leak one conversation into another.
 */
internal data class CanonicalAgentTranscriptMessage(
    val id: String,
    val agentId: String,
    val role: MobileChatRole,
    val text: String,
    val operationId: String?,
    val timestampMs: Long,
)

internal fun canonicalAgentTranscriptMessages(
    entries: JSONArray,
): List<CanonicalAgentTranscriptMessage> {
    val byIdentity = linkedMapOf<String, CanonicalAgentTranscriptMessage>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("kind") != "message") continue
        val id = entry.optString("id").trim()
        val agentId = entry.optString("agentId").trim()
        val role = when (entry.optString("role")) {
            "user" -> MobileChatRole.USER
            "assistant" -> MobileChatRole.ASSISTANT
            else -> continue
        }
        if (id.isBlank() || agentId.isBlank()) continue
        val message = CanonicalAgentTranscriptMessage(
            id = id,
            agentId = agentId,
            role = role,
            text = entry.optString("content"),
            operationId = entry.optString("operationId").takeIf(String::isNotBlank),
            timestampMs = entry.optLong("timestampMs", 0L),
        )
        byIdentity["$agentId\u0000$id"] = message
    }
    return byIdentity.values.toList()
}

internal fun canonicalMobileTranscriptForAgent(
    entries: JSONArray,
    agentId: String,
): List<MobileChatMessage> =
    canonicalAgentTranscriptMessages(entries)
        .asSequence()
        .filter { it.agentId == agentId }
        .map { entry ->
            MobileChatMessage(
                id = entry.id,
                role = entry.role,
                text = entry.text,
                operationId = entry.operationId,
            )
        }
        .toList()

/**
 * A transcript snapshot is authoritative for everything that existed when loading began.
 * Preserve only entries that arrived in presentation after that baseline so a late snapshot
 * cannot erase a newer live event.
 */
internal fun mergeCanonicalMobileTranscript(
    baselineEntryIds: Set<String>,
    current: List<MobileChatMessage>,
    canonical: List<MobileChatMessage>,
): List<MobileChatMessage> {
    val canonicalIds = canonical.mapTo(linkedSetOf()) { it.id }
    val arrivedDuringLoad = current.filter { it.id !in baselineEntryIds && it.id !in canonicalIds }
    return (canonical + arrivedDuringLoad).distinctBy(MobileChatMessage::id)
}
