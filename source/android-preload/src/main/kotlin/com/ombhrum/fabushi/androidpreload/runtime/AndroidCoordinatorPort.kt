package com.ombhrum.fabushi.androidpreload.runtime

import android.content.Context
import android.view.View

import org.json.JSONArray
import org.json.JSONObject

data class AndroidSidebarSection(
    val id: String,
    val name: String,
    val agentIds: List<String>,
)

data class AndroidMcpOAuthCompletion(
    val provider: String,
    val state: String,
    val outcome: String,
)

enum class AndroidSendAcknowledgementPhase(val wireName: String) {
    PENDING("pending"),
    QUEUED("queued"),
    DISPATCHING("dispatching"),
    ACCEPTED_AWAITING_ECHO("accepted-awaiting-echo"),
    FAILED("failed"),
    OUTCOME_UNKNOWN("outcome-unknown");

    companion object {
        fun fromWire(value: String): AndroidSendAcknowledgementPhase =
            entries.firstOrNull { it.wireName == value }
                ?: error("Unsupported send acknowledgement phase: $value")
    }
}

data class AndroidSendAcknowledgementEntry(
    val id: String,
    val kind: String,
    val clientNonce: String,
)

data class AndroidSendAcknowledgement(
    val accountSlot: String?,
    val agentId: String,
    val nonce: String,
    val priorNonces: List<String>,
    val entries: List<AndroidSendAcknowledgementEntry>,
    val messageText: String,
    val phase: AndroidSendAcknowledgementPhase,
    val generation: Long,
    val failedAtMs: Long?,
    val reason: String?,
)

data class AndroidSendAcknowledgementDispatch(
    val acknowledgement: AndroidSendAcknowledgement,
    val operationId: String,
)

data class AndroidComposerDraft(
    val draftId: String,
    val prompt: String,
    val updatedAtMs: Long,
)


data class AndroidApprovalCleanupResult(
    val generation: Long,
    val cleared: Boolean,
    val approvalId: String?,
)

data class AndroidWidgetActionResult(
    val accepted: Boolean,
    val generation: Long,
    val operationId: String? = null,
    val reason: String? = null,
)

data class AndroidToolResultCard(
    val kind: String,
    val toolCallId: String,
    val status: String,
    val path: String?,
    val command: String?,
    val workingDirectory: String?,
    val summary: String,
    val output: String,
    val diff: String,
    val isStreaming: Boolean,
    val isBackground: Boolean,
)

data class AndroidClientSideToolV2Event(
    val accountSlot: String,
    val agentId: String,
    val epoch: String,
    val sequence: Long,
    val projection: AndroidToolResultCard?,
)

data class AndroidConversationOutlineItem(
    val kind: String,
    val id: String,
    val text: String? = null,
    val name: String? = null,
    val status: String? = null,
    val summary: String? = null,
    val messageType: String? = null,
    val messageValue: String? = null,
)

fun parseAndroidConversationOutlineItems(
    entries: JSONArray,
    agentId: String,
): List<AndroidConversationOutlineItem> {
    require(agentId.isNotBlank()) { "Conversation outline requires a non-empty Agent id" }
    val byId = linkedMapOf<String, AndroidConversationOutlineItem>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("agentId").trim() != agentId) continue
        val id = entry.optString("id").trim()
        if (id.isEmpty()) continue
        val item = when (entry.optString("kind")) {
            "message" -> {
                val role = entry.optString("role")
                check(role == "user" || role == "assistant") {
                    "Conversation outline message $id has an unsupported role"
                }
                AndroidConversationOutlineItem(
                    kind = if (role == "user") "user" else "assistant-text",
                    id = id,
                    text = entry.optString("content"),
                )
            }
            "thinking" -> AndroidConversationOutlineItem(
                kind = "thinking",
                id = id,
                text = entry.optString("text").ifBlank { entry.optString("content") },
            )
            "tool-call" -> {
                val name = entry.optString("name").trim()
                check(name.isNotEmpty()) { "Conversation outline tool call $id has no name" }
                val status = when (entry.optString("status").trim()) {
                    "pending", "running" -> "pending"
                    "done", "completed", "success" -> "done"
                    "failed", "error", "aborted", "cancelled", "denied", "rejected" -> "failed"
                    else -> error("Conversation outline tool call $id has an unsupported status")
                }
                AndroidConversationOutlineItem(
                    kind = "tool-call",
                    id = id,
                    name = name,
                    status = status,
                    summary = entry.optString("summary").takeIf(String::isNotBlank),
                )
            }
            "send-message" -> {
                val message = entry.optJSONObject("message") ?: continue
                when (message.optString("type")) {
                    "text" -> AndroidConversationOutlineItem(
                        kind = "send-message",
                        id = id,
                        messageType = "text",
                        messageValue = message.optString("content"),
                    )
                    "attachment" -> {
                        val value = message.optString("url").trim()
                        if (value.isEmpty()) continue
                        AndroidConversationOutlineItem(
                            kind = "send-message",
                            id = id,
                            messageType = "attachment",
                            messageValue = value,
                        )
                    }
                    else -> continue
                }
            }
            else -> continue
        }
        byId[id] = item
    }
    return byId.values.toList()
}

/**
 * Typed method surface exposed to Android presentation code.
 *
 * Method names are part of the Android contract: callers cannot choose an arbitrary Host method
 * string. JSON remains confined to compatibility payloads while each domain is migrated to fully
 * typed Kotlin models.
 */
interface AndroidCoordinatorPort {
    fun coordinatorStatus(): JSONObject
    fun coordinatorResync(generation: Long, afterSequence: Long): JSONObject
    fun mcpOAuthRegister(state: String, provider: String): Boolean
    fun mcpOAuthRegisterBound(
        state: String,
        provider: String,
        serverId: String,
        accountKey: String,
        generation: Long,
    ): Boolean = mcpOAuthRegister(state, provider)
    fun mcpOAuthComplete(
        state: String,
        code: String?,
        error: String?,
    ): AndroidMcpOAuthCompletion

    fun authStatus(): JSONObject
    fun accountAccessProjection(): AccountAccessProjection
    fun authDeviceAgentSession(): JSONObject
    fun authBrowserStart(): JSONObject
    fun authBrowserReopen(params: JSONObject): JSONObject
    fun authBrowserCancel(params: JSONObject): JSONObject
    fun authBrowserPoll(params: JSONObject): JSONObject
    fun authLogout(): JSONObject
    fun automationUpsert(params: JSONObject): JSONObject
    fun automationList(): org.json.JSONArray
    fun automationStart(params: JSONObject): JSONObject
    fun automationAdvanceStep(params: JSONObject): JSONObject
    fun automationAwaitApproval(params: JSONObject): JSONObject
    fun automationResolveApproval(params: JSONObject): JSONObject
    fun automationCancel(params: JSONObject): JSONObject
    fun automationSettle(params: JSONObject): JSONObject
    fun automationSnapshot(params: JSONObject): JSONObject

    fun featureExecute(params: JSONObject): JSONObject
    fun sendAcknowledgementBegin(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement
    fun sendAcknowledgementDispatch(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
    ): AndroidSendAcknowledgementDispatch
    fun sendAcknowledgementRetry(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
        freshNonce: String,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgementDispatch

    fun clearApprovalsAfterJournal(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
        approvalId: String? = null,
        approvalOperationId: String? = null,
    ): AndroidApprovalCleanupResult =
        error("Post-journal approval cleanup is unavailable on this Coordinator port")
    fun sendAcknowledgementSnapshot(
        accountSlot: String?,
        agentId: String,
    ): List<AndroidSendAcknowledgement>
    fun composerDraftRead(accountSlot: String?, agentId: String): AndroidComposerDraft? =
        error("Durable composer draft projection is unavailable on this Coordinator port")
    fun composerDraftWrite(accountSlot: String?, agentId: String, prompt: String): AndroidComposerDraft =
        error("Durable composer draft mutation is unavailable on this Coordinator port")
    fun composerDraftClearIfCurrent(accountSlot: String?, agentId: String, draftId: String): Boolean =
        error("Durable composer draft conditional clear is unavailable on this Coordinator port")
    fun acceptClientSideToolV2(event: JSONObject): AndroidClientSideToolV2Event =
        error("Client-side tool v2 ingress is unavailable on this Coordinator port")
    fun replayClientSideToolV2(): List<AndroidClientSideToolV2Event> = emptyList()
    fun respondToWidget(
        accountSlot: String?,
        agentId: String,
        entryId: String,
        value: String,
        generation: Long,
    ): AndroidWidgetActionResult =
        error("Transcript widget response is unavailable on this Coordinator port")
    fun dismissWidget(
        accountSlot: String?,
        agentId: String,
        entryId: String,
        generation: Long,
    ): AndroidWidgetActionResult =
        error("Transcript widget dismissal is unavailable on this Coordinator port")
    fun featureInterrupt(params: JSONObject): JSONObject
    fun featureApprovalResolve(params: JSONObject): JSONObject
    fun transcriptSnapshot(): JSONArray
    fun conversationOutlineSnapshot(agentId: String): List<AndroidConversationOutlineItem> =
        parseAndroidConversationOutlineItems(transcriptSnapshot(), agentId)
    fun assistantProjection(): JSONObject = JSONObject().put("hasUnread", false)
    fun assistantMarkRead(): JSONObject = assistantProjection()
    fun agentSubagentTool(params: JSONObject): JSONObject
    fun agentSubagentReconcile(params: JSONObject): JSONObject
    fun agentAsyncTasks(id: String): JSONArray =
        error("Agent async tasks are unavailable on this Coordinator port")

    fun agentList(): JSONArray
    fun agentCreate(name: String, description: String): JSONObject
    fun agentCreateGroup(name: String, description: String, memberIds: List<String>): JSONObject
    fun agentSetGroupMembers(id: String, memberIds: List<String>): JSONObject
    fun agentUpdate(id: String, name: String, description: String): JSONObject
    fun agentUpdateProfile(
        id: String,
        name: String,
        description: String,
        title: String?,
        avatarShape: String?,
        avatarColor: String?,
    ): JSONObject = agentUpdate(id, name, description)
    fun agentSetNotifyOnUpdates(id: String, isEnabled: Boolean): JSONObject =
        error("Agent notification settings are unavailable on this Coordinator port")
    fun agentSetHidden(id: String, isHidden: Boolean): JSONObject
    fun agentSetUnread(id: String, isUnread: Boolean): JSONObject
    fun agentDuplicate(id: String): JSONObject
    fun agentDelete(id: String): JSONObject
    fun agentSetPinned(ids: List<String>): List<String>
    fun agentSidebarSections(): List<AndroidSidebarSection>
    fun agentSetSidebarSections(sections: List<AndroidSidebarSection>): List<AndroidSidebarSection>

    fun marketplaceBrowse(params: JSONObject): JSONObject
    fun marketplaceRelease(params: JSONObject): JSONObject
    fun pluginInstall(params: JSONObject): JSONObject
    fun pluginVariableFields(schema: JSONObject): JSONArray
    fun pluginVariablesConfigure(params: JSONObject): JSONObject
    fun pluginUiDocument(params: JSONObject): JSONObject
    fun pluginCompatibility(params: JSONObject): JSONObject
    fun pluginPermissionGrant(params: JSONObject): JSONObject

    fun runtimeStart(params: JSONObject): JSONObject
    fun runtimeCallValue(params: JSONObject): Any?
    fun runtimeCancel(params: JSONObject): JSONObject

    fun messagingAccessIssue(params: JSONObject): JSONObject
    fun messagingBlobRead(params: JSONObject): JSONObject
    fun messagingExecute(params: JSONObject): JSONObject

    fun platformRequest(params: JSONObject): JSONObject
    fun remoteComputerList(): JSONObject = error("remote_computer_list_not_implemented")
    fun remoteComputerPair(pairingCode: String, label: String): JSONObject =
        error("remote_computer_pair_not_implemented")
    fun remoteComputerRevoke(deviceId: String, clientId: String): JSONObject =
        error("remote_computer_revoke_not_implemented")
    fun remoteComputerPairingStatus(): JSONObject =
        error("remote_computer_pairing_status_not_implemented")
    fun remoteComputerSessionCreate(deviceId: String): JSONObject =
        error("remote_computer_session_create_not_implemented")
    fun remoteComputerSessionReconcile(): JSONObject =
        error("remote_computer_session_reconcile_not_implemented")
    fun remoteComputerSessionStatus(): JSONObject =
        error("remote_computer_session_status_not_implemented")
    fun remoteComputerSessionTransport(
        deviceId: String,
        sessionId: String,
        directAvailable: Boolean,
        relayRegion: String? = null,
    ): JSONObject = error("remote_computer_session_transport_not_implemented")
    fun remoteComputerSignal(
        deviceId: String,
        sessionId: String,
        kind: String,
        payload: JSONObject,
    ): JSONObject = error("remote_computer_signal_not_implemented")
    fun remoteComputerSignalDrain(
        deviceId: String,
        sessionId: String,
        afterSignalId: Long = 0L,
    ): JSONObject = error("remote_computer_signal_drain_not_implemented")
    fun remoteComputerSignalAcknowledge(
        deviceId: String,
        sessionId: String,
        lastSignalId: Long,
    ): JSONObject = error("remote_computer_signal_acknowledge_not_implemented")
    fun remoteComputerHumanTakeover(
        deviceId: String,
        sessionId: String,
        expectedViewportRevision: Long,
        active: Boolean,
    ): JSONObject = error("remote_computer_human_takeover_not_implemented")
    fun remoteComputerViewportAdvance(
        deviceId: String,
        sessionId: String,
        expectedViewportRevision: Long,
    ): JSONObject = error("remote_computer_viewport_advance_not_implemented")
    fun remoteComputerDataPlaneConnect(deviceId: String, sessionId: String): JSONObject =
        error("remote_computer_data_plane_connect_not_implemented")
    fun remoteComputerViewportView(context: Context): View =
        error("remote_computer_viewport_view_not_implemented")
    fun remoteComputerDataPlaneDisconnect(): JSONObject =
        error("remote_computer_data_plane_disconnect_not_implemented")
    fun remoteComputerSessionClose(deviceId: String, sessionId: String): JSONObject =
        error("remote_computer_session_close_not_implemented")
    fun computerRebuildRequest(
        preserveData: Boolean = true,
        forceRecreate: Boolean = false,
    ): JSONObject = error("computer_rebuild_request_not_implemented")
    fun computerRebuildQueueWhenIdle(): JSONObject =
        error("computer_rebuild_queue_when_idle_not_implemented")
    fun computerRebuildForceUpdate(): JSONObject =
        error("computer_rebuild_force_update_not_implemented")
    fun computerRebuildCancelQueuedUpdate(): JSONObject =
        error("computer_rebuild_cancel_queued_update_not_implemented")
    fun computerRebuildStatus(): JSONObject =
        error("computer_rebuild_status_not_implemented")
    fun webAuthnRegisterProvider(): JSONObject
    fun webAuthnUnregisterProvider(params: JSONObject): JSONObject
    fun webAuthnPollRequest(params: JSONObject): JSONObject
    fun webAuthnSubmitResponses(params: JSONObject): JSONObject
    fun publishFeatureEvent(event: JSONObject)
    fun addFeatureEventListener(listener: (JSONObject) -> Unit): AutoCloseable
}
