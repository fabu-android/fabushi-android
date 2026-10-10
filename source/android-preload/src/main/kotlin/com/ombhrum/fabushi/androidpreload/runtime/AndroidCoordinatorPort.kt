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
    fun featureInterrupt(params: JSONObject): JSONObject
    fun featureApprovalResolve(params: JSONObject): JSONObject
    fun transcriptSnapshot(): JSONArray
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
        avatarShape: String?,
        avatarColor: String?,
    ): JSONObject = agentUpdate(id, name, description)
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
    fun computerRebuildStatus(): JSONObject =
        error("computer_rebuild_status_not_implemented")
    fun webAuthnRegisterProvider(): JSONObject
    fun webAuthnUnregisterProvider(params: JSONObject): JSONObject
    fun webAuthnPollRequest(params: JSONObject): JSONObject
    fun webAuthnSubmitResponses(params: JSONObject): JSONObject
    fun publishFeatureEvent(event: JSONObject)
    fun addFeatureEventListener(listener: (JSONObject) -> Unit): AutoCloseable
}
