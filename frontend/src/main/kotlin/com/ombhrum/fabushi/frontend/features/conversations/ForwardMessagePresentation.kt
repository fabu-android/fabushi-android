package com.ombhrum.fabushi

import org.json.JSONObject

data class ForwardMessageBatchRequest(
    val sourceConversationId: String,
    val messageId: String,
    val destinationConversationIds: List<String>,
    val batchId: String,
    val dropSenderNames: Boolean = false,
    val dropCaptions: Boolean = false,
)

enum class ForwardSettlementStatus { SENT, FAILED }

data class ForwardSettlement(
    val conversationId: String,
    val status: ForwardSettlementStatus,
    val error: String? = null,
)

internal fun normalizeForwardDestinations(destinationConversationIds: List<String>): List<String> =
    destinationConversationIds.map(String::trim).filter(String::isNotEmpty).distinct()

internal fun forwardClientMessageId(batchId: String, destinationConversationId: String): String {
    val batch = batchId.trim()
    val destination = destinationConversationId.trim()
    require(batch.isNotEmpty()) { "forward batch id must not be empty" }
    require(destination.isNotEmpty()) { "forward destination must not be empty" }
    return "android:forward:$batch:$destination"
}

internal fun buildForwardCommand(
    request: ForwardMessageBatchRequest,
    destinationConversationId: String,
): JSONObject {
    val sourceConversationId = request.sourceConversationId.trim()
    val messageId = request.messageId.trim()
    val batchId = request.batchId.trim()
    val destination = destinationConversationId.trim()
    require(sourceConversationId.isNotEmpty()) { "forward source conversation must not be empty" }
    require(messageId.isNotEmpty()) { "forward message id must not be empty" }
    require(batchId.isNotEmpty()) { "forward batch id must not be empty" }
    require(destination.isNotEmpty()) { "forward destination must not be empty" }
    val dropCaptions = request.dropCaptions
    val dropSenderNames = request.dropSenderNames || dropCaptions
    return JSONObject()
        .put("type", "forwardMessage")
        .put("sourceConversationId", sourceConversationId)
        .put("messageId", messageId)
        .put("destinationConversationId", destination)
        .put("clientMessageId", forwardClientMessageId(batchId, destination))
        .put("privacy", JSONObject().put("dropSenderNames", dropSenderNames).put("dropCaptions", dropCaptions))
}

internal enum class ForwardRecipientNavigationKey {
    ARROW_DOWN, ARROW_UP, PAGE_DOWN, PAGE_UP, HOME, END,
}

internal fun nextForwardRecipientIndex(
    currentIndex: Int,
    recipientCount: Int,
    key: ForwardRecipientNavigationKey,
    pageSize: Int = 6,
): Int? {
    if (recipientCount <= 0) return null
    val last = recipientCount - 1
    val current = currentIndex.coerceIn(0, last)
    val page = pageSize.coerceAtLeast(1)
    return when (key) {
        ForwardRecipientNavigationKey.ARROW_DOWN -> (current + 1).coerceAtMost(last)
        ForwardRecipientNavigationKey.ARROW_UP -> (current - 1).coerceAtLeast(0)
        ForwardRecipientNavigationKey.PAGE_DOWN -> (current + page).coerceAtMost(last)
        ForwardRecipientNavigationKey.PAGE_UP -> (current - page).coerceAtLeast(0)
        ForwardRecipientNavigationKey.HOME -> 0
        ForwardRecipientNavigationKey.END -> last
    }
}

internal fun isForwardSubmitShortcut(key: String, ctrlKey: Boolean, metaKey: Boolean): Boolean =
    key == "Enter" && (ctrlKey || metaKey)

internal fun isForwardToggleShortcut(key: String, ctrlKey: Boolean, metaKey: Boolean): Boolean =
    (key == "Enter" && !ctrlKey && !metaKey) || key == " "
