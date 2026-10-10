package com.ombhrum.fabushi.androidpreload.runtime

import org.json.JSONArray
import org.json.JSONObject

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
    fun transcriptSnapshot(): JSONArray
    fun agentSubagentTool(params: JSONObject): JSONObject
    fun agentSubagentReconcile(params: JSONObject): JSONObject

    fun agentList(): JSONArray
    fun agentCreate(name: String, description: String): JSONObject
    fun agentUpdate(id: String, name: String, description: String): JSONObject
    fun agentSetHidden(id: String, isHidden: Boolean): JSONObject
    fun agentSetUnread(id: String, isUnread: Boolean): JSONObject
    fun agentDuplicate(id: String): JSONObject
    fun agentDelete(id: String): JSONObject
    fun agentSetPinned(ids: List<String>): List<String>

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
    fun webAuthnRegisterProvider(): JSONObject
    fun webAuthnUnregisterProvider(params: JSONObject): JSONObject
    fun webAuthnPollRequest(params: JSONObject): JSONObject
    fun webAuthnSubmitResponses(params: JSONObject): JSONObject
    fun publishFeatureEvent(event: JSONObject)
    fun addFeatureEventListener(listener: (JSONObject) -> Unit): AutoCloseable
}
