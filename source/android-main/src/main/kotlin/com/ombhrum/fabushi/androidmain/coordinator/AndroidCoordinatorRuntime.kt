package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import android.os.StatFs
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessBlockReason
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessState
import com.ombhrum.fabushi.androidpreload.runtime.AccountEntitlementState
import com.ombhrum.fabushi.androidpreload.runtime.AccountPaymentState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRebuildState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRecoveryState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRosterLoadState
import com.ombhrum.fabushi.androidpreload.runtime.AccountTruthState
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidMcpOAuthCompletion
import com.ombhrum.fabushi.core.MahayanaHost
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/**
 * Process-scoped Android owner of the native Mahayana Host.
 *
 * Presentation/ViewModel code receives only [AndroidCoordinatorPort]; the native Host is no longer
 * constructed or closed by screens/ViewModels. The process runtime becomes the single lifecycle
 * owner and is the insertion point for reconnect/resync and process-death recovery.
 */
class AndroidCoordinatorRuntime private constructor(application: Application) : AndroidCoordinatorPort {
    private val epochStore = object : CoordinatorEpochStore {
        private val preferences = application.getSharedPreferences("fabushi-coordinator-runtime", 0)
        override fun read(): Long = preferences.getLong("generation", 0L)
        override fun write(value: Long) { preferences.edit().putLong("generation", value).apply() }
    }
    private val processRuntime = CoordinatorProcessRuntime(epochStore)
    val processGeneration: Long = processRuntime.start()
    private val host = MahayanaHost(application, processGeneration = processGeneration)
    private val accountAccessOwner = AccountAccessProjectionOwner(
        SharedPreferencesAccountAccessEpochStore(application),
    )
    private val agentRosterMutationOwner = AgentRosterMutationOwner(
        SharedPreferencesAgentRosterMutationStore(application),
    )
    private val featureEventListeners = CopyOnWriteArrayList<(JSONObject) -> Unit>()
    private val eventPumpRunning = AtomicBoolean(false)
    private val lastEventSequence = AtomicLong(0L)
    private val eventPumpExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-coordinator-feature-events").apply { isDaemon = true }
    }
    private val storagePressureExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-storage-pressure").apply { isDaemon = true }
    }

    init {
        runCatching { reconcileAgentRosterMutations() }
        storagePressureExecutor.scheduleWithFixedDelay(
            {
                runCatching {
                    val stats = StatFs(application.filesDir.absolutePath)
                    val totalBytes = stats.totalBytes
                    val availableBytes = stats.availableBytes
                    if (totalBytes > 0L && availableBytes >= 0L && availableBytes <= totalBytes) {
                        host.request(
                            "feature.agent.diskPressure.observe",
                            JSONObject()
                                .put("totalBytes", totalBytes)
                                .put("availableBytes", availableBytes),
                        )
                    }
                }
            },
            0L,
            STORAGE_PRESSURE_SAMPLE_SECONDS,
            TimeUnit.SECONDS,
        )
    }

    override fun coordinatorStatus() = host.request("coordinator.status")

    override fun coordinatorResync(generation: Long, afterSequence: Long) =
        host.request(
            "coordinator.resync",
            JSONObject()
                .put("generation", generation)
                .put("afterSequence", afterSequence),
        )

    override fun mcpOAuthRegister(state: String, provider: String): Boolean =
        host.request(
            "coordinator.mcpOAuth.register",
            JSONObject()
                .put("state", state)
                .put("provider", provider),
        ).optBoolean("registered", false)

    override fun mcpOAuthRegisterBound(
        state: String,
        provider: String,
        serverId: String,
        accountKey: String,
        generation: Long,
    ): Boolean =
        host.request(
            "coordinator.mcpOAuth.register",
            JSONObject()
                .put("state", state)
                .put("provider", provider)
                .put("serverId", serverId)
                .put("accountKey", accountKey)
                .put("generation", generation),
        ).optBoolean("registered", false)

    override fun mcpOAuthComplete(
        state: String,
        code: String?,
        error: String?,
    ): AndroidMcpOAuthCompletion {
        require((code == null) xor (error == null)) {
            "MCP OAuth completion requires exactly one of code or error"
        }
        val result = host.request(
            "coordinator.mcpOAuth.complete",
            JSONObject()
                .put("state", state)
                .apply {
                    code?.let { put("code", it) }
                    error?.let { put("error", it) }
                },
        )
        return AndroidMcpOAuthCompletion(
            provider = result.getString("provider"),
            state = result.getString("state"),
            outcome = result.optString("outcome", "completed"),
        )
    }

    override fun authStatus(): JSONObject =
        host.request("feature.auth.status").also(::observeAccountAccessAuth)

    override fun accountAccessProjection(): AccountAccessProjection {
        val auth = authStatus()
        val token = accountAccessOwner.beginRefresh()
        if (!auth.optBoolean("loggedIn", false)) {
            return accountAccessOwner.settle(
                token,
                AccountAccessFacts(
                    loggedIn = false,
                    sessionSettled = true,
                    recoveryState = AccountRecoveryState.READY,
                ),
            )
        }

        val failures = mutableListOf<String>()
        var sandAccessState = AccountAccessState.UNKNOWN
        var sandAccessBlockReason = AccountAccessBlockReason.UNSPECIFIED
        runCatching { host.request("feature.account.sandAccess") }.onSuccess { access ->
            sandAccessState = when (access.optString("state")) {
                "checking" -> AccountAccessState.CHECKING
                "granted" -> AccountAccessState.GRANTED
                "unavailable" -> AccountAccessState.UNAVAILABLE
                "paymentRequired" -> AccountAccessState.PAYMENT_REQUIRED
                else -> AccountAccessState.UNKNOWN
            }
            sandAccessBlockReason = when (access.optString("reason")) {
                "none" -> AccountAccessBlockReason.NONE
                "teamPrivacyMode" -> AccountAccessBlockReason.TEAM_PRIVACY_MODE
                "teamSetupRequired" -> AccountAccessBlockReason.TEAM_SETUP_REQUIRED
                "teamAccessRequired" -> AccountAccessBlockReason.TEAM_ACCESS_REQUIRED
                "notOffered" -> AccountAccessBlockReason.NOT_OFFERED
                "freeTrialAvailable" -> AccountAccessBlockReason.FREE_TRIAL_AVAILABLE
                "paywallIndividual" -> AccountAccessBlockReason.PAYWALL_INDIVIDUAL
                "paywallTeamMember" -> AccountAccessBlockReason.PAYWALL_TEAM_MEMBER
                "paywallTeamAdmin" -> AccountAccessBlockReason.PAYWALL_TEAM_ADMIN
                else -> AccountAccessBlockReason.UNSPECIFIED
            }
        }.onFailure { error ->
            failures += "sand-access:" + (error.message ?: error::class.java.simpleName)
        }
        val privacyMode = runCatching {
            host.request("feature.account.privacyMode").optString("mode", "unknown")
        }.getOrElse { error ->
            failures += "privacy:" + (error.message ?: error::class.java.simpleName)
            "unknown"
        }
        val remote = runCatching { host.request("feature.remote.binding.status") }.getOrElse { error ->
            failures += "remote:" + (error.message ?: error::class.java.simpleName)
            null
        }
        val remoteReady = remote?.optBoolean("ready", false) == true
        val remoteHasDesktop = remoteReady && remote?.optBoolean("hasDesktop", false) == true

        var rosterLoadState = AccountRosterLoadState.LOADING
        var rosterFailureCode: String? = null
        var rosterFailureTransportKind: String? = null
        runCatching { host.requestValue("listAgents") }.onSuccess { value ->
            rosterLoadState = if (value is JSONArray) {
                AccountRosterLoadState.READY
            } else {
                failures += "roster:invalid-payload"
                rosterFailureCode = "roster-invalid-payload"
                AccountRosterLoadState.ERROR
            }
        }.onFailure { error ->
            val message = error.message.orEmpty().lowercase()
            rosterLoadState = AccountRosterLoadState.ERROR
            rosterFailureTransportKind = when {
                "dns" in message -> "dns"
                "network" in message || "connect" in message || "timeout" in message -> "network"
                else -> null
            }
            rosterFailureCode = "roster-load-failed"
            failures += "roster:" + (error.message ?: error::class.java.simpleName)
        }

        var entitlementState = AccountEntitlementState.UNKNOWN
        var entitlementReason: String? = null
        var paymentState = AccountPaymentState.UNKNOWN
        runCatching {
            host.request(
                "platform.request",
                JSONObject()
                    .put("method", "GET")
                    .put("path", "/v1/plugins/global-dharma/entitlements/local.prayer-wheel.start")
                    .put("authenticated", true),
            )
        }.onSuccess { response ->
            val access = response.optJSONObject("data")?.optJSONObject("access")
            if (access == null) {
                failures += "entitlement:missing-access-decision"
            } else {
                entitlementReason = access.optString("reason").trim().takeIf(String::isNotEmpty)
                val allowed = access.optBoolean("allowed", false)
                entitlementState = when {
                    allowed -> AccountEntitlementState.GRANTED
                    entitlementReason?.lowercase()?.let { "revok" in it || "refund" in it } == true ->
                        AccountEntitlementState.REVOKED
                    else -> AccountEntitlementState.DENIED
                }
                paymentState = when {
                    allowed -> AccountPaymentState.SETTLED
                    entitlementReason?.lowercase()?.contains("pending") == true -> AccountPaymentState.PENDING
                    entitlementState == AccountEntitlementState.DENIED -> AccountPaymentState.REQUIRED
                    else -> AccountPaymentState.UNKNOWN
                }
            }
        }.onFailure { error ->
            failures += "entitlement:" + (error.message ?: error::class.java.simpleName)
            paymentState = AccountPaymentState.OUTCOME_UNKNOWN
        }

        return accountAccessOwner.settle(
            token,
            AccountAccessFacts(
                loggedIn = true,
                // Current Desktop Fabushi shipping composition owns Sand access in the
                // product account adapter; the Host exposes that canonical policy over JNI.
                sandAccessState = sandAccessState,
                blockReason = sandAccessBlockReason,
                // Descriptor-account authorization and managed-team policy remain separate
                // owners and stay unknown until their exact shipping contracts are wired.
                authorizationState = AccountTruthState.UNKNOWN,
                paymentState = paymentState,
                entitlementState = entitlementState,
                entitlementReason = entitlementReason,
                privacyMode = privacyMode,
                teamPolicyState = AccountTruthState.UNKNOWN,
                remoteReady = remoteReady,
                remoteHasDesktop = remoteHasDesktop,
                sessionSettled = true,
                rebuildState = when {
                    remote == null -> AccountRebuildState.OUTCOME_UNKNOWN
                    remoteReady -> AccountRebuildState.IDLE
                    else -> AccountRebuildState.UNKNOWN
                },
                recoveryState = when {
                    failures.isNotEmpty() -> AccountRecoveryState.OUTCOME_UNKNOWN
                    remoteReady -> AccountRecoveryState.READY
                    else -> AccountRecoveryState.UNKNOWN
                },
                rosterLoadState = rosterLoadState,
                rosterFailureCode = rosterFailureCode,
                rosterFailureTransportKind = rosterFailureTransportKind,
                // Restored-roster is not inferred from a successful durable Host roster read.
                // It remains false until the canonical Host exposes an explicit restored snapshot.
                isShowingRestoredRoster = false,
                isRosterFetching = false,
                detail = failures.takeIf { it.isNotEmpty() }?.joinToString("; ")?.take(240),
            ),
        )
    }

    override fun authDeviceAgentSession() = host.request("feature.auth.deviceAgentSession")
    override fun authBrowserStart() = host.request("feature.auth.browserStart")
    override fun authBrowserReopen(params: JSONObject) = host.request("feature.auth.browserReopen", params)
    override fun authBrowserCancel(params: JSONObject) = host.request("feature.auth.browserCancel", params)
    override fun authBrowserPoll(params: JSONObject): JSONObject =
        host.request("feature.auth.browserPoll", params).also { result ->
            if (result.optString("status") == "completed") {
                result.optJSONObject("auth")?.let(::observeAccountAccessAuth)
            }
        }
    override fun authLogout(): JSONObject =
        host.request("feature.auth.logout").also(::observeAccountAccessAuth)

    private fun observeAccountAccessAuth(auth: JSONObject) {
        val user = auth.optJSONObject("user")
        val identity = auth.optString("authId").trim()
            .ifBlank { user?.optString("id").orEmpty().trim() }
            .ifBlank { user?.optString("email").orEmpty().trim() }
            .takeIf(String::isNotEmpty)
        accountAccessOwner.observeAuth(
            loggedIn = auth.optBoolean("loggedIn", false),
            identity = identity,
        )
    }
    override fun automationUpsert(params: JSONObject) = host.request("feature.automation.upsert", params)
    override fun automationList(): JSONArray = host.requestValue("feature.automation.list") as? JSONArray ?: JSONArray()
    override fun automationStart(params: JSONObject) = host.request("feature.automation.start", params)
    override fun automationAdvanceStep(params: JSONObject) = host.request("feature.automation.advanceStep", params)
    override fun automationAwaitApproval(params: JSONObject) = host.request("feature.automation.awaitApproval", params)
    override fun automationResolveApproval(params: JSONObject) = host.request("feature.automation.resolveApproval", params)
    override fun automationCancel(params: JSONObject) = host.request("feature.automation.cancel", params)
    override fun automationSettle(params: JSONObject) = host.request("feature.automation.settle", params)
    override fun automationSnapshot(params: JSONObject) = host.request("feature.automation.snapshot", params)

    private fun remoteBoxCapabilities(): RemoteBoxCapabilitySnapshot {
        val status = runCatching { host.request("feature.remote.binding.status") }.getOrNull()
            ?: return RemoteBoxCapabilitySnapshot.Unavailable
        val available = status.optBoolean("ready", false)
        return RemoteBoxCapabilitySnapshot(
            available = available,
            hasDesktop = available && status.optBoolean("hasDesktop", false),
        )
    }

    override fun featureExecute(params: JSONObject) =
        host.request(
            "feature.execute",
            AgentTurnCapabilityProjection.forFeatureExecute(
                params,
                remoteBox = remoteBoxCapabilities(),
            ),
        )
    override fun featureInterrupt(params: JSONObject) = host.request("feature.interrupt", params)
    override fun transcriptSnapshot(): JSONArray =
        host.requestValue("feature.transcript.snapshot") as? JSONArray ?: JSONArray()

    override fun assistantProjection(): JSONObject =
        host.request("feature.assistant.projection")

    override fun assistantMarkRead(): JSONObject =
        host.request("feature.assistant.markRead")

    override fun agentSubagentTool(params: JSONObject): JSONObject =
        host.request(
            "feature.agent.subagent.tool",
            AgentTurnCapabilityProjection.forSubagentTool(
                params,
                remoteBox = remoteBoxCapabilities(),
            ),
        )

    override fun agentSubagentReconcile(params: JSONObject): JSONObject =
        host.request("feature.agent.subagent.reconcile", params)

    override fun agentList(): JSONArray {
        runCatching { reconcileAgentRosterMutations() }
        return host.requestValue("listAgents") as? JSONArray ?: JSONArray()
    }

    private fun currentAgentRosterAccountFence(): String =
        host.request("feature.account.fence").getString("accountFence")

    private fun sendAgentRosterMutation(
        operation: PendingAgentRosterMutation,
    ): JSONObject =
        host.request(
            "feature.agent.rosterMutation",
            JSONObject()
                .put("operationId", operation.operationId)
                .put("accountFence", operation.accountFence)
                .put("mutation", JSONObject(operation.mutation.toString())),
        )

    private fun reconcileAgentRosterMutations(accountFence: String? = null) {
        val currentFence = accountFence ?: runCatching { currentAgentRosterAccountFence() }.getOrNull() ?: return
        agentRosterMutationOwner.pendingFor(currentFence).forEach { operation ->
            runCatching { sendAgentRosterMutation(operation) }
                .onSuccess { outcome ->
                    when (outcome.optString("status")) {
                        "completed", "rejected" -> agentRosterMutationOwner.settle(operation.operationId)
                    }
                }
        }
    }

    private fun durableAgentRosterMutation(mutation: JSONObject): Any? {
        val accountFence = currentAgentRosterAccountFence()
        reconcileAgentRosterMutations(accountFence)
        val operation = agentRosterMutationOwner.begin(accountFence, mutation)
        val outcome = sendAgentRosterMutation(operation)
        return when (outcome.optString("status")) {
            "completed" -> {
                agentRosterMutationOwner.settle(operation.operationId)
                outcome.opt("result")
            }
            "rejected" -> {
                agentRosterMutationOwner.settle(operation.operationId)
                throw IllegalStateException(
                    outcome.optString("error").ifBlank { "Agent roster mutation was rejected" },
                )
            }
            else -> throw IllegalStateException("Agent roster mutation settlement is unknown")
        }
    }

    override fun agentCreate(name: String, description: String): JSONObject =
        host.request(
            "createAgent",
            JSONObject()
                .put("name", name)
                .put("description", description)
                .put("origin", "user"),
        )

    override fun agentCreateGroup(name: String, description: String, memberIds: List<String>): JSONObject {
        val members = JSONArray()
        memberIds.forEach(members::put)
        return host.request(
            "createGroup",
            JSONObject()
                .put("name", name)
                .put("description", description)
                .put("memberAgentIds", members),
        )
    }

    override fun agentSetGroupMembers(id: String, memberIds: List<String>): JSONObject {
        val members = JSONArray()
        memberIds.forEach(members::put)
        return durableAgentRosterMutation(
            JSONObject()
                .put("kind", "group-members")
                .put("id", id)
                .put("memberIds", members),
        ) as? JSONObject ?: JSONObject()
    }

    override fun agentUpdate(id: String, name: String, description: String): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "update")
                .put("id", id)
                .put("name", name)
                .put("description", description),
        ) as? JSONObject ?: JSONObject()

    override fun agentSetHidden(id: String, isHidden: Boolean): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "hidden")
                .put("id", id)
                .put("value", isHidden),
        ) as? JSONObject ?: JSONObject()

    override fun agentSetUnread(id: String, isUnread: Boolean): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "unread")
                .put("id", id)
                .put("value", isUnread),
        ) as? JSONObject ?: JSONObject()

    override fun agentDuplicate(id: String): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "duplicate")
                .put("id", id),
        ) as? JSONObject ?: JSONObject()

    override fun agentDelete(id: String): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "delete")
                .put("ids", JSONArray().put(id)),
        ) as? JSONObject ?: JSONObject()

    override fun agentSetPinned(ids: List<String>): List<String> {
        val values = JSONArray()
        ids.forEach(values::put)
        val result = durableAgentRosterMutation(
            JSONObject()
                .put("kind", "pinned")
                .put("ids", values),
        ) as? JSONObject ?: JSONObject()
        val settled = result.optJSONArray("ids") ?: JSONArray()
        return buildList {
            for (index in 0 until settled.length()) {
                settled.optString(index).takeIf(String::isNotBlank)?.let(::add)
            }
        }
    }

    override fun marketplaceBrowse(params: JSONObject) = host.request("feature.marketplace.browse", params)
    override fun marketplaceRelease(params: JSONObject) = host.request("feature.marketplace.release", params)
    override fun pluginInstall(params: JSONObject) = host.request("feature.plugin.install", params)
    override fun pluginVariableFields(schema: JSONObject): JSONArray =
        host.request("feature.plugin.variables.fields", JSONObject().put("schema", schema))
            .optJSONArray("fields") ?: JSONArray()
    override fun pluginVariablesConfigure(params: JSONObject) =
        host.request("feature.plugin.variables.configure", params)
    override fun pluginUiDocument(params: JSONObject) = host.request("feature.plugin.uiDocument", params)
    override fun pluginCompatibility(params: JSONObject) = host.request("plugin.compatibility", params)
    override fun pluginPermissionGrant(params: JSONObject) = host.request("plugin.permission.grant", params)

    override fun runtimeStart(params: JSONObject) = host.request("runtime.start", params)
    override fun runtimeCallValue(params: JSONObject): Any? = host.requestValue("runtime.call", params)
    override fun runtimeCancel(params: JSONObject) = host.request("runtime.cancel", params)

    override fun messagingAccessIssue(params: JSONObject) = host.request("feature.messaging.access.issue", params)
    override fun messagingBlobRead(params: JSONObject) = host.request("feature.messaging.blob.read", params)
    override fun messagingExecute(params: JSONObject) = host.request("feature.messaging.execute", params)

    override fun platformRequest(params: JSONObject) = host.request("platform.request", params)
    override fun webAuthnRegisterProvider() = host.request("feature.webauthn.registerProvider")
    override fun webAuthnUnregisterProvider(params: JSONObject) = host.request("feature.webauthn.unregisterProvider", params)
    override fun webAuthnPollRequest(params: JSONObject) = host.request("feature.webauthn.pollRequest", params)
    override fun webAuthnSubmitResponses(params: JSONObject) = host.request("feature.webauthn.submitResponses", params)

    override fun publishFeatureEvent(event: JSONObject) {
        val metadata = runCatching {
            host.request(
                "coordinator.publishEvent",
                JSONObject().put("event", JSONObject(event.toString())),
            )
        }.getOrNull()
        val projected = JSONObject(event.toString())
        if (metadata != null) {
            projected.put("_coordinator", JSONObject(metadata.toString()))
        }
        dispatchFeatureEvent(projected)
    }

    override fun addFeatureEventListener(listener: (JSONObject) -> Unit): AutoCloseable {
        featureEventListeners += listener
        ensureFeatureEventPump()
        return AutoCloseable { featureEventListeners.remove(listener) }
    }

    private fun ensureFeatureEventPump() {
        if (featureEventListeners.isEmpty()) return
        if (!eventPumpRunning.compareAndSet(false, true)) return
        eventPumpExecutor.execute {
            try {
                runCatching { replayCoordinatorEvents() }
                .onFailure { Thread.sleep(20) }
                while (featureEventListeners.isNotEmpty()) {
                    val event = try {
                        host.request(
                            "feature.receive",
                            JSONObject().put("timeoutMs", 250),
                        )
                    } catch (_: Throwable) {
                        runCatching { replayCoordinatorEvents() }
                        Thread.sleep(100)
                        continue
                    }
                    if (event.optString("type").isBlank()) {
                        Thread.sleep(20)
                        continue
                    }
                    dispatchFeatureEvent(event)
                }
            } finally {
                eventPumpRunning.set(false)
                if (featureEventListeners.isNotEmpty()) ensureFeatureEventPump()
            }
        }
    }

    private fun replayCoordinatorEvents() {
        val status = coordinatorStatus()
        val generation = status.optLong("generation", -1L)
        check(generation == processGeneration) {
            "Coordinator generation mismatch: expected $processGeneration, got $generation"
        }
        val snapshot = coordinatorResync(generation, lastEventSequence.get())
        val events = snapshot.optJSONArray("events") ?: JSONArray()
        for (index in 0 until events.length()) {
            val envelope = events.optJSONObject(index) ?: continue
            val sequence = envelope.optLong("sequence", -1L)
            val payload = envelope.optJSONObject("payload") ?: continue
            val projected = JSONObject(payload.toString()).put(
                "_coordinator",
                JSONObject()
                    .put("generation", generation)
                    .put("sequence", sequence)
                    .put("eventId", envelope.optString("eventId")),
            )
            dispatchFeatureEvent(projected)
        }
    }

    private fun acceptCoordinatorEvent(event: JSONObject): Boolean {
        val metadata = event.optJSONObject("_coordinator") ?: return true
        val generation = metadata.optLong("generation", -1L)
        val sequence = metadata.optLong("sequence", -1L)
        if (generation != processGeneration || sequence <= 0L) return false
        while (true) {
            val current = lastEventSequence.get()
            if (sequence <= current) return false
            if (lastEventSequence.compareAndSet(current, sequence)) return true
        }
    }

    private fun dispatchFeatureEvent(event: JSONObject) {
        if (!acceptCoordinatorEvent(event)) return
        val serialized = event.toString()
        featureEventListeners.forEach { listener ->
            runCatching { listener(JSONObject(serialized)) }
        }
    }

    companion object {
        private const val STORAGE_PRESSURE_SAMPLE_SECONDS = 60L
        @Volatile private var instance: AndroidCoordinatorRuntime? = null

        fun get(application: Application): AndroidCoordinatorRuntime =
            instance ?: synchronized(this) {
                instance ?: AndroidCoordinatorRuntime(application).also { instance = it }
            }
    }
}
