package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidClientSideToolV2Event
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
import com.ombhrum.fabushi.androidpreload.runtime.AndroidToolResultCard
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
    val attachments: List<MobileTranscriptAttachment>,
)

private fun canonicalTranscriptIsHiddenAgentPeer(entry: org.json.JSONObject): Boolean =
    entry.optBoolean("hiddenOutboundAgentPeerMessage", false) ||
        entry.optJSONObject("toAgent")
            ?.optString("kind")
            ?.trim()
            ?.equals("agent", ignoreCase = true) == true

internal fun canonicalAgentTranscriptMessages(
    entries: JSONArray,
): List<CanonicalAgentTranscriptMessage> {
    val byIdentity = linkedMapOf<String, CanonicalAgentTranscriptMessage>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("kind") != "message") continue
        if (canonicalTranscriptIsHiddenAgentPeer(entry)) continue
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
            attachments = canonicalTranscriptAttachments(entry),
        )
        byIdentity["$agentId\u0000$id"] = message
    }
    return byIdentity.values.toList()
}


private fun canonicalTranscriptPresentationKind(entry: org.json.JSONObject): String? {
    val direct = entry.optString("presentationKind").trim().takeIf(String::isNotEmpty)
    if (direct != null) return direct
    return entry
        .optJSONObject("sendMessageText")
        ?.optJSONObject("presentation")
        ?.optString("kind")
        ?.trim()
        ?.takeIf(String::isNotEmpty)
}

private fun canonicalTranscriptHasReaction(entry: org.json.JSONObject): Boolean =
    (entry.optJSONArray("reactions")?.length() ?: 0) > 0

private fun canonicalTranscriptAttachments(entry: org.json.JSONObject): List<MobileTranscriptAttachment> {
    val rows = entry.optJSONArray("attachments") ?: return emptyList()
    return buildList {
        for (index in 0 until rows.length()) {
            val attachment = rows.optJSONObject(index) ?: continue
            val path = attachment.optString("path").trim()
            val name = attachment.optString("name").trim()
            if (path.isBlank() || name.isBlank()) continue
            val sizeBytes = if (attachment.has("size") && !attachment.isNull("size")) {
                attachment.optLong("size", -1L).takeIf { it >= 0L }
            } else {
                null
            }
            add(
                MobileTranscriptAttachment(
                    path = path,
                    name = name,
                    sizeBytes = sizeBytes,
                    mimeType = attachment.optString("mimeType").trim().takeIf(String::isNotEmpty),
                ),
            )
        }
    }
}

internal fun canonicalMobileTranscriptForAgent(
    entries: JSONArray,
    agentId: String,
): List<MobileChatMessage> {
    val projected = linkedMapOf<String, MobileChatMessage>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("agentId").trim() != agentId) continue
        if (canonicalTranscriptIsHiddenAgentPeer(entry)) continue
        val id = entry.optString("id").trim()
        if (id.isBlank()) continue
        when (entry.optString("kind")) {
            "message" -> {
                val role = when (entry.optString("role")) {
                    "user" -> MobileChatRole.USER
                    "assistant" -> MobileChatRole.ASSISTANT
                    else -> continue
                }
                val assistantPresentation = if (role == MobileChatRole.ASSISTANT) {
                    projectCanonicalAssistantPresentation(entry)
                } else {
                    TranscriptAssistantPresentation()
                }
                projected[id] = MobileChatMessage(
                    id = id,
                    role = role,
                    text = entry.optString("content"),
                    operationId = entry.optString("operationId").takeIf(String::isNotBlank),
                    clientNonce = entry.optString("clientNonce").takeIf(String::isNotBlank),
                    attachments = canonicalTranscriptAttachments(entry),
                    assistantImages = assistantPresentation.images,
                    assistantChannel = assistantPresentation.channel,
                    assistantSourceTrusted = assistantPresentation.sourceTrusted,
                    urlCardUrl = assistantPresentation.urlCardUrl,
                    richTextJson = entry.optJSONObject("richText")?.toString(),
                    transcriptAuthor = entry.optString("author").trim().takeIf(String::isNotEmpty),
                    transcriptSourceKind = entry.optString("sourceKind").trim().takeIf(String::isNotEmpty),
                    transcriptPresentationKind = canonicalTranscriptPresentationKind(entry),
                    hasTranscriptReaction = canonicalTranscriptHasReaction(entry),
                    hasTranscriptThreadChip = entry.optBoolean("hasThreadChip", false),
                )
            }
            "tool-call" -> {
                val name = entry.optString("name").trim()
                val status = entry.optString("status").trim()
                if (name.isBlank() || status !in setOf("pending", "running", "done", "failed", "error", "aborted")) {
                    continue
                }
                projected[id] = MobileChatMessage(
                    id = id,
                    role = MobileChatRole.ASSISTANT,
                    text = "",
                    kind = MobileChatEntryKind.ACTION,
                    actionTitle = name,
                    actionDetail = entry.optString("summary").trim().takeIf(String::isNotEmpty),
                    actionStatus = status,
                    toolCallId = id,
                )
            }
            "send-message" -> {
                val message = entry.optJSONObject("message") ?: continue
                if (message.optString("type") == "text") {
                    val text = projectTranscriptSendMessageText(entry) ?: continue
                    projected[id] = MobileChatMessage(
                        id = id,
                        role = MobileChatRole.ASSISTANT,
                        text = text.content,
                        streaming = text.streaming,
                        assistantImages = text.images,
                        assistantChannel = text.channel,
                        assistantSourceTrusted = text.sourceTrusted,
                        urlCardUrl = text.urlCardUrl,
                        transcriptAuthor = entry.optString("author").trim().takeIf(String::isNotEmpty),
                        transcriptSourceKind = "send-message",
                        transcriptPresentationKind = if (text.urlCardUrl == null) "text" else "url-card",
                        hasTranscriptReaction = canonicalTranscriptHasReaction(entry),
                        hasTranscriptThreadChip = entry.optBoolean("hasThreadChip", false),
                    )
                    continue
                }
                if (message.optString("type") != "widget") continue
                val widget = message.optJSONObject("widget") ?: continue
                val prompt = widget.optString("prompt").trim()
                val optionRows = widget.optJSONArray("options") ?: continue
                if (prompt.isBlank()) continue
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
                    hasTranscriptReaction = canonicalTranscriptHasReaction(entry),
                    hasTranscriptThreadChip = entry.optBoolean("hasThreadChip", false),
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

internal fun prependCanonicalMobileTranscriptPage(
    older: List<MobileChatMessage>,
    current: List<MobileChatMessage>,
): List<MobileChatMessage> =
    (older + current).distinctBy(MobileChatMessage::id)



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
            richTextJson = acknowledgement.messageRichText?.toString(),
            attachments = acknowledgement.attachments.map { attachment ->
                MobileTranscriptAttachment(
                    path = attachment.path,
                    name = attachment.name,
                    sizeBytes = attachment.sizeBytes,
                    mimeType = attachment.mimeType,
                )
            },
            sendStatus = acknowledgement.phase.wireName,
            sendAcknowledgementGeneration = acknowledgement.generation,
            sendFailureReason = acknowledgement.reason,
            transcriptAuthor = "user",
        )
    }
    return byId.values.toList()
}

internal fun isExplicitSendRetryAllowed(message: MobileChatMessage): Boolean =
    message.sendStatus == AndroidSendAcknowledgementPhase.FAILED.wireName &&
        !message.clientNonce.isNullOrBlank() &&
        (message.sendAcknowledgementGeneration ?: 0L) > 0L

internal fun isQueuedSendCancellationAllowed(message: MobileChatMessage): Boolean =
    message.sendStatus == AndroidSendAcknowledgementPhase.QUEUED.wireName &&
        !message.clientNonce.isNullOrBlank() &&
        (message.sendAcknowledgementGeneration ?: 0L) > 0L


internal fun joinClientSideToolResultCards(
    canonical: List<MobileChatMessage>,
    events: List<AndroidClientSideToolV2Event>,
    agentId: String,
): List<MobileChatMessage> {
    val byToolCallId = linkedMapOf<String, MobileToolResultCard>()
    events
        .asSequence()
        .filter { it.accountSlot == "host" && it.agentId == agentId }
        .sortedBy { it.sequence }
        .forEach { event ->
            val projection = event.projection ?: return@forEach
            byToolCallId[projection.toolCallId] = projection.toMobileToolResultCard()
        }
    return canonical.map { row ->
        val id = row.toolCallId ?: return@map row
        val card = byToolCallId[id] ?: return@map row
        row.copy(
            kind = MobileChatEntryKind.TOOL_RESULT,
            actionTitle = when (card.kind) {
                "file-edit" -> "Edited file"
                "file-write" -> "Wrote file"
                "shell" -> "Terminal"
                else -> row.actionTitle ?: "Tool"
            },
            actionDetail = card.path ?: card.command ?: card.summary.ifBlank { row.actionDetail.orEmpty() },
            actionStatus = card.status,
            toolResult = card,
        )
    }
}

private fun AndroidToolResultCard.toMobileToolResultCard(): MobileToolResultCard =
    MobileToolResultCard(
        kind = kind,
        toolCallId = toolCallId,
        status = status,
        path = path,
        command = command,
        workingDirectory = workingDirectory,
        summary = summary,
        output = output.replace("\r\n", "\n").replace('\r', '\n'),
        diff = diff.replace("\r\n", "\n").replace('\r', '\n'),
        isStreaming = isStreaming,
        isBackground = isBackground,
    )
