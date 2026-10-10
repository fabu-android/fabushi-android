package com.ombhrum.fabushi

import android.app.Application
import android.util.Base64
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

internal fun normalizeForwardDestinations(destinationConversationIds: List<String>): List<String> =
    destinationConversationIds.map(String::trim).filter(String::isNotEmpty).distinct()

internal fun forwardClientMessageId(batchId: String, destinationConversationId: String): String {
    val batch = batchId.trim()
    val destination = destinationConversationId.trim()
    require(batch.isNotEmpty()) { "forward batch id must not be empty" }
    require(destination.isNotEmpty()) { "forward destination must not be empty" }
    return "android:forward:$batch:$destination"
}

enum class ConversationKind(val wire: String, val label: String) {
    DIRECT("direct", "私聊"), GROUP("group", "群组"), CHANNEL("channel", "频道"),
    SAVED_MESSAGES("savedMessages", "收藏"), SECRET("secret", "加密聊天"),
}

internal fun conversationKindFromWire(wire: String): ConversationKind? =
    ConversationKind.entries.firstOrNull { it.wire == wire }

data class ConversationParticipant(val actorId: String, val role: String, val joinedAtMs: Long)

data class ConversationSummary(
    val id: String,
    val title: String,
    val description: String,
    val ownerId: String? = null,
    val participants: List<ConversationParticipant> = emptyList(),
    val preview: String,
    val time: String,
    val badge: String,
    val kind: ConversationKind,
    val unreadCount: Int = 0,
    val isPinned: Boolean = false,
    val isMuted: Boolean = false,
    val isArchived: Boolean = false,
    val lastMessageId: String? = null,
    val pinnedMessageIds: List<String> = emptyList(),
    val folderIds: List<String> = emptyList(),
    val markedUnread: Boolean = false,
)

data class MessagingDraft(val conversationId: String, val text: String, val replyToMessageId: String? = null, val updatedAtMs: Long = 0L)

data class MessagingFolder(
    val id: String, val title: String, val icon: String? = null, val conversationIds: List<String> = emptyList(),
    val includeContacts: Boolean = false, val includeBots: Boolean = false, val includeGroups: Boolean = false, val includeChannels: Boolean = false,
    val excludeMuted: Boolean = false, val excludeRead: Boolean = false, val excludeArchived: Boolean = true,
)

data class MessagingContact(
    val id: String,
    val displayName: String,
    val username: String?,
    val kind: String,
)

data class ChatReaction(val reaction: String, val count: Int, val chosenByMe: Boolean)

data class ChatPollOption(val id: String, val text: String, val voterCount: Int, val chosen: Boolean)

data class ChatMessage(
    val id: String,
    val conversationId: String,
    val text: String,
    val contentType: String = "text",
    val mediaFileName: String? = null,
    val mediaBlobId: String? = null,
    val mediaMimeType: String? = null,
    val mediaSizeBytes: Int = 0,
    val contactName: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
    val pollQuestion: String? = null,
    val pollOptions: List<ChatPollOption> = emptyList(),
    val pollMultipleAnswers: Boolean = false,
    val outgoing: Boolean,
    val time: String,
    val replyToMessageId: String? = null,
    val forwardOrigin: String? = null,
    val reactions: List<ChatReaction> = emptyList(),
    val deliveryState: String = "sent",
    val edited: Boolean = false,
    val pinned: Boolean = false,
)

data class MessagingUiState(
    val loading: Boolean = false,
    val conversations: List<ConversationSummary> = emptyList(),
    val contacts: List<MessagingContact> = emptyList(),
    val folders: List<MessagingFolder> = emptyList(),
    val draftsByConversation: Map<String, MessagingDraft> = emptyMap(),
    val messagesByConversation: Map<String, List<ChatMessage>> = emptyMap(),
    val typingActorByConversation: Map<String, String> = emptyMap(),
    val error: String? = null,
)

internal class MessagingViewModel(application: Application) : AndroidViewModel(application) {
    private val coordinator: AndroidCoordinatorPort = CoordinatorClient.presentation()
    private val mutableState = MutableStateFlow(MessagingUiState())
    val state: StateFlow<MessagingUiState> = mutableState.asStateFlow()
    private var actorId = ""
    private var displayName = "当前用户"
    private val deviceId = "android:native"
    private val sessionId = "account-session:android-native"
    val currentActorId: String get() = actorId

    init { refresh() }

    fun refresh() {
        mutableState.value = mutableState.value.copy(loading = true)
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { ensureIdentity(); execute(JSONObject().put("type", "sync").put("cursor", JSONObject.NULL).put("limit", 1000)) } }
                .onSuccess { mutableState.value = mutableState.value.copy(loading = false, error = null) }
                .onFailure { mutableState.value = mutableState.value.copy(loading = false, error = it.message) }
        }
    }

    fun createDirect(contact: MessagingContact) {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                val now = System.currentTimeMillis()
                val id = "direct:${UUID.randomUUID()}"
                val participants = JSONArray()
                    .put(JSONObject().put("actorId", actorId).put("role", "owner").put("joinedAtMs", now).put("mutedUntilMs", JSONObject.NULL))
                    .put(JSONObject().put("actorId", contact.id).put("role", "member").put("joinedAtMs", now).put("mutedUntilMs", JSONObject.NULL))
                val conversation = baseConversation(id, ConversationKind.DIRECT, contact.displayName, "", participants, now)
                execute(JSONObject().put("type", "createConversation").put("conversation", conversation))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun createConversation(kind: ConversationKind, title: String, description: String = "", participantActorIds: List<String> = emptyList()) {
        if (kind != ConversationKind.GROUP && kind != ConversationKind.CHANNEL) return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                val now = System.currentTimeMillis()
                val id = "${kind.wire}:${UUID.randomUUID()}"
                val participants = JSONArray().put(JSONObject().put("actorId", actorId).put("role", "owner").put("joinedAtMs", now).put("mutedUntilMs", JSONObject.NULL))
                participantActorIds.filter { it.isNotBlank() && it != actorId }.distinct().forEach { participantId ->
                    participants.put(JSONObject().put("actorId", participantId).put("role", "member").put("joinedAtMs", now).put("mutedUntilMs", JSONObject.NULL))
                }
                val conversation = baseConversation(id, kind, title.trim(), description.trim(), participants, now)
                execute(JSONObject().put("type", "createConversation").put("conversation", conversation))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun sendText(conversationId: String, text: String, replyToMessageId: String? = null, silent: Boolean = false, scheduledAtMs: Long? = null) {
        val value = text.trim(); if (value.isEmpty()) return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                execute(JSONObject().put("type", "sendMessage").put("conversationId", conversationId)
                    .put("clientMessageId", "android:${UUID.randomUUID()}")
                    .put("content", JSONObject().put("type", "text").put("data", JSONObject().put("text", JSONObject().put("text", value).put("entities", JSONArray()))))
                    .put("replyToMessageId", replyToMessageId ?: JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", scheduledAtMs ?: JSONObject.NULL)
                    .put("silent", silent).put("protectedContent", false))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun sendContact(conversationId: String, contact: MessagingContact) {
        executeAsync(JSONObject().put("type", "sendMessage").put("conversationId", conversationId).put("clientMessageId", "android:${UUID.randomUUID()}")
            .put("content", JSONObject().put("type", "contact").put("data", JSONObject().put("actorId", contact.id).put("displayName", contact.displayName).put("phoneNumber", JSONObject.NULL)))
            .put("replyToMessageId", JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", JSONObject.NULL).put("silent", false).put("protectedContent", false))
    }
    fun sendLocation(conversationId: String, latitude: Double, longitude: Double, liveUntilMs: Long? = null) {
        executeAsync(JSONObject().put("type", "sendMessage").put("conversationId", conversationId).put("clientMessageId", "android:${UUID.randomUUID()}")
            .put("content", JSONObject().put("type", "location").put("data", JSONObject().put("latitude", latitude).put("longitude", longitude).put("liveUntilMs", liveUntilMs ?: JSONObject.NULL)))
            .put("replyToMessageId", JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", JSONObject.NULL).put("silent", false).put("protectedContent", false))
    }
    fun votePoll(conversationId: String, messageId: String, optionIds: List<String>) {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                execute(JSONObject().put("type", "votePoll").put("conversationId", conversationId).put("messageId", messageId).put("optionIds", JSONArray(optionIds)))
                execute(JSONObject().put("type", "sync").put("cursor", JSONObject.NULL).put("limit", 1000))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun sendPoll(conversationId: String, question: String, options: List<String>, multipleAnswers: Boolean = false) {
        val cleanOptions = options.map { it.trim() }.filter { it.isNotEmpty() }; if (question.isBlank() || cleanOptions.size < 2) return
        val pollOptions = JSONArray(); cleanOptions.forEachIndexed { index, option -> pollOptions.put(JSONObject().put("id", "option-${index + 1}").put("text", option).put("voterCount", 0).put("chosen", false).put("correct", JSONObject.NULL)) }
        executeAsync(JSONObject().put("type", "sendMessage").put("conversationId", conversationId).put("clientMessageId", "android:${UUID.randomUUID()}")
            .put("content", JSONObject().put("type", "poll").put("data", JSONObject().put("question", JSONObject().put("text", question.trim()).put("entities", JSONArray())).put("options", pollOptions).put("anonymous", true).put("multipleAnswers", multipleAnswers).put("quiz", false)))
            .put("replyToMessageId", JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", JSONObject.NULL).put("silent", false).put("protectedContent", false))
    }

    fun loadBlob(blobId: String, sizeBytes: Int, onResult: (Result<ByteArray>) -> Unit) {
        viewModelScope.launch {
            val result = runCatching { withContext(Dispatchers.IO) {
                if (sizeBytes <= 0) return@withContext ByteArray(0)
                val output = java.io.ByteArrayOutputStream(sizeBytes)
                var offset = 0
                val chunkSize = 1024 * 1024
                while (offset < sizeBytes) {
                    val requested = minOf(chunkSize, sizeBytes - offset)
                    val response = coordinator.messagingBlobRead( JSONObject().put("blobId", blobId).put("offset", offset).put("length", requested))
                    val encoded = response.optString("dataBase64")
                    val chunk = Base64.decode(encoded, Base64.DEFAULT)
                    check(chunk.isNotEmpty()) { "Blob read returned empty data" }
                    output.write(chunk); offset += chunk.size
                }
                output.toByteArray()
            }}
            onResult(result)
        }
    }

    fun sendVoice(conversationId: String, fileName: String, mimeType: String, bytes: ByteArray, waveform: List<Int> = emptyList()) {
        if (bytes.isEmpty()) return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                val blobId = "voice-${UUID.randomUUID()}"
                val now = System.currentTimeMillis()
                execute(JSONObject().put("type", "beginBlobUpload").put("metadata", JSONObject().put("id", blobId).put("fileName", fileName).put("mimeType", mimeType).put("sizeBytes", bytes.size).put("contentHash", JSONObject.NULL).put("createdAtMs", now)))
                val chunkSize = 512 * 1024; var offset = 0
                while (offset < bytes.size) { val end = minOf(bytes.size, offset + chunkSize); execute(JSONObject().put("type", "appendBlobChunk").put("blobId", blobId).put("offset", offset).put("dataBase64", Base64.encodeToString(bytes.copyOfRange(offset, end), Base64.NO_WRAP))); offset = end }
                execute(JSONObject().put("type", "finishBlobUpload").put("blobId", blobId))
                val media = JSONObject().put("id", blobId).put("fileName", fileName).put("mimeType", mimeType).put("sizeBytes", bytes.size).put("remoteUrl", "fabushi-blob://$blobId")
                val waveformJson = JSONArray(); waveform.forEach { waveformJson.put(it.coerceIn(0, 255)) }
                val content = JSONObject().put("type", "voice").put("data", JSONObject().put("media", media).put("caption", JSONObject().put("text", "").put("entities", JSONArray())).put("waveform", waveformJson))
                execute(JSONObject().put("type", "sendMessage").put("conversationId", conversationId).put("clientMessageId", "android:${UUID.randomUUID()}").put("content", content)
                    .put("replyToMessageId", JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", JSONObject.NULL).put("silent", false).put("protectedContent", false))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun sendAttachment(conversationId: String, fileName: String, mimeType: String, bytes: ByteArray) {
        if (bytes.isEmpty()) return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) {
                ensureIdentity()
                val blobId = "blob-${UUID.randomUUID()}"
                val now = System.currentTimeMillis()
                execute(JSONObject().put("type", "beginBlobUpload").put("metadata", JSONObject().put("id", blobId).put("fileName", fileName).put("mimeType", mimeType).put("sizeBytes", bytes.size).put("contentHash", JSONObject.NULL).put("createdAtMs", now)))
                val chunkSize = 512 * 1024
                var offset = 0
                while (offset < bytes.size) {
                    val end = minOf(bytes.size, offset + chunkSize)
                    val chunk = bytes.copyOfRange(offset, end)
                    execute(JSONObject().put("type", "appendBlobChunk").put("blobId", blobId).put("offset", offset).put("dataBase64", Base64.encodeToString(chunk, Base64.NO_WRAP)))
                    offset = end
                }
                execute(JSONObject().put("type", "finishBlobUpload").put("blobId", blobId))
                val media = JSONObject().put("id", blobId).put("fileName", fileName).put("mimeType", mimeType).put("sizeBytes", bytes.size).put("remoteUrl", "fabushi-blob://$blobId")
                val caption = JSONObject().put("text", "").put("entities", JSONArray())
                val content = when {
                    mimeType.startsWith("image/") -> JSONObject().put("type", "photo").put("data", JSONObject().put("media", media).put("caption", caption).put("spoiler", false))
                    mimeType.startsWith("video/") -> JSONObject().put("type", "video").put("data", JSONObject().put("media", media).put("caption", caption).put("spoiler", false).put("streaming", true))
                    else -> JSONObject().put("type", "document").put("data", JSONObject().put("media", media).put("caption", caption))
                }
                execute(JSONObject().put("type", "sendMessage").put("conversationId", conversationId).put("clientMessageId", "android:${UUID.randomUUID()}").put("content", content)
                    .put("replyToMessageId", JSONObject.NULL).put("threadRootMessageId", JSONObject.NULL).put("scheduledAtMs", JSONObject.NULL).put("silent", false).put("protectedContent", false))
            }}.onFailure { mutableState.value = mutableState.value.copy(error = it.message) }
        }
    }

    fun setMessagePinned(conversationId: String, messageId: String, pinned: Boolean) =
        executeAsync(JSONObject().put("type", "pinMessage").put("conversationId", conversationId).put("messageId", messageId).put("pinned", pinned))
    fun upsertFolder(folder: MessagingFolder) = executeAsync(JSONObject().put("type", "upsertFolder").put("folder", JSONObject()
        .put("id", folder.id).put("title", folder.title).put("icon", folder.icon ?: JSONObject.NULL).put("conversationIds", JSONArray(folder.conversationIds))
        .put("includeContacts", folder.includeContacts).put("includeBots", folder.includeBots).put("includeGroups", folder.includeGroups).put("includeChannels", folder.includeChannels)
        .put("excludeMuted", folder.excludeMuted).put("excludeRead", folder.excludeRead).put("excludeArchived", folder.excludeArchived)))
    fun deleteFolder(folderId: String) = executeAsync(JSONObject().put("type", "deleteFolder").put("folderId", folderId))

    fun updateConversationInfo(conversationId: String, title: String, description: String) = executeAsync(JSONObject().put("type", "updateConversationInfo").put("conversationId", conversationId).put("title", title.trim()).put("description", description.trim().ifBlank { JSONObject.NULL }))
    fun setConversationParticipant(conversation: ConversationSummary, targetActorId: String, role: String) {
        val joinedAt = conversation.participants.firstOrNull { it.actorId == targetActorId }?.joinedAtMs ?: System.currentTimeMillis()
        executeAsync(JSONObject().put("type", "setConversationParticipant").put("conversationId", conversation.id).put("participant", JSONObject().put("actorId", targetActorId).put("role", role).put("joinedAtMs", joinedAt).put("mutedUntilMs", JSONObject.NULL)))
    }
    fun removeConversationParticipant(conversationId: String, targetActorId: String) = executeAsync(JSONObject().put("type", "removeConversationParticipant").put("conversationId", conversationId).put("actorId", targetActorId))

    fun setMarkedUnread(conversation: ConversationSummary, markedUnread: Boolean) = executeAsync(JSONObject().put("type", "setMarkedUnread").put("conversationId", conversation.id).put("markedUnread", markedUnread))
    fun setDraft(conversationId: String, text: String, replyToMessageId: String?) = executeAsync(JSONObject().put("type", "setDraft").put("conversationId", conversationId).put("text", text).put("replyToMessageId", replyToMessageId ?: JSONObject.NULL))

    fun setPinned(conversation: ConversationSummary, pinned: Boolean) = executeAsync(JSONObject().put("type", "pinConversation").put("conversationId", conversation.id).put("pinned", pinned))
    fun setArchived(conversation: ConversationSummary, archived: Boolean) = executeAsync(JSONObject().put("type", "archiveConversation").put("conversationId", conversation.id).put("archived", archived))
    fun setMuted(conversation: ConversationSummary, muted: Boolean) = executeAsync(JSONObject().put("type", "setConversationNotifications").put("conversationId", conversation.id).put("settings", JSONObject().put("mutedUntilMs", if (muted) Long.MAX_VALUE / 4 else JSONObject.NULL).put("sound", JSONObject.NULL).put("showPreview", true).put("notifyMentions", true)))
    fun markRead(conversation: ConversationSummary) { conversation.lastMessageId?.let { executeAsync(JSONObject().put("type", "markRead").put("conversationId", conversation.id).put("messageId", it)) } }

    fun editText(conversationId: String, messageId: String, text: String) {
        val value = text.trim(); if (value.isEmpty()) return
        executeAsync(JSONObject().put("type", "editMessage").put("conversationId", conversationId).put("messageId", messageId)
            .put("content", JSONObject().put("type", "text").put("data", JSONObject().put("text", JSONObject().put("text", value).put("entities", JSONArray())))))
    }
    fun deleteMessage(conversationId: String, messageId: String, forEveryone: Boolean = true) =
        executeAsync(JSONObject().put("type", "deleteMessages").put("conversationId", conversationId).put("messageIds", JSONArray().put(messageId)).put("forEveryone", forEveryone))
    fun setReaction(conversationId: String, messageId: String, reaction: String, enabled: Boolean) =
        executeAsync(JSONObject().put("type", "setReaction").put("conversationId", conversationId).put("messageId", messageId)
            .put("reaction", JSONObject().put("reaction", reaction).put("count", if (enabled) 1 else 0).put("chosenByMe", enabled).put("recentActorIds", if (enabled) JSONArray().put(actorId) else JSONArray())))
    fun forwardMessage(sourceConversationId: String, messageId: String, destinationConversationIds: List<String>) {
        val destinations = normalizeForwardDestinations(destinationConversationIds)
        if (destinations.isEmpty()) return
        val batchId = UUID.randomUUID().toString()
        destinations.forEach { destinationConversationId ->
            executeAsync(
                JSONObject()
                    .put("type", "forwardMessage")
                    .put("sourceConversationId", sourceConversationId)
                    .put("messageId", messageId)
                    .put("destinationConversationId", destinationConversationId)
                    .put("clientMessageId", forwardClientMessageId(batchId, destinationConversationId)),
            )
        }
    }
    fun startTyping(conversationId: String) = executeAsync(JSONObject().put("type", "startTyping").put("conversationId", conversationId).put("action", "typing"))
    fun stopTyping(conversationId: String) = executeAsync(JSONObject().put("type", "stopTyping").put("conversationId", conversationId))

    private fun executeAsync(command: JSONObject) {
        viewModelScope.launch { runCatching { withContext(Dispatchers.IO) { ensureIdentity(); execute(command) } }.onFailure { mutableState.value = mutableState.value.copy(error = it.message) } }
    }

    private fun ensureIdentity() {
        if (actorId.isNotEmpty()) return
        val auth = coordinator.authStatus()
        auth.optJSONObject("user")?.let { displayName = it.optString("nickname").ifBlank { it.optString("username").ifBlank { displayName } } }
        val access = coordinator.messagingAccessIssue( JSONObject().put("deviceId", deviceId).put("sessionId", sessionId)
            .put("scopes", JSONArray(listOf("messaging", "calls", "blobsRead", "blobsWrite", "payments", "miniApps"))))
        actorId = access.optString("actorId")
        check(actorId.isNotBlank()) { "Messaging identity is unavailable" }
        execute(JSONObject().put("type", "upsertProfile").put("actor", JSONObject()
            .put("id", actorId).put("kind", "human").put("displayName", displayName).put("username", JSONObject.NULL).put("avatarUrl", JSONObject.NULL).put("bio", JSONObject.NULL)
            .put("capabilities", JSONArray(listOf("messages", "groups", "channels", "calls", "payments", "miniApps")))
            .put("presence", JSONObject().put("status", "online").put("lastSeenAtMs", System.currentTimeMillis()).put("statusText", JSONObject.NULL)).put("verified", false)))
    }

    private fun execute(command: JSONObject) {
        val requestId = "android-messaging-${UUID.randomUUID()}"
        val envelope = JSONObject().put("protocolVersion", 2)
            .put("context", JSONObject().put("requestId", requestId).put("deviceId", deviceId).put("actorId", actorId).put("sessionId", sessionId).put("sentAtMs", System.currentTimeMillis()))
            .put("command", command)
        val result = coordinator.messagingExecute( JSONObject().put("requestId", requestId).put("envelope", envelope))
        apply(result.optJSONArray("envelopes") ?: JSONArray())
    }

    private fun apply(envelopes: JSONArray) {
        var conversations = mutableState.value.conversations.toMutableList()
        var contacts = mutableState.value.contacts
        var folders = mutableState.value.folders
        val draftMap = mutableState.value.draftsByConversation.toMutableMap()
        val typingMap = mutableState.value.typingActorByConversation.toMutableMap()
        val messageMap = mutableState.value.messagesByConversation.mapValues { it.value.toMutableList() }.toMutableMap()
        for (i in 0 until envelopes.length()) {
            val event = envelopes.optJSONObject(i)?.optJSONObject("event") ?: continue
            when (event.optString("type")) {
                "syncBatch" -> {
                    conversations = parseConversations(event.optJSONArray("conversations") ?: JSONArray()).toMutableList()
                    contacts = parseContacts(event.optJSONArray("actors") ?: JSONArray())
                    folders = parseFolders(event.optJSONArray("folders") ?: JSONArray())
                    draftMap.clear(); parseDrafts(event.optJSONArray("drafts") ?: JSONArray()).forEach { draftMap[it.conversationId] = it }
                    messageMap.clear()
                    parseMessages(event.optJSONArray("messages") ?: JSONArray()).forEach { messageMap.getOrPut(it.conversationId) { mutableListOf() }.add(it) }
                }
                "conversationChanged" -> parseConversation(event.optJSONObject("conversation"))?.let { item ->
                    val index = conversations.indexOfFirst { it.id == item.id }; if (index >= 0) conversations[index] = item else conversations.add(item)
                }
                "conversationParticipantChanged" -> {
                    val removedActorId = event.optString("removedActorId")
                    val item = parseConversation(event.optJSONObject("conversation"))
                    if (item != null && removedActorId == actorId) { conversations.removeAll { it.id == item.id }; messageMap.remove(item.id); draftMap.remove(item.id) }
                    else if (item != null) { val index = conversations.indexOfFirst { it.id == item.id }; if (index >= 0) conversations[index] = item else conversations.add(item) }
                }
                "markedUnreadChanged" -> {
                    val conversationId = event.optString("conversationId"); val index = conversations.indexOfFirst { it.id == conversationId }; if (index >= 0) conversations[index] = conversations[index].copy(markedUnread = event.optBoolean("markedUnread"))
                }
                "draftChanged" -> parseDraft(event.optJSONObject("draft"))?.let { draft ->
                    if (draft.text.isBlank() && draft.replyToMessageId == null) draftMap.remove(draft.conversationId) else draftMap[draft.conversationId] = draft
                }
                "folderChanged" -> parseFolder(event.optJSONObject("folder"))?.let { item ->
                    val list = folders.toMutableList(); val index = list.indexOfFirst { it.id == item.id }; if (index >= 0) list[index] = item else list.add(item); folders = list
                }
                "folderDeleted" -> folders = folders.filterNot { it.id == event.optString("folderId") }
                "messageAdded", "messageChanged" -> parseMessage(event.optJSONObject("message"))?.let { item ->
                    val list = messageMap.getOrPut(item.conversationId) { mutableListOf() }; val index = list.indexOfFirst { it.id == item.id }; if (index >= 0) list[index] = item else list.add(item)
                }
                "messagesDeleted" -> {
                    val conversationId = event.optString("conversationId"); val ids = event.optJSONArray("messageIds")?.toStringSet().orEmpty(); messageMap[conversationId]?.removeAll { it.id in ids }
                }
                "typingChanged" -> {
                    val conversationId = event.optString("conversationId")
                    val typingActorId = event.optString("actorId")
                    if (typingActorId.isBlank() || typingActorId == actorId || event.isNull("action")) typingMap.remove(conversationId)
                    else typingMap[conversationId] = contacts.firstOrNull { it.id == typingActorId }?.displayName ?: "对方"
                }
            }
        }
        conversations = conversations.map { conversation ->
            val draft = draftMap[conversation.id]
            val last = messageMap[conversation.id]?.lastOrNull()
            when { draft != null -> conversation.copy(preview = "草稿：${draft.text}"); last != null -> conversation.copy(preview = last.text, time = last.time); else -> conversation }
        }.toMutableList()
        mutableState.value = mutableState.value.copy(conversations = conversations, contacts = contacts, folders = folders, draftsByConversation = draftMap, messagesByConversation = messageMap.mapValues { it.value.toList() }, typingActorByConversation = typingMap, error = null)
    }

    private fun parseDrafts(array: JSONArray) = buildList { for (i in 0 until array.length()) parseDraft(array.optJSONObject(i))?.let(::add) }
    private fun parseDraft(raw: JSONObject?): MessagingDraft? { raw ?: return null; val conversationId = raw.optString("conversationId"); if (conversationId.isBlank()) return null; return MessagingDraft(conversationId, raw.optString("text"), raw.optString("replyToMessageId").takeIf { it.isNotBlank() }, raw.optLong("updatedAtMs")) }
    private fun parseFolders(array: JSONArray) = buildList { for (i in 0 until array.length()) parseFolder(array.optJSONObject(i))?.let(::add) }
    private fun parseFolder(raw: JSONObject?): MessagingFolder? {
        raw ?: return null; val id = raw.optString("id"); val title = raw.optString("title"); if (id.isBlank() || title.isBlank()) return null
        val conversationIds = buildList { val ids = raw.optJSONArray("conversationIds") ?: JSONArray(); for (i in 0 until ids.length()) ids.optString(i).takeIf { it.isNotBlank() }?.let(::add) }
        return MessagingFolder(id, title, raw.optString("icon").takeIf { it.isNotBlank() }, conversationIds, raw.optBoolean("includeContacts"), raw.optBoolean("includeBots"), raw.optBoolean("includeGroups"), raw.optBoolean("includeChannels"), raw.optBoolean("excludeMuted"), raw.optBoolean("excludeRead"), raw.optBoolean("excludeArchived", true))
    }
    private fun parseContacts(array: JSONArray) = buildList {
        for (i in 0 until array.length()) {
            val raw = array.optJSONObject(i) ?: continue
            val id = raw.optString("id"); if (id.isBlank() || id == actorId) continue
            val name = raw.optString("displayName"); if (name.isBlank()) continue
            add(MessagingContact(id, name, raw.optString("username").takeIf { it.isNotBlank() }, raw.optString("kind", "human")))
        }
    }
    private fun parseConversations(array: JSONArray) = buildList { for (i in 0 until array.length()) parseConversation(array.optJSONObject(i))?.let(::add) }
    private fun parseMessages(array: JSONArray) = buildList { for (i in 0 until array.length()) parseMessage(array.optJSONObject(i))?.let(::add) }

    private fun baseConversation(id: String, kind: ConversationKind, title: String, description: String, participants: JSONArray, now: Long): JSONObject =
        JSONObject().put("id", id).put("kind", kind.wire).put("title", title)
            .put("description", description.let { if (it.isBlank()) JSONObject.NULL else it }).put("avatarUrl", JSONObject.NULL).put("participants", participants)
            .put("ownerId", actorId).put("lastMessageId", JSONObject.NULL).put("lastReadMessageId", JSONObject.NULL)
            .put("unreadCount", 0).put("mentionCount", 0).put("pinnedMessageIds", JSONArray())
            .put("notificationSettings", JSONObject().put("mutedUntilMs", JSONObject.NULL).put("sound", JSONObject.NULL).put("showPreview", true).put("notifyMentions", true))
            .put("permissions", JSONObject().put("canSendMessages", true).put("canSendMedia", true).put("canSendPolls", true).put("canAddMembers", true).put("canPinMessages", true).put("canManageTopics", true).put("canManageCalls", true))
            .put("historyVisibility", "allMembers").put("topics", JSONArray()).put("folderIds", JSONArray())
            .put("archived", false).put("pinned", false).put("markedUnread", false).put("createdAtMs", now).put("updatedAtMs", now)

    private fun parseConversation(raw: JSONObject?): ConversationSummary? {
        raw ?: return null; val id = raw.optString("id"); val title = raw.optString("title"); if (id.isBlank() || title.isBlank()) return null
        val kind = conversationKindFromWire(raw.optString("kind")) ?: return null
        val mutedUntil = raw.optJSONObject("notificationSettings")?.optLong("mutedUntilMs", 0L) ?: 0L
        val participants = buildList { val values = raw.optJSONArray("participants") ?: JSONArray(); for (i in 0 until values.length()) { val participant = values.optJSONObject(i) ?: continue; val actorId = participant.optString("actorId"); val role = participant.optString("role"); if (actorId.isNotBlank() && role.isNotBlank()) add(ConversationParticipant(actorId, role, participant.optLong("joinedAtMs"))) } }
        val pinnedMessageIds = buildList { val ids = raw.optJSONArray("pinnedMessageIds") ?: JSONArray(); for (i in 0 until ids.length()) ids.optString(i).takeIf { it.isNotBlank() }?.let(::add) }
        val folderIds = buildList { val ids = raw.optJSONArray("folderIds") ?: JSONArray(); for (i in 0 until ids.length()) ids.optString(i).takeIf { it.isNotBlank() }?.let(::add) }
        return ConversationSummary(id = id, title = title, description = raw.optString("description"), ownerId = raw.optString("ownerId").takeIf { it.isNotBlank() }, participants = participants, preview = raw.optString("description"), time = timeLabel(raw.optLong("updatedAtMs")), badge = title.take(1).uppercase().ifBlank { "✦" }, kind = kind,
            unreadCount = raw.optInt("unreadCount"), isPinned = raw.optBoolean("pinned"), isMuted = mutedUntil > System.currentTimeMillis(), isArchived = raw.optBoolean("archived"), lastMessageId = raw.optString("lastMessageId").takeIf { it.isNotBlank() }, pinnedMessageIds = pinnedMessageIds, folderIds = folderIds, markedUnread = raw.optBoolean("markedUnread"))
    }

    private fun parseMessage(raw: JSONObject?): ChatMessage? {
        raw ?: return null; val id = raw.optString("id"); val conversationId = raw.optString("conversationId"); val senderId = raw.optString("senderId"); if (id.isBlank() || conversationId.isBlank()) return null
        val content = raw.optJSONObject("content") ?: JSONObject()
        val contentType = content.optString("type", "unknown")
        val data = content.optJSONObject("data") ?: JSONObject()
        val media = data.optJSONObject("media")
        val mediaFileName = media?.optString("fileName")?.takeIf { it.isNotBlank() }
        val mediaBlobId = media?.optString("id")?.takeIf { it.isNotBlank() }
        val mediaMimeType = media?.optString("mimeType")?.takeIf { it.isNotBlank() }
        val mediaSizeBytes = media?.optInt("sizeBytes", 0) ?: 0
        val contactName = data.optString("displayName").takeIf { it.isNotBlank() }
        val latitude = if (data.has("latitude")) data.optDouble("latitude") else null
        val longitude = if (data.has("longitude")) data.optDouble("longitude") else null
        val pollQuestion = data.optJSONObject("question")?.optString("text")?.takeIf { it.isNotBlank() }
        val pollOptions = buildList {
            val options = data.optJSONArray("options") ?: JSONArray()
            for (i in 0 until options.length()) {
                val option = options.optJSONObject(i) ?: continue
                val id = option.optString("id"); val text = option.optString("text")
                if (id.isNotBlank() && text.isNotBlank()) add(ChatPollOption(id, text, option.optInt("voterCount"), option.optBoolean("chosen")))
            }
        }
        val pollMultipleAnswers = data.optBoolean("multipleAnswers")
        val text = when (contentType) {
            "text" -> data.optJSONObject("text")?.optString("text").orEmpty()
            "photo" -> mediaFileName?.let { "🖼 $it" } ?: "🖼 图片"
            "video" -> mediaFileName?.let { "🎬 $it" } ?: "🎬 视频"
            "document" -> mediaFileName?.let { "📎 $it" } ?: "📎 文件"
            "voice" -> "🎙 语音消息"
            "audio" -> mediaFileName?.let { "🎵 $it" } ?: "🎵 音频"
            "location" -> "📍 位置"
            "contact" -> contactName?.let { "👤 $it" } ?: "👤 联系人"
            "invoice" -> "🧾 账单"
            "poll" -> pollQuestion?.let { "📊 $it" } ?: "📊 投票"
            "miniApp" -> "▣ Mini App"
            else -> "消息"
        }
        val reactions = buildList {
            val array = raw.optJSONArray("reactions") ?: JSONArray()
            for (i in 0 until array.length()) {
                val reaction = array.optJSONObject(i) ?: continue
                val symbol = reaction.optString("reaction"); if (symbol.isNotBlank()) add(ChatReaction(symbol, reaction.optInt("count"), reaction.optBoolean("chosenByMe")))
            }
        }
        val deliveryRaw = raw.opt("deliveryState")
        val deliveryState = when (deliveryRaw) {
            is String -> deliveryRaw
            is JSONObject -> deliveryRaw.optString("state").ifBlank { deliveryRaw.keys().asSequence().firstOrNull() ?: "sent" }
            else -> "sent"
        }
        return ChatMessage(
            id = id, conversationId = conversationId, text = text, contentType = contentType, mediaFileName = mediaFileName, mediaBlobId = mediaBlobId, mediaMimeType = mediaMimeType, mediaSizeBytes = mediaSizeBytes, contactName = contactName,
            latitude = latitude, longitude = longitude, pollQuestion = pollQuestion, pollOptions = pollOptions, pollMultipleAnswers = pollMultipleAnswers, outgoing = senderId == actorId,
            time = timeLabel(raw.optLong("createdAtMs")), replyToMessageId = raw.optString("replyToMessageId").takeIf { it.isNotBlank() },
            forwardOrigin = raw.optString("forwardOrigin").takeIf { it.isNotBlank() }, reactions = reactions, deliveryState = deliveryState,
            edited = !raw.isNull("editedAtMs"), pinned = raw.optBoolean("pinned")
        )
    }

    private fun timeLabel(ms: Long): String = if (ms <= 0) "" else java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(ms))

    override fun onCleared() { super.onCleared() }
}

private fun JSONArray.toStringSet(): Set<String> = buildSet { for (i in 0 until length()) optString(i).takeIf { it.isNotBlank() }?.let(::add) }
