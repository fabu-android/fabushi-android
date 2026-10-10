package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import android.content.Context
import android.view.View
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
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import com.ombhrum.fabushi.androidmain.security.AndroidRemoteControlSessionStore
import com.ombhrum.fabushi.androidmain.security.AndroidRemotePairingStore
import com.ombhrum.fabushi.androidmain.security.RemoteControlSessionCredential
import com.ombhrum.fabushi.androidmain.security.RemoteControlSessionLifecycle
import com.ombhrum.fabushi.androidmain.security.RemotePairingCredential
import com.ombhrum.fabushi.androidmain.remote.AndroidRemoteComputerDataPlane
import com.ombhrum.fabushi.androidmain.remote.RemoteComputerDataPlaneFence
import com.ombhrum.fabushi.core.MahayanaHost
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.UUID

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
    private val remotePairingStore = AndroidRemotePairingStore(application)
    private val remoteControlSessionStore = AndroidRemoteControlSessionStore(application)
    private val remoteCreateIntentPreferences =
        application.getSharedPreferences("fabushi-remote-create-intent-v1", 0)
    private val applicationContext = application.applicationContext
    private val remoteComputerDataPlane by lazy {
        AndroidRemoteComputerDataPlane(
            context = applicationContext,
            currentFence = { runCatching { currentRemoteDataPlaneFence() }.getOrNull() },
            sendSignal = { kind, payload ->
                val fence = currentRemoteDataPlaneFence()
                    ?: error("Remote Computer data-plane session is unavailable")
                remoteComputerSignal(fence.deviceId, fence.sessionId, kind, payload)
            },
            drainSignals = {
                val fence = currentRemoteDataPlaneFence()
                    ?: error("Remote Computer data-plane session is unavailable")
                val session = requireRemoteControlSession(fence.deviceId, fence.sessionId)
                remoteComputerSignalDrain(
                    fence.deviceId,
                    fence.sessionId,
                    session.lastAcknowledgedSignalId,
                ).optJSONArray("signals") ?: JSONArray()
            },
            acknowledgeSignals = { lastSignalId ->
                val fence = currentRemoteDataPlaneFence()
                    ?: error("Remote Computer data-plane session is unavailable")
                remoteComputerSignalAcknowledge(fence.deviceId, fence.sessionId, lastSignalId)
            },
            onReconnectRequired = { disconnectedFence ->
                scheduleRemoteComputerReconnect(disconnectedFence)
            },
            onRemoteClose = {
                // The authoritative signal-drain path marks the durable session outcome unknown
                // before this callback. Do not emit another close mutation from the data plane.
            },
        )
    }
    // Independent shared-Host consumer for process-owned rebuild/transport state. MahayanaHost
    // fans each native event into every registered consumer queue, so this owner may drain
    // continuously without stealing approvals/chat/tool events from Presentation.
    private val computerRebuildEventHost = MahayanaHost(application, processGeneration = processGeneration)
    private val accountAccessOwner = AccountAccessProjectionOwner(
        SharedPreferencesAccountAccessEpochStore(application),
    )
    private val computerRebuildOwner = ComputerRebuildStateOwner(
        SharedPreferencesComputerRebuildStateStore(application),
        processGeneration = processGeneration,
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
    private val computerRebuildEventExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-computer-rebuild-events").apply { isDaemon = true }
    }
    private val computerRebuildMigrationExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-computer-rebuild-migration").apply { isDaemon = true }
    }
    private val remoteComputerReconnectExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-remote-computer-reconnect").apply { isDaemon = true }
    }
    private val computerRebuildMigrationRunning = AtomicBoolean(false)
    private val storagePressureExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-storage-pressure").apply { isDaemon = true }
    }

    init {
        computerRebuildOwner.observeAccount(accountAccessOwner.currentProjection().accountEpoch)
        startComputerRebuildEventPump()
        resumeComputerRebuildMigrationIfNeeded()
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
        var teamPolicyState = AccountTruthState.UNKNOWN
        runCatching { host.request("feature.account.teamRules") }.onSuccess { response ->
            if (response.opt("rules") is JSONArray) {
                // Team rules are an enforced policy input, not an authorization decision.
                // A successful canonical fetch means policy ownership is settled for this refresh.
                teamPolicyState = AccountTruthState.GRANTED
            } else {
                failures += "team-policy:invalid-payload"
            }
        }.onFailure { error ->
            failures += "team-policy:" + (error.message ?: error::class.java.simpleName)
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

        val rebuildState = runCatching {
            computerRebuildOwner.accountProjection(accountAccessOwner.currentProjection().accountEpoch)
        }.getOrElse { error ->
            failures += "computer-rebuild:" + (error.message ?: error::class.java.simpleName)
            AccountRebuildState.UNKNOWN
        }

        return accountAccessOwner.settle(
            token,
            AccountAccessFacts(
                loggedIn = true,
                // Current Desktop Fabushi shipping composition owns Sand access in the
                // product account adapter; the Host exposes that canonical policy over JNI.
                sandAccessState = sandAccessState,
                blockReason = sandAccessBlockReason,
                // Descriptor-account authorization remains a separate owner and stays unknown
                // until its exact shipping contract is wired.
                authorizationState = AccountTruthState.UNKNOWN,
                paymentState = paymentState,
                entitlementState = entitlementState,
                entitlementReason = entitlementReason,
                privacyMode = privacyMode,
                teamPolicyState = teamPolicyState,
                remoteReady = remoteReady,
                remoteHasDesktop = remoteHasDesktop,
                sessionSettled = true,
                // Canonical rebuild state comes only from ComputerRebuildStateOwner. Remote
                // binding readiness remains a separate execution-capability fact.
                rebuildState = rebuildState,
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
        val previousEpoch = accountAccessOwner.currentProjection().accountEpoch
        val user = auth.optJSONObject("user")
        val identity = auth.optString("authId").trim()
            .ifBlank { user?.optString("id").orEmpty().trim() }
            .ifBlank { user?.optString("email").orEmpty().trim() }
            .takeIf(String::isNotEmpty)
        val loggedIn = auth.optBoolean("loggedIn", false)
        accountAccessOwner.observeAuth(
            loggedIn = loggedIn,
            identity = identity,
        )
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        if (!loggedIn || currentEpoch != previousEpoch) {
            remoteControlSessionStore.clear()
            remotePairingStore.clear()
            runCatching { host.clearProtectedRemoteBinding() }
        }
        computerRebuildOwner.observeAccount(currentEpoch)
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
        return RemoteBoxCapabilitySnapshot.fromBindingStatus(status)
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
    override fun featureApprovalResolve(params: JSONObject) = host.request("feature.approval.resolve", params)
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

    override fun agentSidebarSections(): List<AndroidSidebarSection> =
        parseSidebarSections(host.request("feature.agent.sidebarSections").optJSONArray("sections"))

    override fun agentSetSidebarSections(
        sections: List<AndroidSidebarSection>,
    ): List<AndroidSidebarSection> {
        val values = JSONArray()
        sections.forEach { section ->
            val agentIds = JSONArray()
            section.agentIds.forEach(agentIds::put)
            values.put(
                JSONObject()
                    .put("id", section.id)
                    .put("name", section.name)
                    .put("agentIds", agentIds)
                    .put("isCollapsed", false),
            )
        }
        val result = durableAgentRosterMutation(
            JSONObject()
                .put("kind", "sidebar-sections")
                .put("sections", values),
        ) as? JSONObject ?: JSONObject()
        return parseSidebarSections(result.optJSONArray("sections"))
    }

    private fun parseSidebarSections(values: JSONArray?): List<AndroidSidebarSection> {
        if (values == null) return emptyList()
        return buildList {
            for (index in 0 until values.length()) {
                val section = values.optJSONObject(index) ?: continue
                val id = section.optString("id").trim()
                if (id.isBlank()) continue
                val agentIds = buildList {
                    val raw = section.optJSONArray("agentIds") ?: JSONArray()
                    for (agentIndex in 0 until raw.length()) {
                        raw.optString(agentIndex).trim().takeIf(String::isNotBlank)?.let(::add)
                    }
                }
                add(
                    AndroidSidebarSection(
                        id = id,
                        name = section.optString("name"),
                        agentIds = agentIds,
                    ),
                )
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

    override fun remoteComputerList(): JSONObject =
        authenticatedRemotePlatformRequest("GET", "/v1/computers")

    override fun remoteComputerPair(pairingCode: String, label: String): JSONObject {
        val normalizedCode = pairingCode.trim().uppercase()
        require(normalizedCode.length == 12 && normalizedCode.all { it.isDigit() || it in 'A'..'F' }) {
            "Remote pairing code is invalid"
        }
        val normalizedLabel = label.trim()
        require(normalizedLabel.isNotEmpty() && normalizedLabel.length <= 80) {
            "Remote pairing label is invalid"
        }
        val currentFence = host.request("feature.account.fence").getString("accountFence")
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        require(currentEpoch > 0L) { "Remote pairing requires a settled account epoch" }
        val data = authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/pair",
            JSONObject()
                .put("pairingCode", normalizedCode)
                .put("label", normalizedLabel),
        )
        val credential = RemotePairingCredential(
            deviceId = boundedRemoteIdentifier(data.getString("deviceId"), "deviceId"),
            clientId = boundedRemoteIdentifier(data.getString("clientId"), "clientId"),
            clientToken = data.getString("clientToken"),
            accountFence = currentFence,
            accountEpoch = currentEpoch,
        )
        // A newly paired client supersedes any short-lived control session owned by this
        // Android process. The long-lived clientToken remains confined to pairing storage.
        remoteControlSessionStore.clear()
        remotePairingStore.write(RemotePairingCredential.parse(credential.toSecretJson()))
        return JSONObject()
            .put("deviceId", credential.deviceId)
            .put("clientId", credential.clientId)
            .put("computerLabel", data.optString("computerLabel"))
            .put("clientLabel", data.optString("clientLabel"))
            .put("pairedAt", data.optLong("pairedAt"))
            .put("accountEpoch", credential.accountEpoch)
    }

    override fun remoteComputerRevoke(deviceId: String, clientId: String): JSONObject {
        val safeDeviceId = boundedRemoteIdentifier(deviceId, "deviceId")
        val safeClientId = boundedRemoteIdentifier(clientId, "clientId")
        val currentFence = host.request("feature.account.fence").getString("accountFence")
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        val stored = remotePairingStore.readForAccountFence(currentFence, currentEpoch)
        val activeSession = remoteControlSessionStore.readForAccountFence(currentFence, currentEpoch)
        if (activeSession != null &&
            activeSession.deviceId == safeDeviceId &&
            activeSession.clientId == safeClientId
        ) {
            remoteControlSessionStore.clear()
        }
        if (stored != null && stored.deviceId == safeDeviceId && stored.clientId == safeClientId) {
            remotePairingStore.clear()
        }
        // Revoke every local execution credential before the remote mutation. A network failure
        // must leave this Android process fail-closed even when the server revoke is still pending.
        host.clearProtectedRemoteBinding()
        return authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/$safeDeviceId/clients/$safeClientId/revoke",
        )
    }

    override fun remoteComputerPairingStatus(): JSONObject {
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val pairing = remotePairingStore.readForAccountFence(currentFence, currentEpoch)
            ?: return JSONObject().put("paired", false)
        return pairing.publicProjection().put("paired", true)
    }

    override fun remoteComputerSessionCreate(deviceId: String): JSONObject {
        val safeDeviceId = boundedRemoteIdentifier(deviceId, "deviceId")
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val pairing = remotePairingStore.readForAccountFence(currentFence, currentEpoch)
            ?: error("Remote Computer is not paired for the current account")
        require(pairing.deviceId == safeDeviceId) { "Remote Computer pairing does not match device" }
        val requestId = reserveRemoteControlCreateRequest(pairing, currentEpoch)
        val data = authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/" + safeDeviceId + "/sessions",
            remoteControlSessionRecoveryBody(pairing, requestId),
        )
        return persistRemoteControlSessionResponse(
            pairing = pairing,
            currentFence = currentFence,
            currentEpoch = currentEpoch,
            requestId = requestId,
            data = data,
        )
    }

    override fun remoteComputerSessionReconcile(): JSONObject {
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val pending = currentRemoteControlCreateRequest(currentEpoch)
            ?: return remoteComputerSessionStatus()
        val pairing = remotePairingStore.readForAccountFence(currentFence, currentEpoch)
            ?: error("Remote Computer create outcome cannot be reconciled without current pairing")
        require(pairing.deviceId == pending.first) {
            "Remote Computer create outcome belongs to a different device"
        }
        require(remoteCreateIntentPreferences.getString("clientId", null) == pairing.clientId) {
            "Remote Computer create outcome belongs to a different paired client"
        }
        val requestId = pending.second
        val data = authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/" + pairing.deviceId + "/sessions/reconcile",
            remoteControlSessionRecoveryBody(pairing, requestId),
        )
        return persistRemoteControlSessionResponse(
            pairing = pairing,
            currentFence = currentFence,
            currentEpoch = currentEpoch,
            requestId = requestId,
            data = data,
        )
    }

    override fun remoteComputerSessionStatus(): JSONObject {
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val session = remoteControlSessionStore.bindProcess(
            currentFence,
            currentEpoch,
            processGeneration,
        ) ?: run {
            val pending = currentRemoteControlCreateRequest(currentEpoch)
            return JSONObject()
                .put("stored", false)
                .put("createOutcomeUnknown", pending != null)
                .put("pendingDeviceId", pending?.first ?: JSONObject.NULL)
        }
        if (session.expiresAt <= System.currentTimeMillis() / 1_000L) {
            remoteControlSessionStore.clear()
            return JSONObject().put("stored", false).put("reason", "expired")
        }
        return session.publicProjection().put("stored", true)
    }

    override fun remoteComputerSessionTransport(
        deviceId: String,
        sessionId: String,
        directAvailable: Boolean,
        relayRegion: String?,
    ): JSONObject {
        val session = requireRemoteControlSession(deviceId, sessionId)
        if (
            session.selectedRoute == "relay" &&
            session.routePolicy == "direct-preferred" &&
            directAvailable
        ) {
            error("Remote control session is locked to relay after direct transport fallback")
        }
        val region = relayRegion?.trim()?.takeIf(String::isNotEmpty)
        if (region != null) {
            require(region.length <= 32 && region.all { it.isLetterOrDigit() || it == '-' || it == '_' }) {
                "Remote relay region is invalid"
            }
        }
        val body = JSONObject()
            .put("role", "mobile")
            .put("clientId", session.clientId)
            .put("mobileToken", session.mobileToken)
            .put("directAvailable", directAvailable)
        region?.let { body.put("relayRegion", it) }
        val response = authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/" + session.deviceId + "/sessions/" + session.sessionId + "/transport",
            body,
        )
        require(
            boundedRemoteIdentifier(response.getString("sessionId"), "sessionId") == session.sessionId,
        ) {
            "Remote control transport session identity mismatch"
        }
        val provider = boundedRemoteIdentifier(response.getString("provider"), "provider")
        val routePolicy = response.getString("routePolicy")
        val selectedRoute = response.getString("selectedRoute")
        val returnedRegion = if (response.isNull("relayRegion")) {
            null
        } else {
            response.optString("relayRegion").trim().takeIf(String::isNotEmpty)
        }
        val transportUpdatedAt = response.getLong("transportUpdatedAt")
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val updated = remoteControlSessionStore.recordTransport(
            currentFence,
            currentEpoch,
            session.sessionId,
            provider,
            routePolicy,
            selectedRoute,
            returnedRegion,
            transportUpdatedAt,
            processGeneration,
        )
        return JSONObject(response.toString())
            .put("selectedRoute", updated.selectedRoute)
            .put("relayRegion", updated.relayRegion ?: JSONObject.NULL)
    }

    override fun remoteComputerSignal(
        deviceId: String,
        sessionId: String,
        kind: String,
        payload: JSONObject,
    ): JSONObject {
        val session = requireRemoteControlSession(deviceId, sessionId)
        val normalizedKind = kind.trim()
        require(normalizedKind in setOf("offer", "ice", "ready", "close")) {
            "Remote mobile signal kind is invalid"
        }
        val payloadCopy = JSONObject(payload.toString())
        require(payloadCopy.toString().toByteArray(Charsets.UTF_8).size <= 256 * 1024) {
            "Remote signal payload exceeds 256 KiB"
        }
        return authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/" + session.deviceId + "/signals",
            JSONObject()
                .put("sessionId", session.sessionId)
                .put("senderRole", "mobile")
                .put("clientId", session.clientId)
                .put("mobileToken", session.mobileToken)
                .put("kind", normalizedKind)
                .put("payload", payloadCopy),
        )
    }

    override fun remoteComputerSignalDrain(
        deviceId: String,
        sessionId: String,
        afterSignalId: Long,
    ): JSONObject {
        require(afterSignalId >= 0L) { "Remote signal cursor is invalid" }
        val session = requireRemoteControlSession(deviceId, sessionId)
        require(afterSignalId == session.lastAcknowledgedSignalId) {
            "Remote signal cursor must match the durable acknowledged cursor"
        }
        val response = authenticatedRemotePlatformRequest(
            "POST",
            "/v1/computers/" + session.deviceId + "/signals/drain",
            JSONObject()
                .put("sessionId", session.sessionId)
                .put("receiverRole", "mobile")
                .put("clientId", session.clientId)
                .put("mobileToken", session.mobileToken)
                .put("afterSignalId", afterSignalId),
        )
        require(
            boundedRemoteIdentifier(response.getString("sessionId"), "sessionId") == session.sessionId,
        ) {
            "Remote signal drain session identity mismatch"
        }
        val signals = response.optJSONArray("signals") ?: JSONArray()
        require(signals.length() <= 128) { "Remote signal drain exceeds server batch contract" }
        var previousSignalId = afterSignalId
        repeat(signals.length()) { index ->
            val signal = signals.getJSONObject(index)
            val signalId = signal.getLong("signalId")
            require(signalId > previousSignalId) { "Remote signal ids must be strictly increasing" }
            require(signal.optString("senderRole") == "desktop") {
                "Remote mobile drain accepted a non-desktop signal"
            }
            require(signal.optString("kind") in setOf("answer", "ice", "ready", "close")) {
                "Remote desktop signal kind is invalid"
            }
            previousSignalId = signalId
        }
        val lastSignalId = response.getLong("lastSignalId")
        require(lastSignalId == previousSignalId) {
            "Remote signal drain lastSignalId does not match the delivered batch"
        }
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        remoteControlSessionStore.recordSignalDrain(
            currentFence,
            currentEpoch,
            session.sessionId,
            afterSignalId,
            lastSignalId,
        )
        var sawReady = false
        var sawClose = false
        repeat(signals.length()) { index ->
            when (signals.getJSONObject(index).optString("kind")) {
                "ready" -> sawReady = true
                "close" -> sawClose = true
            }
        }
        val updated = remoteControlSessionStore.reconcileAfterSignalDrain(
            currentFence,
            currentEpoch,
            session.sessionId,
            sawReady,
            sawClose,
        )
        return JSONObject(response.toString())
            .put("lastAcknowledgedSignalId", updated.lastAcknowledgedSignalId)
            .put("highestDrainedSignalId", updated.highestDrainedSignalId)
            .put("lifecycle", updated.lifecycle.name.lowercase())
            .put("reconcileRequired", updated.reconcileRequired)
    }

    override fun remoteComputerSignalAcknowledge(
        deviceId: String,
        sessionId: String,
        lastSignalId: Long,
    ): JSONObject {
        require(lastSignalId >= 0L) { "Remote signal acknowledgement cursor is invalid" }
        val session = requireRemoteControlSession(deviceId, sessionId)
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val updated = remoteControlSessionStore.acknowledgeSignals(
            currentFence,
            currentEpoch,
            session.sessionId,
            lastSignalId,
        )
        return updated.publicProjection().put("acknowledged", true)
    }

    override fun remoteComputerDataPlaneConnect(
        deviceId: String,
        sessionId: String,
    ): JSONObject {
        val before = requireRemoteControlSession(deviceId, sessionId)
        remoteComputerSessionTransport(
            before.deviceId,
            before.sessionId,
            directAvailable = true,
        )
        val session = requireRemoteControlSession(deviceId, sessionId)
        require(session.iceServersJson != "[]") {
            "Remote Computer session has no protected ICE configuration; reconcile or create a fresh session"
        }
        val fence = session.toDataPlaneFence()
        remoteComputerDataPlane.connect(fence, session.iceServersJson)
        return session.publicProjection()
            .put("stored", true)
            .put("dataPlane", "negotiating")
            .put("nativeViewport", true)
    }

    override fun remoteComputerViewportView(context: Context): View =
        remoteComputerDataPlane.createViewport(context)

    override fun remoteComputerDataPlaneDisconnect(): JSONObject {
        remoteComputerDataPlane.disconnect()
        return JSONObject()
            .put("disconnected", true)
            .put("processGeneration", processGeneration)
    }

    override fun remoteComputerHumanTakeover(
        deviceId: String,
        sessionId: String,
        expectedViewportRevision: Long,
        active: Boolean,
    ): JSONObject {
        val session = requireRemoteControlSession(deviceId, sessionId)
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val updated = remoteControlSessionStore.setHumanTakeover(
            currentFence,
            currentEpoch,
            session.sessionId,
            expectedViewportRevision,
            active,
        )
        remoteComputerDataPlane.updateFence(updated.toDataPlaneFence())
        return updated.publicProjection().put("stored", true)
    }

    override fun remoteComputerViewportAdvance(
        deviceId: String,
        sessionId: String,
        expectedViewportRevision: Long,
    ): JSONObject {
        val session = requireRemoteControlSession(deviceId, sessionId)
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val updated = remoteControlSessionStore.advanceViewport(
            currentFence,
            currentEpoch,
            session.sessionId,
            expectedViewportRevision,
        )
        remoteComputerDataPlane.updateFence(updated.toDataPlaneFence())
        return updated.publicProjection().put("stored", true)
    }

    override fun remoteComputerSessionClose(deviceId: String, sessionId: String): JSONObject {
        val session = requireRemoteControlSession(deviceId, sessionId)
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        remoteControlSessionStore.beginClosing(currentFence, currentEpoch, session.sessionId)
        remoteComputerDataPlane.disconnect()
        return try {
            val response = authenticatedRemotePlatformRequest(
                "POST",
                "/v1/computers/" + session.deviceId + "/sessions/" + session.sessionId + "/close",
                JSONObject()
                    .put("role", "mobile")
                    .put("clientId", session.clientId)
                    .put("mobileToken", session.mobileToken),
            )
            remoteControlSessionStore.clear()
            JSONObject(response.toString())
                .put("stored", false)
                .put("reconcileRequired", false)
        } catch (error: Throwable) {
            remoteControlSessionStore.markCloseOutcomeUnknown(
                currentFence,
                currentEpoch,
                session.sessionId,
            )
            throw error
        }
    }

    override fun computerRebuildRequest(
        preserveData: Boolean,
        forceRecreate: Boolean,
    ): JSONObject {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        require(accountEpoch > 0L) { "Computer rebuild requires a settled account epoch" }
        val requestId = "android-rebuild-$processGeneration-" + UUID.randomUUID().toString()
        computerRebuildOwner.reserveRequest(accountEpoch, requestId)
        val reply = try {
            host.requestComputerRebuild(
                requestId = requestId,
                preserveData = preserveData,
                forceRecreate = forceRecreate,
            )
        } catch (error: Throwable) {
            runCatching { computerRebuildOwner.markRequestOutcomeUnknown(accountEpoch, requestId) }
            throw error
        }
        if (!reply.optBoolean("started", false)) {
            computerRebuildOwner.rejectRequest(accountEpoch, requestId)
            return JSONObject(reply.toString())
                .put("requestId", requestId)
                .put("accepted", false)
        }
        val operationId = reply.optString("operationId").trim()
        if (operationId.isEmpty()) {
            computerRebuildOwner.markRequestOutcomeUnknown(accountEpoch, requestId)
            error("Computer rebuild started without a stable operation identity")
        }
        computerRebuildOwner.acceptRequest(
            accountEpoch = accountEpoch,
            requestId = requestId,
            operationId = operationId,
            kind = if (forceRecreate || !preserveData) {
                ComputerRebuildKind.RESET
            } else {
                ComputerRebuildKind.UPDATE
            },
        )
        resumeComputerRebuildMigrationIfNeeded()
        return JSONObject(reply.toString())
            .put("requestId", requestId)
            .put("accepted", true)
    }

    override fun computerRebuildStatus(): JSONObject {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        val snapshot = computerRebuildOwner.snapshot(accountEpoch)
        return JSONObject()
            .put("accountEpoch", snapshot.accountEpoch)
            .put("processGeneration", snapshot.processGeneration)
            .put("requestId", snapshot.requestId ?: JSONObject.NULL)
            .put("operationId", snapshot.operationId ?: JSONObject.NULL)
            .put("migrationOffsetKey", snapshot.migrationOffsetKey)
            .put("pending", snapshot.pending)
            .put("acknowledged", snapshot.acknowledged)
            .put("outcomeUnknown", snapshot.outcomeUnknown)
            .put("kind", snapshot.kind?.name?.lowercase() ?: JSONObject.NULL)
            .put("lastResolution", snapshot.lastResolution?.name?.lowercase() ?: JSONObject.NULL)
    }

    private fun remoteControlSessionRecoveryBody(
        pairing: RemotePairingCredential,
        requestId: String,
    ): JSONObject =
        JSONObject()
            .put("clientId", pairing.clientId)
            .put("clientToken", pairing.clientToken)
            .put("requestId", requestId)

    private fun persistRemoteControlSessionResponse(
        pairing: RemotePairingCredential,
        currentFence: String,
        currentEpoch: Long,
        requestId: String,
        data: JSONObject,
    ): JSONObject {
        require(boundedRemoteIdentifier(data.getString("deviceId"), "deviceId") == pairing.deviceId) {
            "Remote control session device identity mismatch"
        }
        require(boundedRemoteIdentifier(data.getString("clientId"), "clientId") == pairing.clientId) {
            "Remote control session client identity mismatch"
        }
        val serverState = data.getString("state")
        val lifecycle = when (serverState) {
            "pending" -> RemoteControlSessionLifecycle.PENDING
            "active" -> RemoteControlSessionLifecycle.NEGOTIATING
            else -> error("Remote control session returned unsupported lifecycle state")
        }
        val credential = RemoteControlSessionCredential(
            deviceId = pairing.deviceId,
            clientId = pairing.clientId,
            sessionId = boundedRemoteIdentifier(data.getString("sessionId"), "sessionId"),
            mobileToken = data.getString("mobileToken"),
            requestId = requestId,
            iceServersJson = (data.optJSONArray("iceServers")
                ?: error("Remote control session is missing protected ICE configuration")).toString(),
            accountFence = currentFence,
            accountEpoch = currentEpoch,
            expiresAt = data.getLong("expiresAt"),
            processGeneration = processGeneration,
            lifecycle = lifecycle,
        )
        remoteControlSessionStore.write(RemoteControlSessionCredential.parse(credential.toSecretJson()))
        clearRemoteControlCreateRequest(requestId)
        return credential.publicProjection()
            .put("stored", true)
            .put("state", serverState)
            .put("createdAt", data.optLong("createdAt"))
            .put("reconciledCreateOutcome", true)
    }

    private fun reserveRemoteControlCreateRequest(
        pairing: RemotePairingCredential,
        accountEpoch: Long,
    ): String {
        val existing = currentRemoteControlCreateRequest(accountEpoch)
        if (existing != null &&
            existing.first == pairing.deviceId &&
            remoteCreateIntentPreferences.getString("clientId", null) == pairing.clientId
        ) {
            return existing.second
        }
        val requestId = "android-remote-" + UUID.randomUUID().toString()
        check(
            remoteCreateIntentPreferences.edit()
                .clear()
                .putLong("accountEpoch", accountEpoch)
                .putString("deviceId", pairing.deviceId)
                .putString("clientId", pairing.clientId)
                .putString("requestId", requestId)
                .commit(),
        ) { "Unable to persist Remote Computer create request identity" }
        return requestId
    }

    private fun currentRemoteControlCreateRequest(accountEpoch: Long): Pair<String, String>? {
        if (remoteCreateIntentPreferences.getLong("accountEpoch", -1L) != accountEpoch) {
            remoteCreateIntentPreferences.edit().clear().commit()
            return null
        }
        val deviceId = remoteCreateIntentPreferences.getString("deviceId", null)?.trim()
            ?.takeIf(String::isNotEmpty) ?: return null
        val requestId = remoteCreateIntentPreferences.getString("requestId", null)?.trim()
            ?.takeIf { it.length in 16..160 } ?: return null
        return deviceId to requestId
    }

    private fun clearRemoteControlCreateRequest(requestId: String) {
        if (remoteCreateIntentPreferences.getString("requestId", null) == requestId) {
            check(remoteCreateIntentPreferences.edit().clear().commit()) {
                "Unable to clear Remote Computer create request identity"
            }
        }
    }

    private fun scheduleRemoteComputerReconnect(disconnectedFence: RemoteComputerDataPlaneFence) {
        val (accountFence, accountEpoch) = runCatching { currentRemoteAccountFence() }.getOrNull() ?: return
        val reconnecting = runCatching {
            remoteControlSessionStore.markReconnectRequired(
                accountFence,
                accountEpoch,
                disconnectedFence.sessionId,
                disconnectedFence.processGeneration,
                disconnectedFence.viewportRevision,
            )
        }.getOrNull() ?: return
        val delaySeconds = minOf(30L, 1L shl minOf(5, reconnecting.reconnectCount.coerceAtLeast(1) - 1))
        remoteComputerReconnectExecutor.schedule(
            {
                runCatching {
                    val current = requireRemoteControlSession(
                        reconnecting.deviceId,
                        reconnecting.sessionId,
                    )
                    require(current.processGeneration == disconnectedFence.processGeneration) {
                        "Remote Computer reconnect crossed process generation"
                    }
                    require(current.viewportRevision == disconnectedFence.viewportRevision) {
                        "Remote Computer reconnect crossed viewport revision"
                    }
                    require(current.lifecycle == RemoteControlSessionLifecycle.RECONNECTING) {
                        "Remote Computer reconnect intent is no longer current"
                    }
                    require(current.expiresAt > System.currentTimeMillis() / 1_000L) {
                        "Remote Computer session expired before reconnect"
                    }
                    remoteComputerDataPlane.connect(
                        current.toDataPlaneFence(),
                        current.iceServersJson,
                    )
                }
            },
            delaySeconds,
            TimeUnit.SECONDS,
        )
    }

    private fun RemoteControlSessionCredential.toDataPlaneFence(): RemoteComputerDataPlaneFence =
        RemoteComputerDataPlaneFence(
            deviceId = deviceId,
            sessionId = sessionId,
            processGeneration = processGeneration,
            viewportRevision = viewportRevision,
            humanTakeover = humanTakeover,
            lifecycle = lifecycle.name.lowercase(),
        )

    private fun currentRemoteDataPlaneFence(): RemoteComputerDataPlaneFence? {
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val session = remoteControlSessionStore.readForAccountFence(currentFence, currentEpoch)
            ?: return null
        if (session.expiresAt <= System.currentTimeMillis() / 1_000L) return null
        if (session.processGeneration != processGeneration) return null
        return session.toDataPlaneFence()
    }

    private fun requireRemoteControlSession(
        deviceId: String,
        sessionId: String,
    ): RemoteControlSessionCredential {
        val safeDeviceId = boundedRemoteIdentifier(deviceId, "deviceId")
        val safeSessionId = boundedRemoteIdentifier(sessionId, "sessionId")
        val (currentFence, currentEpoch) = currentRemoteAccountFence()
        val session = remoteControlSessionStore.readForAccountFence(currentFence, currentEpoch)
            ?: error("Remote control session is unavailable")
        if (session.expiresAt <= System.currentTimeMillis() / 1_000L) {
            remoteControlSessionStore.clear()
            error("Remote control session expired")
        }
        require(session.deviceId == safeDeviceId && session.sessionId == safeSessionId) {
            "Remote control session identity mismatch"
        }
        val pairing = remotePairingStore.readForAccountFence(currentFence, currentEpoch)
            ?: run {
                remoteControlSessionStore.clear()
                error("Remote Computer pairing is unavailable")
            }
        require(pairing.deviceId == session.deviceId && pairing.clientId == session.clientId) {
            remoteControlSessionStore.clear()
            error("Remote control session no longer matches paired client")
        }
        return session
    }

    private fun currentRemoteAccountFence(): Pair<String, Long> {
        val currentFence = host.request("feature.account.fence").getString("accountFence")
        require(currentFence.isNotBlank()) { "Remote Computer requires an account fence" }
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        require(currentEpoch > 0L) { "Remote Computer requires a settled account epoch" }
        return currentFence to currentEpoch
    }

    private fun authenticatedRemotePlatformRequest(
        method: String,
        path: String,
        body: JSONObject? = null,
    ): JSONObject {
        val request = JSONObject()
            .put("authenticated", true)
            .put("method", method)
            .put("path", path)
        body?.let { request.put("body", JSONObject(it.toString())) }
        val response = platformRequest(request)
        check(response.optBoolean("ok", false)) { "Remote Computer platform request was not accepted" }
        return response.getJSONObject("data")
    }

    private fun boundedRemoteIdentifier(value: String, label: String): String {
        val normalized = value.trim()
        require(normalized.isNotEmpty() && normalized.length <= 160) { "Remote $label is invalid" }
        require(normalized.all { it.isLetterOrDigit() || it in setOf('-', '_', '.', ':') }) {
            "Remote $label contains unsupported path characters"
        }
        return normalized
    }

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

    private fun startComputerRebuildEventPump() {
        computerRebuildEventExecutor.execute {
            while (true) {
                val event = try {
                    computerRebuildEventHost.request(
                        "feature.receive",
                        JSONObject().put("timeoutMs", 250),
                    ).also {
                        observeComputerRebuildTransport(connected = true)
                    }
                } catch (_: Throwable) {
                    observeComputerRebuildTransport(connected = false)
                    Thread.sleep(100)
                    continue
                }
                if (event.optString("type").isNotBlank()) {
                    observeComputerRebuildEvent(event)
                }
            }
        }
    }

    private fun observeComputerRebuildTransport(connected: Boolean) {
        val epoch = accountAccessOwner.currentProjection().accountEpoch
        runCatching { computerRebuildOwner.observeConnection(epoch, connected) }
    }

    private fun observeComputerRebuildEvent(event: JSONObject) {
        val epoch = accountAccessOwner.currentProjection().accountEpoch
        when (event.optString("type")) {
            "forever-box" -> {
                val (boxId, phase) = projectForeverBoxRebuildEvent(event) ?: return
                runCatching { computerRebuildOwner.observeBox(epoch, boxId, phase) }
            }
            "dev-box-rebuild" -> {
                if (!isDevBoxRebuildStartEvent(event)) return
                runCatching {
                    computerRebuildOwner.setPending(epoch, true)
                    computerRebuildOwner.begin(
                        accountEpoch = epoch,
                        kind = ComputerRebuildKind.RECONNECTING,
                        operationId = null,
                        source = null,
                    )
                }
            }
            "box-migration" -> {
                val migration = projectBoxMigrationRebuildEvent(event) ?: return
                runCatching {
                    computerRebuildOwner.observeMigration(
                        accountEpoch = epoch,
                        operationId = migration.operationId,
                        phase = migration.phase,
                    )
                }
            }
        }
    }

    private fun resumeComputerRebuildMigrationIfNeeded() {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        val snapshot = runCatching { computerRebuildOwner.snapshot(accountEpoch) }.getOrNull() ?: return
        if (snapshot.operationId == null || snapshot.requestId == null || snapshot.kind == null) return
        if (!computerRebuildMigrationRunning.compareAndSet(false, true)) return
        val expectedEpoch = accountEpoch
        val expectedGeneration = processGeneration
        computerRebuildMigrationExecutor.execute {
            try {
                var yieldedAfterResume = false
                while (true) {
                    val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
                    if (currentEpoch != expectedEpoch) break
                    val current = runCatching { computerRebuildOwner.snapshot(expectedEpoch) }.getOrNull() ?: break
                    if (
                        current.processGeneration != expectedGeneration ||
                        current.operationId == null ||
                        current.requestId == null ||
                        current.kind == null
                    ) {
                        break
                    }
                    val event = try {
                        host.watchComputerRebuildOnce(
                            requestId = current.requestId + "-watch",
                            fromOffsetKey = current.migrationOffsetKey,
                        )
                    } catch (_: Throwable) {
                        if (accountAccessOwner.currentProjection().accountEpoch != expectedEpoch) break
                        if (!yieldedAfterResume && current.migrationOffsetKey.isNotEmpty()) {
                            // Desktop box-migration-watcher clears a persisted resume cursor only
                            // when the first resumed attach fails before yielding any event.
                            computerRebuildOwner.recordMigrationOffset(
                                expectedEpoch,
                                current.operationId,
                                "",
                            )
                        }
                        Thread.sleep(COMPUTER_REBUILD_RECONNECT_MS)
                        continue
                    }
                    if (accountAccessOwner.currentProjection().accountEpoch != expectedEpoch) break
                    val operationId = event.optString("operationId").trim().takeIf(String::isNotEmpty)
                    val offsetKey = event.optString("offsetKey")
                    // The migration stream is account-wide. Advancing the durable stream cursor is
                    // independent from allowing an event to mutate this operation's state.
                    computerRebuildOwner.recordMigrationOffset(
                        expectedEpoch,
                        current.operationId,
                        offsetKey,
                    )
                    yieldedAfterResume = true
                    if (operationId != current.operationId) continue
                    val phase = parseComputerRebuildMigrationPhase(event.optString("phase")) ?: continue
                    val after = computerRebuildOwner.observeMigration(expectedEpoch, operationId, phase)
                    if (phase == ComputerRebuildMigrationPhase.DONE && after.terminalMigration) {
                        computerRebuildOwner.deactivate(expectedEpoch)
                        break
                    }
                    if (phase == ComputerRebuildMigrationPhase.FAILED) break
                }
            } finally {
                computerRebuildMigrationRunning.set(false)
                val epoch = accountAccessOwner.currentProjection().accountEpoch
                val pending = runCatching { computerRebuildOwner.snapshot(epoch) }.getOrNull()
                if (pending?.operationId != null && pending.kind != null) {
                    resumeComputerRebuildMigrationIfNeeded()
                }
            }
        }
    }

    private fun parseComputerRebuildMigrationPhase(value: String): ComputerRebuildMigrationPhase? =
        when (value) {
            "backing-up" -> ComputerRebuildMigrationPhase.BACKING_UP
            "creating" -> ComputerRebuildMigrationPhase.CREATING
            "moving" -> ComputerRebuildMigrationPhase.MOVING
            "cleaning-up" -> ComputerRebuildMigrationPhase.CLEANING_UP
            "wiping" -> ComputerRebuildMigrationPhase.WIPING
            "done" -> ComputerRebuildMigrationPhase.DONE
            "failed" -> ComputerRebuildMigrationPhase.FAILED
            else -> null
        }

    companion object {
        private const val STORAGE_PRESSURE_SAMPLE_SECONDS = 60L
        private const val COMPUTER_REBUILD_RECONNECT_MS = 3_000L
        @Volatile private var instance: AndroidCoordinatorRuntime? = null

        fun get(application: Application): AndroidCoordinatorRuntime =
            instance ?: synchronized(this) {
                instance ?: AndroidCoordinatorRuntime(application).also { instance = it }
            }
    }
}
