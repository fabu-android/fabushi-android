package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
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
    val clientNonce: String?,
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
            clientNonce = entry.optString("clientNonce").takeIf(String::isNotBlank),
            timestampMs = entry.optLong("timestampMs", 0L),
        )
        byIdentity["$agentId\u0000$id"] = message
    }
    return byIdentity.values.toList()
}

internal fun canonicalMobileTranscriptForAgent(
    entries: JSONArray,
    agentId: String,
): List<MobileChatMessage> {
    val projected = linkedMapOf<String, MobileChatMessage>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("agentId").trim() != agentId) continue
        val id = entry.optString("id").trim()
        if (id.isBlank()) continue
        when (entry.optString("kind")) {
            "message" -> {
                val role = when (entry.optString("role")) {
                    "user" -> MobileChatRole.USER
                    "assistant" -> MobileChatRole.ASSISTANT
                    else -> continue
                }
                projected[id] = MobileChatMessage(
                    id = id,
                    role = role,
                    text = entry.optString("content"),
                    operationId = entry.optString("operationId").takeIf(String::isNotBlank),
                    clientNonce = entry.optString("clientNonce").takeIf(String::isNotBlank),
                )
            }
            "send-message" -> {
                val message = entry.optJSONObject("message") ?: continue
                if (message.optString("type") != "widget") continue
                val widget = message.optJSONObject("widget") ?: continue
                val prompt = widget.optString("prompt").trim()
                val optionRows = widget.optJSONArray("options") ?: continue
                if (prompt.isBlank() || optionRows.length() !in 1..6) continue
                val options = buildList {
                    for (optionIndex in 0 until optionRows.length()) {
                        val option = optionRows.optJSONObject(optionIndex) ?: return@buildList
                        val label = option.optString("label").trim()
                        if (label.isBlank()) return@buildList
                        add(
                            MobileWidgetOption(
                                label = label,
                                value = option.optString("value").trim().ifBlank { label },
                                description = option.optString("description").trim().takeIf(String::isNotEmpty),
                            ),
                        )
                    }
                }
                if (options.size != optionRows.length()) continue
                projected[id] = MobileChatMessage(
                    id = id,
                    role = MobileChatRole.ASSISTANT,
                    text = prompt,
                    kind = MobileChatEntryKind.WIDGET,
                    widget = MobileWidgetCard(
                        prompt = prompt,
                        helpText = widget.optString("helpText").trim().takeIf(String::isNotEmpty),
                        options = options,
                        allowCustom = widget.optBoolean("allowCustom", false),
                        dismissOnMoveOn = widget.optBoolean("dismissOnMoveOn", false),
                        respondedValue = entry.optString("respondedValue").takeIf(String::isNotBlank),
                        dismissed = entry.optBoolean("widgetDismissed", false),
                    ),
                )
            }
        }
    }
    return projected.values.toList()
}

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


internal fun projectDurableSendAcknowledgements(
    canonical: List<MobileChatMessage>,
    acknowledgements: List<AndroidSendAcknowledgement>,
): List<MobileChatMessage> {
    val byId = linkedMapOf<String, MobileChatMessage>()
    canonical.forEach { byId[it.id] = it }
    acknowledgements.forEach { acknowledgement ->
        val messageEntry = acknowledgement.entries.firstOrNull { it.kind == "message" } ?: return@forEach
        if (byId.containsKey(messageEntry.id)) return@forEach
        byId[messageEntry.id] = MobileChatMessage(
            id = messageEntry.id,
            role = MobileChatRole.USER,
            text = acknowledgement.messageText,
            clientNonce = acknowledgement.nonce,
            sendStatus = acknowledgement.phase.wireName,
            sendAcknowledgementGeneration = acknowledgement.generation,
            sendFailureReason = acknowledgement.reason,
        )
    }
    return byId.values.toList()
}

internal fun isExplicitSendRetryAllowed(message: MobileChatMessage): Boolean =
    message.sendStatus == AndroidSendAcknowledgementPhase.FAILED.wireName &&
        !message.clientNonce.isNullOrBlank() &&
        (message.sendAcknowledgementGeneration ?: 0L) > 0L
