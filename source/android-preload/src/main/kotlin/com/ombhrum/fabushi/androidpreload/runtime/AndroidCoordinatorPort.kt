package com.ombhrum.fabushi.androidpreload.runtime

import android.content.Context
import android.view.View

import org.json.JSONArray
import org.json.JSONObject

data class AndroidSidebarSection(
    val id: String,
    val name: String,
    val agentIds: List<String>,
    val isCollapsed: Boolean = false,
)

data class AndroidSidebarSectionsSnapshot(
    val accountFence: String,
    val revision: Long,
    val sections: List<AndroidSidebarSection>,
)

data class AndroidMcpOAuthCompletion(
    val provider: String,
    val state: String,
    val outcome: String,
)

data class AndroidGeneratedAvatarImage(
    val imageData: String,
    val mimeType: String,
) {
    val dataUrl: String
        get() = "data:$mimeType;base64,$imageData"
}

enum class AndroidHumanCallState(val wireName: String) {
    INVITED("invited"),
    RINGING("ringing"),
    NEGOTIATING("negotiating"),
    CONNECTED("connected"),
    RECONNECTING("reconnecting"),
    ENDED("ended"),
    FAILED("failed"),
    OUTCOME_UNKNOWN("outcome_unknown");

    companion object {
        fun fromWire(value: String): AndroidHumanCallState =
            entries.firstOrNull { it.wireName == value }
                ?: error("Unsupported Human call state: $value")
    }
}

data class AndroidHumanCallSession(
    val id: String,
    val requestId: String,
    val scopeId: String,
    val creatorId: String,
    val participantIds: List<String>,
    val state: AndroidHumanCallState,
    val generation: Long,
    val signalSeq: Long,
    val mediaCapabilities: JSONObject,
    val deviceSelection: JSONObject,
    val createdAtMs: Long,
    val updatedAtMs: Long,
    val terminalReason: String?,
)

data class AndroidHumanCallLease(
    val callId: String,
    val userId: String,
    val deviceId: String,
    val role: String,
    val isOwner: Boolean,
    val claimAvailable: Boolean,
    val generation: Long,
)

data class AndroidHumanCallSignal(
    val callId: String,
    val generation: Long,
    val seq: Long,
    val senderDeviceId: String,
    val kind: String,
    val payload: JSONObject,
    val createdAtMs: Long,
)

data class AndroidPrReviewPreferences(
    val user: String?,
    val team: String?,
)

data class AndroidTimeZoneSettings(
    val detectedTimeZone: String,
    val overrideTimeZone: String?,
) {
    val effectiveTimeZone: String get() = overrideTimeZone ?: detectedTimeZone
}

data class AndroidAutoReviewInstructions(
    val isEnabled: Boolean = true,
    val allowInstructions: List<String> = emptyList(),
    val blockInstructions: List<String> = emptyList(),
)

data class AndroidAutoReviewInstructionsSnapshot(
    val accountFence: String,
    val revision: Long,
    val instructions: AndroidAutoReviewInstructions,
)

data class AndroidOnDemandUsage(
    val usedCents: Long,
    val limitCents: Long,
    val resetTimestampMs: Long?,
)

data class AndroidAccountUsageSummary(
    val sandUsagePercent: Double?,
    val sandUsageResetTimestampMs: Long?,
    val hasAvailableUsage: Boolean,
    val hasNonZeroIncludedLimit: Boolean,
    val usesPooledEnterpriseAllowance: Boolean,
    val isSandTrial: Boolean,
    val hasEndedSandTrial: Boolean,
    val canCancelSandTrial: Boolean,
    val onDemand: AndroidOnDemandUsage?,
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
    val messageRichText: JSONObject? = null,
    val phase: AndroidSendAcknowledgementPhase,
    val generation: Long,
    val enteredAtMs: Long = 0L,
    val dispatchStartedAtMs: Long? = null,
    val failedAtMs: Long?,
    val reason: String?,
    val attachments: List<AndroidAgentAttachment> = emptyList(),
)

data class AndroidSendAcknowledgementDispatch(
    val acknowledgement: AndroidSendAcknowledgement,
    val operationId: String?,
)

data class AndroidAgentAttachment(
    val id: String,
    val path: String,
    val name: String,
    val sizeBytes: Long,
    val mimeType: String?,
)

data class AndroidComposerDraft(
    val draftId: String,
    val prompt: String,
    val updatedAtMs: Long,
    val attachments: List<AndroidAgentAttachment> = emptyList(),
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


data class AndroidTranscriptHistoryPage(
    val entries: JSONArray,
    val nextBeforeSeq: Long?,
)

data class AndroidFindInChatEntry(
    val entryId: String,
    val searchText: String,
)

fun parseAndroidFindInChatEntries(
    entries: JSONArray,
    agentId: String,
): List<AndroidFindInChatEntry> {
    require(agentId.isNotBlank()) { "Find in chat requires a non-empty Agent id" }
    val byId = linkedMapOf<String, AndroidFindInChatEntry>()
    for (index in 0 until entries.length()) {
        val entry = entries.optJSONObject(index) ?: continue
        if (entry.optString("agentId").trim() != agentId) continue
        val id = entry.optString("id").trim()
        if (id.isEmpty()) continue
        val searchText = when (entry.optString("kind")) {
            "message" -> {
                val parts = mutableListOf<String>()
                entry.optString("content").takeIf(String::isNotEmpty)?.let(parts::add)
                entry.optString("author").takeIf(String::isNotEmpty)?.let(parts::add)
                val attachments = entry.optJSONArray("attachments")
                if (attachments != null) {
                    for (attachmentIndex in 0 until attachments.length()) {
                        attachments.optJSONObject(attachmentIndex)
                            ?.optString("name")
                            ?.takeIf(String::isNotEmpty)
                            ?.let(parts::add)
                    }
                }
                parts.joinToString("\n")
            }
            "notice" -> entry.optString("text")
            "send-message" -> {
                val message = entry.optJSONObject("message") ?: continue
                when (message.optString("type")) {
                    "text" -> message.optString("content")
                    "widget" -> message.optJSONObject("widget")?.optString("prompt").orEmpty()
                    "email-draft" -> {
                        val draft = message.optJSONObject("draft") ?: continue
                        val subject = draft.optString("subject")
                        val body = draft.optString("body")
                        if (subject.isEmpty() || body.isEmpty()) "" else "$subject\n$body"
                    }
                    "slack-draft" -> message.optJSONObject("draft")?.optString("body").orEmpty()
                    else -> ""
                }
            }
            else -> ""
        }
        if (searchText.isNotEmpty()) {
            byId[id] = AndroidFindInChatEntry(entryId = id, searchText = searchText)
        }
    }
    return byId.values.toList()
}

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
data class AndroidUpdateTrackPolicy(
    val managedTrack: String?,
    val unlockInternalTracks: Boolean,
    val effectiveTrack: String,
    val selectableTracks: List<String>,
    val availableTracks: List<String>,
)

data class AndroidMandatoryUpdatePolicy(
    val accountEpoch: Long,
    val processGeneration: Long,
    val minimumVersion: String?,
    val backendRequiresUpdate: Boolean,
    val autoUpdateWhenIdleGateObserved: Boolean = false,
    val autoUpdateWhenIdleGateEnabled: Boolean = false,
)

enum class AndroidEntrypointId(val wireId: String) {
    COMPUTER("overlay:computer"),
    HIDDEN_CHATS("overlay:hidden-chats"),
    AGENT_NETWORK("view:org-chart"),
    PLUGINS("overlay:plugins"),
    SETTINGS("overlay:settings"),
}

sealed interface AndroidEntrypointAvailability {
    data object Available : AndroidEntrypointAvailability
    data class Retained(val reason: String) : AndroidEntrypointAvailability
    data class Unavailable(val reason: String) : AndroidEntrypointAvailability

    val isOpenable: Boolean
        get() = this !is Unavailable
}

data class AndroidEntrypointAvailabilitySnapshot(
    val accountEpoch: Long,
    val processGeneration: Long,
    val entries: Map<AndroidEntrypointId, AndroidEntrypointAvailability>,
) {
    fun availability(id: AndroidEntrypointId): AndroidEntrypointAvailability =
        entries[id] ?: AndroidEntrypointAvailability.Unavailable("missing")

    companion object {
        fun initial(
            processGeneration: Long = 0L,
            accountEpoch: Long = 0L,
        ): AndroidEntrypointAvailabilitySnapshot =
            AndroidEntrypointAvailabilitySnapshot(
                accountEpoch = accountEpoch,
                processGeneration = processGeneration,
                entries = mapOf(
                    AndroidEntrypointId.COMPUTER to AndroidEntrypointAvailability.Available,
                    AndroidEntrypointId.HIDDEN_CHATS to AndroidEntrypointAvailability.Available,
                    AndroidEntrypointId.AGENT_NETWORK to AndroidEntrypointAvailability.Unavailable("gate"),
                    AndroidEntrypointId.PLUGINS to AndroidEntrypointAvailability.Available,
                    AndroidEntrypointId.SETTINGS to AndroidEntrypointAvailability.Available,
                ),
            )
    }
}

interface AndroidCoordinatorPort {
    fun coordinatorStatus(): JSONObject
    fun coordinatorResync(generation: Long, afterSequence: Long): JSONObject
    fun resolveUpdateTrackPolicy(
        managedTrack: String?,
        unlockInternalTracks: Boolean,
        userOverride: String?,
        buildDefault: String?,
    ): AndroidUpdateTrackPolicy =
        error("Update-track policy is unavailable on this Coordinator port")
    fun refreshMandatoryUpdatePolicy(): AndroidMandatoryUpdatePolicy =
        error("Mandatory update policy is unavailable on this Coordinator port")
    fun refreshEntrypointAvailability(): AndroidEntrypointAvailabilitySnapshot =
        error("Entrypoint availability is unavailable on this Coordinator port")
    fun isUpdateHostIdle(): Boolean = false
    fun mcpServerCatalog(): JSONArray =
        error("MCP server catalog is unavailable on this Coordinator port")
    fun mcpTeamPopularity(): JSONObject =
        error("MCP team popularity is unavailable on this Coordinator port")
    fun availableModelCatalog(): JSONObject =
        error("Available model catalog is unavailable on this Coordinator port")
    fun mcpServerTools(serverId: String): JSONArray =
        error("MCP server tool catalog is unavailable on this Coordinator port")
    fun mcpServerCustomInstructions(serverId: String): String? =
        error("MCP server custom instructions are unavailable on this Coordinator port")
    fun setMcpServerCustomInstructions(serverId: String, instructions: String): String? =
        error("MCP server custom instruction mutation is unavailable on this Coordinator port")
    fun toggleMcpServerTool(serverId: String, toolName: String): JSONArray =
        error("MCP server tool toggle is unavailable on this Coordinator port")
    fun mcpServerAccounts(serverId: String): JSONArray =
        error("MCP server account projection is unavailable on this Coordinator port")
    fun logoutMcpAccount(serverId: String, accountKey: String): JSONObject =
        error("MCP account logout is unavailable on this Coordinator port")
    fun renameMcpAccount(serverId: String, accountKey: String, newAccountKey: String): JSONObject =
        error("MCP account rename is unavailable on this Coordinator port")
    fun removeMcpAccount(serverId: String, accountKey: String): JSONObject =
        error("MCP account removal is unavailable on this Coordinator port")
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
    fun accountPrivacyMode(): JSONObject
    fun accountPrReviewPreferences(): AndroidPrReviewPreferences =
        AndroidPrReviewPreferences(user = null, team = null)
    fun onboardingSeen(): JSONObject =
        error("Onboarding setting is unavailable on this Coordinator port")
    fun setOnboardingSeen(
        expectedAccountScope: String,
        hasSeenOnboarding: Boolean,
    ): JSONObject =
        error("Onboarding setting mutation is unavailable on this Coordinator port")

    fun timeZoneSettings(): AndroidTimeZoneSettings =
        error("Time zone settings are unavailable on this Coordinator port")
    fun setTimeZoneOverride(value: String?): AndroidTimeZoneSettings =
        error("Time zone mutation is unavailable on this Coordinator port")
    fun processMetricsSettings(): JSONObject =
        error("Process metrics settings are unavailable on this Coordinator port")
    fun setProcessMetricsLocalEnabled(enabled: Boolean): JSONObject =
        error("Process metrics setting mutation is unavailable on this Coordinator port")
    fun reportRecoveredProcessCrash(
        kind: String,
        errorClass: String,
        occurredAtMs: Long,
        uptimeMs: Long,
    ): JSONObject = error("Process crash telemetry is unavailable on this Coordinator port")
    fun reportStartupLifecycle(
        outcome: String,
        phase: String,
        elapsedMs: Long,
        phaseElapsedMs: Long? = null,
        windowMs: Long? = null,
    ): JSONObject = error("Startup lifecycle telemetry is unavailable on this Coordinator port")
    fun reportAgentLoad(
        durationMs: Long,
        openRpcMs: Long?,
        queueMs: Long? = null,
        renderMs: Long? = null,
        cold: Boolean,
        stale: Boolean?,
        boxState: String = "unknown",
        outcome: String,
    ): JSONObject = error("Agent load telemetry is unavailable on this Coordinator port")
    fun reportOnboardingStep(
        expectedAccountScope: String,
        step: String,
        stepIndex: Int,
        direction: String,
    ): JSONObject = error("Onboarding telemetry is unavailable on this Coordinator port")
    fun reportReactionAck(
        ackMs: Long,
        outcome: String,
        conversationId: String,
        emoji: String,
    ): JSONObject = error("Reaction acknowledgement telemetry is unavailable on this Coordinator port")

    fun reportRenderTtfr(
        ttfrMs: Long,
        outcome: String,
        chunkType: String?,
        enterEpochMs: Long,
        isFork: Boolean,
        traceId: String? = null,
        spanId: String? = null,
        clientNonce: String? = null,
        conversationId: String? = null,
    ): JSONObject = error("Render TTFR telemetry is unavailable on this Coordinator port")

    fun reportRenderStream(
        outcome: String,
        chunkCount: Int,
        maxGapMs: Long,
        stalledMs: Long,
        stallCount: Int,
        streamMs: Long,
        isFork: Boolean,
        traceId: String? = null,
        spanId: String? = null,
        clientNonce: String? = null,
        conversationId: String? = null,
    ): JSONObject = error("Render stream telemetry is unavailable on this Coordinator port")

    fun reportRebuildEscalation(
        rebuildKind: String,
        elapsedMs: Long,
        action: String,
        escalation: String,
    ): JSONObject = error("Rebuild telemetry is unavailable on this Coordinator port")

    fun reportRebuildPendingStall(
        rebuildKind: String,
        elapsedMs: Long,
        hasAck: Boolean,
        streamAttached: Boolean,
    ): JSONObject = error("Rebuild telemetry is unavailable on this Coordinator port")

    fun reportReplicaResync(
        replica: String,
        outcome: String,
        attempts: Int,
        durationMs: Long,
        bufferOverflowed: Boolean,
        epochChanged: Boolean,
    ): JSONObject = error("Reconciliation telemetry is unavailable on this Coordinator port")

    fun reportSendJournalRestore(
        outcome: String,
        phase: String,
        ageMs: Long,
        recordCount: Int,
    ): JSONObject = error("Reconciliation telemetry is unavailable on this Coordinator port")

    fun reportResyncCompleted(
        failedSteps: List<String>,
        durationMs: Long,
    ): JSONObject = error("Reconciliation telemetry is unavailable on this Coordinator port")

    fun reportRecoveryAction(
        action: String,
        failureClass: String,
        consecutiveIdentical: Int,
    ): JSONObject = error("Recovery telemetry is unavailable on this Coordinator port")
    fun autoReviewInstructions(): AndroidAutoReviewInstructionsSnapshot =
        error("Auto Review instructions are unavailable on this Coordinator port")
    fun setAutoReviewInstructions(
        expectedRevision: Long,
        instructions: AndroidAutoReviewInstructions,
    ): AndroidAutoReviewInstructionsSnapshot =
        error("Auto Review instruction mutation is unavailable on this Coordinator port")
    fun submitFeedback(
        message: String,
        submissionId: String,
        conversationId: String? = null,
    ): JSONObject = error("Feedback submission is unavailable on this Coordinator port")
    fun accountUsageSummary(): AndroidAccountUsageSummary =
        error("Account usage summary is unavailable on this Coordinator port")
    fun accountCancelSandTrial(): Unit =
        error("Sand trial cancellation is unavailable on this Coordinator port")
    fun credentialVaultList(): org.json.JSONArray =
        error("Credential Vault metadata is unavailable on this Coordinator port")
    fun credentialVaultUpsert(
        name: String,
        label: String,
        secretValue: String,
        allowedOrigins: List<String>,
    ): JSONObject = error("Credential Vault mutation is unavailable on this Coordinator port")
    fun credentialVaultRotate(
        name: String,
        label: String,
        secretValue: String,
        allowedOrigins: List<String>,
    ): JSONObject = error("Credential Vault rotation is unavailable on this Coordinator port")
    fun credentialVaultRevoke(name: String): Boolean =
        error("Credential Vault revocation is unavailable on this Coordinator port")
    fun accountAccessProjection(): AccountAccessProjection
    fun rootConnectionResilienceProjection(): RootConnectionResilienceProjection =
        RootConnectionResilienceProjection.initial()
    fun rootConnectionRetry(): RootConnectionResilienceProjection =
        rootConnectionResilienceProjection()
    fun addRootConnectionResilienceListener(
        listener: (RootConnectionResilienceProjection) -> Unit,
    ): AutoCloseable = AutoCloseable {}
    fun authDeviceAgentSession(): JSONObject
    fun authBrowserStart(): JSONObject
    fun authBrowserReopen(params: JSONObject): JSONObject
    fun authBrowserCancel(params: JSONObject): JSONObject
    fun authBrowserPoll(params: JSONObject): JSONObject
    fun authProfileUpdate(displayName: String): JSONObject =
        error("Account profile mutation is unavailable on this Coordinator port")
    fun authLogout(): JSONObject
    fun humanCallCreate(
        callId: String,
        requestId: String,
        scopeId: String,
        participantIds: List<String>,
    ): AndroidHumanCallSession = error("Human call runtime is unavailable on this Coordinator port")
    fun humanCallList(scopeId: String, limit: Int = 20): List<AndroidHumanCallSession> =
        error("Human call runtime is unavailable on this Coordinator port")
    fun humanCallLease(
        callId: String,
        generation: Long,
        allowClaim: Boolean,
    ): AndroidHumanCallLease = error("Human call transport lease is unavailable on this Coordinator port")
    fun humanCallTransition(
        callId: String,
        generation: Long,
        action: String,
        terminalReason: String? = null,
    ): AndroidHumanCallSession = error("Human call transition is unavailable on this Coordinator port")
    fun humanCallUpdateMedia(
        callId: String,
        generation: Long,
        mediaCapabilities: JSONObject,
        deviceSelection: JSONObject,
    ): AndroidHumanCallSession = error("Human call media projection is unavailable on this Coordinator port")
    fun humanCallSendSignal(
        callId: String,
        generation: Long,
        seq: Long,
        kind: String,
        payload: JSONObject,
    ): AndroidHumanCallSignal = error("Human call signaling is unavailable on this Coordinator port")
    fun humanCallSignalsSince(
        callId: String,
        generation: Long,
        afterSeq: Long,
    ): List<AndroidHumanCallSignal> = error("Human call signaling is unavailable on this Coordinator port")
    fun humanCallReconcile(
        callId: String,
        generation: Long,
        terminalState: String,
        reason: String? = null,
    ): AndroidHumanCallSession = error("Human call reconciliation is unavailable on this Coordinator port")

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
    fun sendAcknowledgementBeginWithAttachments(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        richText: JSONObject? = null,
        attachments: List<AndroidAgentAttachment>,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement {
        if (attachments.isNotEmpty() || richText != null) {
            error("Agent attachment/rich-text acknowledgement is unavailable on this Coordinator port")
        }
        return sendAcknowledgementBegin(accountSlot, agentId, nonce, text, entries)
    }
    fun sendAcknowledgementBeginWithModelAndAttachments(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        richText: JSONObject? = null,
        model: String?,
        attachments: List<AndroidAgentAttachment>,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement {
        if (!model.isNullOrBlank()) {
            error("Model-aware Agent send acknowledgement is unavailable on this Coordinator port")
        }
        return sendAcknowledgementBeginWithAttachments(
            accountSlot = accountSlot,
            agentId = agentId,
            nonce = nonce,
            text = text,
            richText = richText,
            attachments = attachments,
            entries = entries,
        )
    }

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

    fun sendAcknowledgementCancelQueued(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
    ): Boolean = error("Queued send cancellation is unavailable on this Coordinator port")

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
    fun composerDraftStageAttachment(
        accountSlot: String?,
        agentId: String,
        contentUri: String,
    ): AndroidComposerDraft =
        error("Durable composer attachment staging is unavailable on this Coordinator port")
    fun composerDraftRemoveAttachment(
        accountSlot: String?,
        agentId: String,
        attachmentId: String,
    ): AndroidComposerDraft? =
        error("Durable composer attachment removal is unavailable on this Coordinator port")
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
    fun transcriptPage(
        agentId: String,
        beforeSeq: Long? = null,
        limit: Int = 200,
    ): AndroidTranscriptHistoryPage =
        error("Transcript history pagination is unavailable on this Coordinator port")
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
    fun agentCreateWithPersona(
        name: String,
        description: String,
        avatarShape: String?,
        avatarColor: String?,
    ): JSONObject = agentCreate(name, description)
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
    fun agentUpdateAvatarImage(id: String, avatarDataUrl: String?): JSONObject =
        error("Agent avatar image updates are unavailable on this Coordinator port")
    fun agentGenerateAvatarImage(description: String): AndroidGeneratedAvatarImage =
        error("Agent avatar generation is unavailable on this Coordinator port")
    fun agentSetNotifyOnUpdates(id: String, isEnabled: Boolean): JSONObject =
        error("Agent notification settings are unavailable on this Coordinator port")
    fun agentSetHidden(id: String, isHidden: Boolean): JSONObject
    fun agentSetUnread(id: String, isUnread: Boolean): JSONObject
    fun agentDuplicate(id: String): JSONObject
    fun agentDelete(id: String): JSONObject
    fun agentPinnedIds(): List<String>
    fun agentSetPinned(ids: List<String>): List<String>
    fun agentSidebarSections(): AndroidSidebarSectionsSnapshot
    fun agentSetSidebarSections(
        base: AndroidSidebarSectionsSnapshot,
        sections: List<AndroidSidebarSection>,
    ): AndroidSidebarSectionsSnapshot

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

    fun rootNotificationTrays(): JSONArray
    fun dismissRootNotificationTray(id: String): Boolean
    fun clearRootNotificationTrays(): Boolean
    fun invokeRootNotificationDashboardAction(action: String, args: JSONObject): JSONObject

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
    fun remoteComputerClipboardPaste(text: String): JSONObject =
        error("remote_computer_clipboard_paste_not_implemented")
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
