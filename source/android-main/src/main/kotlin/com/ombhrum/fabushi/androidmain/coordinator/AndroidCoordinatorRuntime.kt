package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import android.content.Context
import android.net.Uri
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.provider.OpenableColumns
import android.util.Base64
import android.view.View
import android.os.StatFs
import android.os.Debug
import android.os.Process
import android.os.SystemClock
import com.ombhrum.fabushi.androidpreload.runtime.AndroidOnDemandUsage
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAccountUsageSummary
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPrReviewPreferences
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAutoReviewInstructions
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAutoReviewInstructionsSnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallSession
import com.ombhrum.fabushi.androidpreload.runtime.AndroidGeneratedAvatarImage
import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallLease
import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallSignal
import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallState
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessBlockReason
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessState
import com.ombhrum.fabushi.androidpreload.runtime.AccountEntitlementState
import com.ombhrum.fabushi.androidpreload.runtime.AccountPaymentState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRebuildState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRecoveryState
import com.ombhrum.fabushi.androidpreload.runtime.AccountRosterLoadState
import com.ombhrum.fabushi.androidpreload.runtime.AccountTruthState
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailabilitySnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidMandatoryUpdatePolicy
import com.ombhrum.fabushi.androidpreload.runtime.AndroidUpdateTrackPolicy
import com.ombhrum.fabushi.androidpreload.runtime.AndroidTimeZoneSettings
import com.ombhrum.fabushi.AndroidBackendUpdateSignal
import com.ombhrum.fabushi.AndroidMandatoryUpdateFacts
import com.ombhrum.fabushi.AndroidMandatoryUpdateOwner
import com.ombhrum.fabushi.SharedPreferencesMandatoryUpdateStateStore
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAgentAttachment
import com.ombhrum.fabushi.androidpreload.runtime.AndroidApprovalCleanupResult
import com.ombhrum.fabushi.androidpreload.runtime.AndroidClientSideToolV2Event
import com.ombhrum.fabushi.androidpreload.runtime.AndroidComposerDraft
import com.ombhrum.fabushi.androidpreload.runtime.AndroidConversationOutlineItem
import com.ombhrum.fabushi.androidpreload.runtime.parseAndroidConversationOutlineItems
import com.ombhrum.fabushi.androidpreload.runtime.AndroidMcpOAuthCompletion
import com.ombhrum.fabushi.androidpreload.runtime.AndroidToolResultCard
import com.ombhrum.fabushi.androidpreload.runtime.AndroidTranscriptHistoryPage
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementDispatch
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementEntry
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSectionsSnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidWidgetActionResult
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
import java.time.ZoneId
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
class AndroidCoordinatorRuntime private constructor(application: Application) : AndroidCoordinatorPort, AutoCloseable {
    private val epochStore = object : CoordinatorEpochStore {
        private val preferences = application.getSharedPreferences("fabushi-coordinator-runtime", 0)
        override fun read(): Long = preferences.getLong("generation", 0L)
        override fun write(value: Long) { preferences.edit().putLong("generation", value).apply() }
    }
    private val processRuntime = CoordinatorProcessRuntime(epochStore)
    private val closed = AtomicBoolean(false)
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
    private val accessBlockedTelemetryOwner = AccessBlockedTelemetryOwner()
    private val rootConnectionOwner = RootConnectionResilienceOwner()
    private val rootConnectionListeners =
        CopyOnWriteArrayList<(RootConnectionResilienceProjection) -> Unit>()
    private val rootConnectionRetryExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-root-connection-retry").apply { isDaemon = true }
    }
    private val mandatoryUpdateOwner = AndroidMandatoryUpdateOwner(
        SharedPreferencesMandatoryUpdateStateStore(application),
        processGeneration = processGeneration,
    )
    private val entrypointAvailabilityOwner =
        AndroidEntrypointAvailabilityOwner(processGeneration)
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
    private val computerRebuildQueueExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-computer-update-queue").apply { isDaemon = true }
    }
    private val remoteComputerReconnectExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-remote-computer-reconnect").apply { isDaemon = true }
    }
    private val computerRebuildMigrationRunning = AtomicBoolean(false)
    private val computerRebuildQueueRunning = AtomicBoolean(false)
    private val storagePressureExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-storage-pressure").apply { isDaemon = true }
    }
    private val processMetricsExecutor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-process-metrics").apply { isDaemon = true }
    }
    private val processSessionHeartbeatExecutor =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "fabushi-process-session-heartbeat").apply { isDaemon = true }
        }
    private val sendSubmissionExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-send-submission-queue").apply { isDaemon = true }
    }
    private val connectivityManager =
        applicationContext.getSystemService(ConnectivityManager::class.java)
    private val sendSubmissionQueue = AndroidSendSubmissionTransportQueue(
        isTransportDown = ::isSendTransportDown,
        queue = ::queueSendAcknowledgementDirect,
        dispatch = ::dispatchSendAcknowledgementDirect,
        cancelQueuedPersisted = ::cancelQueuedSendAcknowledgementDirect,
        onFlushed = ::publishFlushedSendAcknowledgement,
    )
    private val sendNetworkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            scheduleSendSubmissionFlush()
        }
    }

    init {
        host.request(
            "feature.settings.timeZone.detected",
            JSONObject().put("detectedTimeZone", ZoneId.systemDefault().id),
        )
        computerRebuildOwner.observeAccount(accountAccessOwner.currentProjection().accountEpoch)
        runCatching { connectivityManager.registerDefaultNetworkCallback(sendNetworkCallback) }
        startComputerRebuildEventPump()
        resumeComputerRebuildMigrationIfNeeded()
        resumeQueuedComputerUpdateIfNeeded()
        runCatching { reconcileAgentRosterMutations() }
        processSessionHeartbeatExecutor.scheduleWithFixedDelay(
            {
                runCatching {
                    host.request(
                        "feature.telemetry.session.heartbeat",
                        JSONObject().put("observedAtMs", System.currentTimeMillis()),
                    )
                }
            },
            PROCESS_SESSION_HEARTBEAT_SECONDS,
            PROCESS_SESSION_HEARTBEAT_SECONDS,
            TimeUnit.SECONDS,
        )
        processMetricsExecutor.scheduleWithFixedDelay(
            {
                runCatching {
                    val settings = host.request("feature.settings.processMetrics")
                    if (!settings.optBoolean("localEnabled", false)) {
                        return@runCatching
                    }
                    val memoryInfo = Debug.MemoryInfo()
                    Debug.getMemoryInfo(memoryInfo)
                    val memoryBytes = memoryInfo.totalPss.toLong().coerceAtLeast(0L) * 1024L
                    host.request(
                        "feature.telemetry.processMetrics.observe",
                        JSONObject()
                            .put("observedAtMs", SystemClock.elapsedRealtime())
                            .put("cpuTimeMsCumulative", Process.getElapsedCpuTime())
                            .put("memoryBytes", memoryBytes),
                    )
                }
            },
            PROCESS_METRICS_INITIAL_DELAY_SECONDS,
            PROCESS_METRICS_SAMPLE_SECONDS,
            TimeUnit.SECONDS,
        )
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

    override fun coordinatorStatus(): JSONObject {
        val status = host.request("coordinator.status")
        val recent = host.request(
            "feature.telemetry.processCrash.recent",
            JSONObject().put("limit", 20),
        ).optJSONArray("value") ?: JSONArray()
        var unclean: JSONObject? = null
        for (index in recent.length() - 1 downTo 0) {
            val entry = recent.optJSONObject(index) ?: continue
            if (entry.optString("kind") == "unclean_exit") {
                unclean = entry
                break
            }
        }
        val projected = unclean?.let { entry ->
            val observedAtMs = entry.optLong("occurredAtMs", 0L).coerceAtLeast(0L)
            val uptimeMs = entry.optLong("uptimeMs", 0L).coerceAtLeast(0L)
            JSONObject()
                .put("previousProcessId", JSONObject.NULL)
                .put("previousStartedAtMs", (observedAtMs - uptimeMs).coerceAtLeast(0L))
                .put("observedAtMs", observedAtMs)
                .put("crashSeen", entry.optBoolean("crashSeen", false))
        }
        return status.put("lastUncleanProcessSession", projected ?: JSONObject.NULL)
    }

    override fun coordinatorResync(generation: Long, afterSequence: Long) =
        host.request(
            "coordinator.resync",
            JSONObject()
                .put("generation", generation)
                .put("afterSequence", afterSequence),
        )

    override fun resolveUpdateTrackPolicy(
        managedTrack: String?,
        unlockInternalTracks: Boolean,
        userOverride: String?,
        buildDefault: String?,
    ): AndroidUpdateTrackPolicy {
        val projection = host.resolveUpdateTrackPolicy(
            managedTrack = managedTrack,
            unlockInternalTracks = unlockInternalTracks,
            userOverride = userOverride,
            buildDefault = buildDefault,
        )
        fun tracks(name: String): List<String> {
            val values = projection.optJSONArray(name) ?: error("Update-track policy omitted $name")
            return List(values.length()) { index ->
                values.getString(index).also { require(it.isNotBlank()) }
            }
        }
        val effectiveTrack = projection.getString("effectiveTrack")
        val managed = projection.optString("managedTrack").takeIf(String::isNotBlank)
        return AndroidUpdateTrackPolicy(
            managedTrack = managed,
            unlockInternalTracks = projection.optBoolean("unlockInternalTracks", false),
            effectiveTrack = effectiveTrack,
            selectableTracks = tracks("selectableTracks"),
            availableTracks = tracks("availableTracks"),
        )
    }

    override fun refreshMandatoryUpdatePolicy(): AndroidMandatoryUpdatePolicy {
        val auth = authStatus()
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        if (!auth.optBoolean("loggedIn", false)) {
            val cleared = mandatoryUpdateOwner.clearForLoggedOutAccount(accountEpoch)
            return AndroidMandatoryUpdatePolicy(
                accountEpoch = cleared.accountEpoch,
                processGeneration = processGeneration,
                minimumVersion = cleared.minimumVersion,
                backendRequiresUpdate = cleared.backendRequiresUpdate,
                autoUpdateWhenIdleGateObserved = cleared.autoUpdateWhenIdleGateObserved,
                autoUpdateWhenIdleGateEnabled = cleared.autoUpdateWhenIdleGateEnabled,
            )
        }

        val token = mandatoryUpdateOwner.begin(accountEpoch)
        val raw = runCatching {
            host.probeUpdatePolicy("android-update-policy-" + UUID.randomUUID())
        }.getOrElse {
            val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
            // Minimum-version and backend-required are durable producers, but the experiment gate
            // is live policy. If the Host/Statsig probe is unavailable, auto-staging must fail
            // closed instead of inheriting a previously observed true gate.
            val current = mandatoryUpdateOwner.settle(
                token = token,
                facts = AndroidMandatoryUpdateFacts(
                    minimumVersionObserved = false,
                    minimumVersion = null,
                    backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                    autoUpdateWhenIdleGateObserved = false,
                    autoUpdateWhenIdleGateEnabled = false,
                ),
                currentAccountEpoch = currentEpoch,
            )
            return AndroidMandatoryUpdatePolicy(
                accountEpoch = current.accountEpoch,
                processGeneration = processGeneration,
                minimumVersion = current.minimumVersion,
                backendRequiresUpdate = current.backendRequiresUpdate,
                autoUpdateWhenIdleGateObserved = false,
                autoUpdateWhenIdleGateEnabled = false,
            )
        }
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        val backendSignal = when (raw.optString("backendRequirement", "unchanged")) {
            "required" -> AndroidBackendUpdateSignal.REQUIRED
            "clear" -> AndroidBackendUpdateSignal.CLEAR
            else -> AndroidBackendUpdateSignal.UNCHANGED
        }
        val minimumVersion = (raw.opt("minimumVersion") as? String)
            ?.trim()
            ?.takeIf(String::isNotEmpty)
        val settled = mandatoryUpdateOwner.settle(
            token = token,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = raw.optBoolean("minimumVersionObserved", false),
                minimumVersion = minimumVersion,
                backendSignal = backendSignal,
                autoUpdateWhenIdleGateObserved =
                    raw.optBoolean("autoUpdateWhenIdleGateObserved", false),
                autoUpdateWhenIdleGateEnabled =
                    raw.optBoolean("autoUpdateWhenIdleGateEnabled", false),
            ),
            currentAccountEpoch = currentEpoch,
        )
        return AndroidMandatoryUpdatePolicy(
            accountEpoch = settled.accountEpoch,
            processGeneration = processGeneration,
            minimumVersion = settled.minimumVersion,
            backendRequiresUpdate = settled.backendRequiresUpdate,
            autoUpdateWhenIdleGateObserved = settled.autoUpdateWhenIdleGateObserved,
            autoUpdateWhenIdleGateEnabled =
                settled.autoUpdateWhenIdleGateObserved && settled.autoUpdateWhenIdleGateEnabled,
        )
    }

    override fun refreshEntrypointAvailability(): AndroidEntrypointAvailabilitySnapshot {
        val auth = authStatus()
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        if (!auth.optBoolean("loggedIn", false)) {
            return entrypointAvailabilityOwner.clearForLoggedOutAccount(accountEpoch)
        }

        val token = entrypointAvailabilityOwner.begin(accountEpoch)
        val policy = runCatching {
            host.probeUpdatePolicy("android-entrypoints-" + UUID.randomUUID())
        }.getOrNull()
        val roster = runCatching {
            host.requestValue("listAgents") as? JSONArray
        }.getOrNull()
        val currentEpoch = accountAccessOwner.currentProjection().accountEpoch
        return entrypointAvailabilityOwner.settle(
            token = token,
            currentAccountEpoch = currentEpoch,
            sandAgentNetworkGateObserved =
                policy?.optBoolean("sandAgentNetworkGateObserved", false) == true,
            sandAgentNetworkGateEnabled =
                policy?.optBoolean("sandAgentNetworkGateEnabled", false) == true,
            rosterObserved = roster != null,
            canonicalAgentCount = roster?.length() ?: 0,
        )
    }

    override fun mcpServerCatalog(): JSONArray =
        host.request("feature.mcp.serverCatalog").optJSONArray("servers") ?: JSONArray()

    override fun mcpTeamPopularity(): JSONObject =
        host.request("feature.mcp.teamPopularity")

    override fun availableModelCatalog(): JSONObject =
        host.request("feature.models.available")

    override fun mcpServerTools(serverId: String): JSONArray =
        host.request(
            "feature.mcp.serverTools.list",
            JSONObject().put("serverId", serverId),
        ).optJSONArray("tools") ?: JSONArray()

    override fun mcpServerCustomInstructions(serverId: String): String? {
        val projection = host.request(
            "feature.mcp.serverInstructions.get",
            JSONObject().put("serverId", serverId),
        )
        return projection
            .takeIf { it.has("instructions") && !it.isNull("instructions") }
            ?.optString("instructions")
            ?.takeIf(String::isNotBlank)
    }

    override fun setMcpServerCustomInstructions(serverId: String, instructions: String): String? {
        val projection = host.request(
            "feature.mcp.serverInstructions.set",
            JSONObject().put("serverId", serverId).put("instructions", instructions),
        )
        return projection
            .takeIf { it.has("instructions") && !it.isNull("instructions") }
            ?.optString("instructions")
            ?.takeIf(String::isNotBlank)
    }

    override fun toggleMcpServerTool(serverId: String, toolName: String): JSONArray =
        host.request(
            "feature.mcp.serverTools.toggle",
            JSONObject().put("serverId", serverId).put("toolName", toolName),
        ).optJSONArray("tools") ?: JSONArray()

    override fun mcpServerAccounts(serverId: String): JSONArray =
        host.request(
            "feature.mcp.serverAccounts.list",
            JSONObject().put("serverId", serverId),
        ).optJSONArray("accounts") ?: JSONArray()

    override fun logoutMcpAccount(serverId: String, accountKey: String): JSONObject =
        host.request(
            "feature.mcp.account.logout",
            JSONObject().put("serverId", serverId).put("accountKey", accountKey),
        )

    override fun renameMcpAccount(
        serverId: String,
        accountKey: String,
        newAccountKey: String,
    ): JSONObject =
        host.request(
            "feature.mcp.account.rename",
            JSONObject()
                .put("serverId", serverId)
                .put("accountKey", accountKey)
                .put("newAccountKey", newAccountKey),
        )

    override fun removeMcpAccount(serverId: String, accountKey: String): JSONObject =
        host.request(
            "feature.mcp.account.remove",
            JSONObject().put("serverId", serverId).put("accountKey", accountKey),
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
        try {
            host.request("feature.auth.status").also {
                observeAccountAccessAuth(it)
                observeRootConnectionTransport(RootConnectionTransportState.CONNECTED)
            }
        } catch (error: Throwable) {
            observeRootConnectionTransport(RootConnectionTransportState.DOWN)
            throw error
        }

    override fun accountPrivacyMode(): JSONObject =
        host.request("feature.account.privacyMode")

    override fun accountPrReviewPreferences(): AndroidPrReviewPreferences {
        val projection = host.request("feature.account.prReviewPreferences")
        fun destination(key: String): String? =
            projection.optString(key)
                .takeIf { it == "github" || it == "graphite" || it == "reviewCursor" }
        return AndroidPrReviewPreferences(
            user = destination("user"),
            team = destination("team"),
        )
    }

    override fun onboardingSeen(): JSONObject =
        host.request("feature.settings.onboardingSeen")

    override fun setOnboardingSeen(
        expectedAccountScope: String,
        hasSeenOnboarding: Boolean,
    ): JSONObject =
        host.request(
            "feature.settings.onboardingSeen.set",
            JSONObject()
                .put("expectedAccountScope", expectedAccountScope)
                .put("hasSeenOnboarding", hasSeenOnboarding),
        )

    override fun processMetricsSettings(): JSONObject =
        host.request("feature.settings.processMetrics")

    override fun setProcessMetricsLocalEnabled(enabled: Boolean): JSONObject =
        host.request(
            "feature.settings.processMetrics.set",
            JSONObject().put("localEnabled", enabled),
        )

    override fun reportRecoveredProcessCrash(
        kind: String,
        errorClass: String,
        occurredAtMs: Long,
        uptimeMs: Long,
    ): JSONObject =
        host.request(
            "feature.telemetry.processCrash.record",
            JSONObject()
                .put("kind", kind)
                .put("errorClass", errorClass)
                .put("occurredAtMs", occurredAtMs)
                .put("uptimeMs", uptimeMs),
        )

    override fun timeZoneSettings(): AndroidTimeZoneSettings {
        val detected = ZoneId.systemDefault().id
        host.request(
            "feature.settings.timeZone.detected",
            JSONObject().put("detectedTimeZone", detected),
        )
        val projection = host.request("feature.settings.timeZone")
        val override = projection
            .takeIf { it.has("overrideTimeZone") && !it.isNull("overrideTimeZone") }
            ?.optString("overrideTimeZone")
            ?.trim()
            ?.takeIf(String::isNotEmpty)
            ?.let(::validatedTimeZoneId)
        return AndroidTimeZoneSettings(
            detectedTimeZone = detected,
            overrideTimeZone = override,
        )
    }

    override fun setTimeZoneOverride(value: String?): AndroidTimeZoneSettings {
        val detected = ZoneId.systemDefault().id
        host.request(
            "feature.settings.timeZone.detected",
            JSONObject().put("detectedTimeZone", detected),
        )
        val normalized = value
            ?.trim()
            ?.takeIf(String::isNotEmpty)
            ?.let(::validatedTimeZoneId)
        val projection = host.request(
            "feature.settings.timeZone.set",
            JSONObject().put("overrideTimeZone", normalized ?: JSONObject.NULL),
        )
        val saved = projection
            .takeIf { it.has("overrideTimeZone") && !it.isNull("overrideTimeZone") }
            ?.optString("overrideTimeZone")
            ?.trim()
            ?.takeIf(String::isNotEmpty)
            ?.let(::validatedTimeZoneId)
        return AndroidTimeZoneSettings(
            detectedTimeZone = detected,
            overrideTimeZone = saved,
        )
    }

    private fun validatedTimeZoneId(value: String): String =
        runCatching { ZoneId.of(value).id }
            .getOrElse { throw IllegalArgumentException("Invalid time zone override", it) }

    override fun autoReviewInstructions(): AndroidAutoReviewInstructionsSnapshot =
        parseAutoReviewInstructionsSnapshot(
            host.request("feature.settings.autoReviewInstructions"),
        )

    override fun setAutoReviewInstructions(
        expectedRevision: Long,
        instructions: AndroidAutoReviewInstructions,
    ): AndroidAutoReviewInstructionsSnapshot =
        parseAutoReviewInstructionsSnapshot(
            host.request(
                "feature.settings.autoReviewInstructions.set",
                JSONObject()
                    .put("expectedRevision", expectedRevision)
                    .put(
                        "instructions",
                        JSONObject()
                            .put("isEnabled", instructions.isEnabled)
                            .put("allowInstructions", JSONArray(instructions.allowInstructions))
                            .put("blockInstructions", JSONArray(instructions.blockInstructions)),
                    ),
            ),
        )

    private fun parseAutoReviewInstructionsSnapshot(
        projection: JSONObject,
    ): AndroidAutoReviewInstructionsSnapshot {
        val instructions = projection.getJSONObject("instructions")
        fun stringList(key: String): List<String> {
            val values = instructions.optJSONArray(key) ?: JSONArray()
            return buildList(values.length()) {
                for (index in 0 until values.length()) {
                    values.optString(index).takeIf(String::isNotEmpty)?.let(::add)
                }
            }
        }
        return AndroidAutoReviewInstructionsSnapshot(
            accountFence = projection.getString("accountFence"),
            revision = projection.getLong("revision"),
            instructions = AndroidAutoReviewInstructions(
                isEnabled = instructions.optBoolean("isEnabled", true),
                allowInstructions = stringList("allowInstructions"),
                blockInstructions = stringList("blockInstructions"),
            ),
        )
    }

    override fun accountUsageSummary(): AndroidAccountUsageSummary {
        val projection = host.request("feature.account.usageSummary")
        fun optionalLong(source: JSONObject, key: String): Long? =
            source.takeIf { it.has(key) && !it.isNull(key) }?.optLong(key)
        fun optionalDouble(source: JSONObject, key: String): Double? =
            source.takeIf { it.has(key) && !it.isNull(key) }
                ?.optDouble(key, Double.NaN)
                ?.takeIf(Double::isFinite)
        val onDemand = projection.optJSONObject("onDemand")?.let { value ->
            val used = optionalLong(value, "usedCents") ?: return@let null
            val limit = optionalLong(value, "limitCents") ?: return@let null
            AndroidOnDemandUsage(
                usedCents = used,
                limitCents = limit,
                resetTimestampMs = optionalLong(value, "resetTimestampMs"),
            )
        }
        return AndroidAccountUsageSummary(
            sandUsagePercent = optionalDouble(projection, "sandUsagePercent"),
            sandUsageResetTimestampMs = optionalLong(projection, "sandUsageResetTimestampMs"),
            hasAvailableUsage = projection.optBoolean("hasAvailableUsage", false),
            hasNonZeroIncludedLimit = projection.optBoolean("hasNonZeroIncludedLimit", false),
            usesPooledEnterpriseAllowance = projection.optBoolean("usesPooledEnterpriseAllowance", false),
            isSandTrial = projection.optBoolean("isSandTrial", false),
            hasEndedSandTrial = projection.optBoolean("hasEndedSandTrial", false),
            canCancelSandTrial = projection.optBoolean("canCancelSandTrial", false),
            onDemand = onDemand,
        )
    }

    override fun accountCancelSandTrial() {
        val settled = host.request("feature.account.cancelSandTrial").optBoolean("ok", false)
        require(settled) { "Sand trial cancellation did not settle successfully" }
    }


    override fun credentialVaultList(): JSONArray =
        host.request("feature.credentialVault.list").optJSONArray("credentials") ?: JSONArray()

    override fun credentialVaultUpsert(
        name: String,
        label: String,
        secretValue: String,
        allowedOrigins: List<String>,
    ): JSONObject =
        host.request(
            "feature.credentialVault.upsert",
            JSONObject()
                .put("name", name)
                .put("label", label)
                .put("secretValue", secretValue)
                .put("allowedOrigins", JSONArray(allowedOrigins)),
        ).getJSONObject("credential")

    override fun credentialVaultRotate(
        name: String,
        label: String,
        secretValue: String,
        allowedOrigins: List<String>,
    ): JSONObject =
        host.request(
            "feature.credentialVault.rotate",
            JSONObject()
                .put("name", name)
                .put("label", label)
                .put("secretValue", secretValue)
                .put("allowedOrigins", JSONArray(allowedOrigins)),
        ).getJSONObject("credential")

    override fun credentialVaultRevoke(name: String): Boolean =
        host.request(
            "feature.credentialVault.revoke",
            JSONObject().put("name", name),
        ).optBoolean("revoked", false)

    override fun rootConnectionResilienceProjection(): RootConnectionResilienceProjection =
        rootConnectionOwner.currentProjection()

    override fun addRootConnectionResilienceListener(
        listener: (RootConnectionResilienceProjection) -> Unit,
    ): AutoCloseable {
        rootConnectionListeners += listener
        listener(rootConnectionOwner.currentProjection())
        return AutoCloseable { rootConnectionListeners.remove(listener) }
    }

    override fun rootConnectionRetry(): RootConnectionResilienceProjection {
        val token = rootConnectionOwner.requestRetry()
        publishRootConnectionProjection()
        if (token != null) {
            reportAgentsUnreachableTelemetry(
                phase = "retry",
                isManual = true,
                errorClass = rootConnectionOwner.currentProjection().failureCode,
            )
            dispatchRootConnectionRetry(token)
        }
        return rootConnectionOwner.currentProjection()
    }

    override fun accountAccessProjection(): AccountAccessProjection {
        val auth = authStatus()
        val token = accountAccessOwner.beginRefresh()
        if (!auth.optBoolean("loggedIn", false)) {
            val settled = accountAccessOwner.settle(
                token,
                AccountAccessFacts(
                    loggedIn = false,
                    sessionSettled = true,
                    recoveryState = AccountRecoveryState.READY,
                ),
            )
            reportAccessBlockedTelemetry(settled)
            publishRootConnectionProjection(rootConnectionOwner.observeReadiness(settled))
            return settled
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
            rosterLoadState = AccountRosterLoadState.ERROR
            val classification = classifyRosterAccessFailure(
                error.message ?: error::class.java.simpleName,
            )
            rosterFailureCode = classification.code
            rosterFailureTransportKind = classification.transportKind
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

        val settled = accountAccessOwner.settle(
            token,
            AccountAccessFacts(
                loggedIn = true,
                // Current Desktop Fabushi shipping composition owns Sand access in the
                // product account adapter; the Host exposes that canonical policy over JNI.
                sandAccessState = sandAccessState,
                blockReason = sandAccessBlockReason,
                // A logged-in Host projection is emitted only after durable
                // descriptor/account authorization succeeds. Binding refusal or corruption
                // clears the candidate session and projects logged-out before this point.
                authorizationState = AccountTruthState.GRANTED,
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
        reportAccessBlockedTelemetry(settled)
        rootConnectionOwner.observeReadiness(settled)
        if (
            settled.rosterFailureTransportKind == "network" ||
            settled.rosterFailureTransportKind == "dns"
        ) {
            observeRootConnectionTransport(RootConnectionTransportState.DOWN)
        } else {
            publishRootConnectionProjection()
        }
        return settled
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
    override fun authProfileUpdate(displayName: String): JSONObject {
        val normalized = displayName.trim()
        require(normalized.isNotEmpty() && normalized.length <= 200) {
            "Account display name must contain 1-200 characters"
        }
        return host.request(
            "feature.auth.profileUpdate",
            JSONObject().put("displayName", normalized),
        ).also { result ->
            if (result.optBoolean("ok", false)) {
                result.optJSONObject("auth")?.let(::observeAccountAccessAuth)
            }
        }
    }

    override fun authLogout(): JSONObject =
        host.request("feature.auth.logout").also(::observeAccountAccessAuth)

    private fun reportAccessBlockedTelemetry(projection: AccountAccessProjection) {
        val edge = accessBlockedTelemetryOwner.observe(
            projection = projection,
            nowMs = SystemClock.elapsedRealtime(),
        ) ?: return
        runCatching {
            host.request(
                "feature.account.telemetry.accessBlocked",
                JSONObject()
                    .put("edge", edge.edge)
                    .put("cause", edge.cause)
                    .put("accessReason", edge.accessReason)
                    .apply {
                        edge.msBlocked?.let { put("msBlocked", it) }
                    },
            )
        }
    }

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
        publishRootConnectionProjection(
            rootConnectionOwner.observeAccount(
                loggedIn = loggedIn,
                identity = identity,
                accountEpoch = currentEpoch,
            ),
        )
        if (!loggedIn || currentEpoch != previousEpoch) {
            // Account-transition cleanup must be exhaustive. A failure in one old-account
            // adapter must not leave later owners live under the replacement account epoch.
            AccountTransitionCleanup().apply {
                add { sendSubmissionQueue.fenceForAccountChange() }
                add { remoteControlSessionStore.clear() }
                add { remotePairingStore.clear() }
                add { host.clearProtectedRemoteBinding() }
            }.runAll()
        }
        computerRebuildOwner.observeAccount(currentEpoch)
        if (loggedIn) resumeQueuedComputerUpdateIfNeeded()
    }
    private fun publishRootConnectionProjection(
        projection: RootConnectionResilienceProjection = rootConnectionOwner.currentProjection(),
    ) {
        rootConnectionListeners.forEach { listener ->
            runCatching { listener(projection) }
        }
    }

    private fun observeRootConnectionTransport(state: RootConnectionTransportState) {
        val previous = rootConnectionOwner.currentProjection()
        val retryToken = rootConnectionOwner.observeTransport(state)
        val current = rootConnectionOwner.currentProjection()
        publishRootConnectionProjection(current)
        reportRootConnectionTelemetryTransition(previous, current)
        if (retryToken != null) {
            reportAgentsUnreachableTelemetry(
                phase = "retry",
                isManual = false,
                errorClass = current.failureCode,
            )
            dispatchRootConnectionRetry(retryToken)
        }
    }

    private fun reportRootConnectionTelemetryTransition(
        previous: RootConnectionResilienceProjection,
        current: RootConnectionResilienceProjection,
    ) {
        when {
            previous.transport != RootConnectionTransportState.DOWN &&
                current.transport == RootConnectionTransportState.DOWN ->
                reportAgentsUnreachableTelemetry(
                    phase = "entered",
                    isManual = false,
                    errorClass = current.failureCode,
                )
            previous.transport == RootConnectionTransportState.DOWN &&
                current.transport == RootConnectionTransportState.CONNECTED ->
                reportAgentsUnreachableTelemetry(
                    phase = "recovered",
                    isManual = false,
                    errorClass = current.failureCode,
                )
        }
    }

    private fun reportAgentsUnreachableTelemetry(
        phase: String,
        isManual: Boolean,
        errorClass: String?,
    ) {
        runCatching {
            host.request(
                "feature.telemetry.agentsUnreachable",
                JSONObject()
                    .put("phase", phase)
                    .put("isManual", isManual)
                    .apply {
                        errorClass?.trim()?.takeIf(String::isNotEmpty)?.let {
                            put("errorClass", it.take(128))
                        }
                    },
            )
        }
    }

    private fun dispatchRootConnectionRetry(token: RootConnectionRetryToken) {
        rootConnectionRetryExecutor.execute {
            val projectionBeforeRetry = rootConnectionOwner.currentProjection()
            try {
                val access = accountAccessProjection()
                val code = access.rosterFailureCode
                val replacementTransportDown =
                    access.rosterFailureTransportKind == "network" ||
                        access.rosterFailureTransportKind == "dns" ||
                        rootConnectionOwner.currentProjection().transport !=
                            RootConnectionTransportState.CONNECTED
                if (code == null) {
                    rootConnectionOwner.completeRetrySuccess(token)
                } else {
                    if (replacementTransportDown) {
                        observeRootConnectionTransport(RootConnectionTransportState.DOWN)
                    }
                    rootConnectionOwner.completeRetryFailure(
                        token = token,
                        code = code,
                        transportDown = replacementTransportDown,
                    )
                }
            } catch (error: Throwable) {
                val classification = classifyRosterAccessFailure(
                    error.message ?: error::class.java.simpleName,
                )
                observeRootConnectionTransport(RootConnectionTransportState.DOWN)
                rootConnectionOwner.completeRetryFailure(
                    token = token,
                    code = classification.code,
                    transportDown = true,
                )
            } finally {
                val current = rootConnectionOwner.currentProjection()
                publishRootConnectionProjection(current)
                reportRootConnectionTelemetryTransition(projectionBeforeRetry, current)
            }
        }
    }

    override fun humanCallCreate(
        callId: String,
        requestId: String,
        scopeId: String,
        participantIds: List<String>,
    ): AndroidHumanCallSession =
        parseHumanCallSession(
            host.request(
                "feature.call.create",
                JSONObject()
                    .put("callId", callId)
                    .put("requestId", requestId)
                    .put("scopeId", scopeId)
                    .put("participantIds", JSONArray(participantIds)),
            ),
        )

    override fun humanCallList(scopeId: String, limit: Int): List<AndroidHumanCallSession> {
        val value = host.requestValue(
            "feature.call.list",
            JSONObject().put("scopeId", scopeId).put("limit", limit.coerceIn(1, 100)),
        ) as? JSONArray ?: JSONArray()
        return buildList(value.length()) {
            for (index in 0 until value.length()) {
                add(parseHumanCallSession(value.getJSONObject(index)))
            }
        }
    }

    override fun humanCallLease(
        callId: String,
        generation: Long,
        allowClaim: Boolean,
    ): AndroidHumanCallLease =
        parseHumanCallLease(
            host.request(
                "feature.call.lease",
                JSONObject()
                    .put("callId", callId)
                    .put("generation", generation)
                    .put("allowClaim", allowClaim),
            ),
        )

    override fun humanCallTransition(
        callId: String,
        generation: Long,
        action: String,
        terminalReason: String?,
    ): AndroidHumanCallSession =
        parseHumanCallSession(
            host.request(
                "feature.call.transition",
                JSONObject()
                    .put("callId", callId)
                    .put("generation", generation)
                    .put("action", action)
                    .putNullable("terminalReason", terminalReason),
            ),
        )

    override fun humanCallUpdateMedia(
        callId: String,
        generation: Long,
        mediaCapabilities: JSONObject,
        deviceSelection: JSONObject,
    ): AndroidHumanCallSession =
        parseHumanCallSession(
            host.request(
                "feature.call.media.update",
                JSONObject()
                    .put("callId", callId)
                    .put("generation", generation)
                    .put("mediaCapabilities", JSONObject(mediaCapabilities.toString()))
                    .put("deviceSelection", JSONObject(deviceSelection.toString())),
            ),
        )

    override fun humanCallSendSignal(
        callId: String,
        generation: Long,
        seq: Long,
        kind: String,
        payload: JSONObject,
    ): AndroidHumanCallSignal =
        parseHumanCallSignal(
            host.request(
                "feature.call.signal.send",
                JSONObject()
                    .put("callId", callId)
                    .put("generation", generation)
                    .put("seq", seq)
                    .put("kind", kind)
                    .put("payload", JSONObject(payload.toString())),
            ),
        )

    override fun humanCallSignalsSince(
        callId: String,
        generation: Long,
        afterSeq: Long,
    ): List<AndroidHumanCallSignal> {
        val value = host.requestValue(
            "feature.call.signal.list",
            JSONObject()
                .put("callId", callId)
                .put("generation", generation)
                .put("afterSeq", afterSeq),
        ) as? JSONArray ?: JSONArray()
        return buildList(value.length()) {
            for (index in 0 until value.length()) {
                add(parseHumanCallSignal(value.getJSONObject(index)))
            }
        }
    }

    override fun humanCallReconcile(
        callId: String,
        generation: Long,
        terminalState: String,
        reason: String?,
    ): AndroidHumanCallSession =
        parseHumanCallSession(
            host.request(
                "feature.call.reconcile",
                JSONObject()
                    .put("callId", callId)
                    .put("generation", generation)
                    .put("terminalState", terminalState)
                    .putNullable("reason", reason),
            ),
        )

    private fun parseHumanCallSession(value: JSONObject): AndroidHumanCallSession {
        val participants = value.optJSONArray("participantIds") ?: JSONArray()
        return AndroidHumanCallSession(
            id = value.getString("id"),
            requestId = value.getString("requestId"),
            scopeId = value.getString("scopeId"),
            creatorId = value.getString("creatorId"),
            participantIds = buildList(participants.length()) {
                for (index in 0 until participants.length()) add(participants.getString(index))
            },
            state = AndroidHumanCallState.fromWire(value.getString("state")),
            generation = value.getLong("generation"),
            signalSeq = value.optLong("signalSeq", 0L),
            mediaCapabilities = value.optJSONObject("mediaCapabilities") ?: JSONObject(),
            deviceSelection = value.optJSONObject("deviceSelection") ?: JSONObject(),
            createdAtMs = value.getLong("createdAtMs"),
            updatedAtMs = value.getLong("updatedAtMs"),
            terminalReason = value.optString("terminalReason").takeIf(String::isNotBlank),
        )
    }

    private fun parseHumanCallLease(value: JSONObject): AndroidHumanCallLease =
        AndroidHumanCallLease(
            callId = value.getString("callId"),
            userId = value.getString("userId"),
            deviceId = value.getString("deviceId"),
            role = value.getString("role"),
            isOwner = value.optBoolean("isOwner", false),
            claimAvailable = value.optBoolean("claimAvailable", false),
            generation = value.getLong("generation"),
        )

    private fun parseHumanCallSignal(value: JSONObject): AndroidHumanCallSignal =
        AndroidHumanCallSignal(
            callId = value.getString("callId"),
            generation = value.getLong("generation"),
            seq = value.getLong("seq"),
            senderDeviceId = value.getString("senderDeviceId"),
            kind = value.getString("kind"),
            payload = value.optJSONObject("payload") ?: JSONObject(),
            createdAtMs = value.getLong("createdAtMs"),
        )

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
    override fun sendAcknowledgementBegin(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement =
        sendAcknowledgementBeginWithAttachments(
            accountSlot = accountSlot,
            agentId = agentId,
            nonce = nonce,
            text = text,
            attachments = emptyList(),
            entries = entries,
        )

    override fun sendAcknowledgementBeginWithAttachments(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        richText: JSONObject?,
        attachments: List<AndroidAgentAttachment>,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement =
        parseSendAcknowledgement(
            host.request(
                "feature.sendAcknowledgement.begin",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("nonce", nonce)
                    .put("text", text)
                    .put("richText", richText ?: JSONObject.NULL)
                    .put("attachments", agentAttachmentsJson(attachments))
                    .put("entries", sendAcknowledgementEntriesJson(entries)),
            ),
        )

    override fun sendAcknowledgementBeginWithModelAndAttachments(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        text: String,
        richText: JSONObject?,
        model: String?,
        attachments: List<AndroidAgentAttachment>,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgement =
        parseSendAcknowledgement(
            host.request(
                "feature.sendAcknowledgement.begin",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("nonce", nonce)
                    .put("text", text)
                    .put("richText", richText ?: JSONObject.NULL)
                    .put("model", model?.trim()?.takeIf(String::isNotEmpty) ?: JSONObject.NULL)
                    .put("attachments", agentAttachmentsJson(attachments))
                    .put("entries", sendAcknowledgementEntriesJson(entries)),
            ),
        )

    override fun sendAcknowledgementDispatch(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
    ): AndroidSendAcknowledgementDispatch =
        sendSubmissionQueue.submit(
            AndroidSendSubmissionScope(
                accountSlot = accountSlot,
                agentId = agentId,
                nonce = nonce,
                generation = generation,
            ),
        )

    override fun sendAcknowledgementRetry(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
        freshNonce: String,
        entries: List<AndroidSendAcknowledgementEntry>,
    ): AndroidSendAcknowledgementDispatch {
        val prepared = parseSendAcknowledgement(
            host.request(
                "feature.sendAcknowledgement.retryPrepare",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("nonce", nonce)
                    .put("generation", generation)
                    .put("freshNonce", freshNonce)
                    .put("entries", sendAcknowledgementEntriesJson(entries)),
            ),
        )
        return sendSubmissionQueue.submit(
            AndroidSendSubmissionScope(
                accountSlot = accountSlot,
                agentId = agentId,
                nonce = prepared.nonce,
                generation = prepared.generation,
            ),
        )
    }

    override fun sendAcknowledgementCancelQueued(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
    ): Boolean =
        sendSubmissionQueue.cancelQueued(
            AndroidSendSubmissionScope(accountSlot, agentId, nonce, generation),
        )

    override fun clearApprovalsAfterJournal(
        accountSlot: String?,
        agentId: String,
        nonce: String,
        generation: Long,
        approvalId: String?,
        approvalOperationId: String?,
    ): AndroidApprovalCleanupResult {
        val result = host.request(
            "feature.sendAcknowledgement.clearApprovals",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId)
                .put("nonce", nonce)
                .put("generation", generation)
                .putNullable("approvalId", approvalId)
                .putNullable("approvalOperationId", approvalOperationId),
        )
        return AndroidApprovalCleanupResult(
            generation = result.getLong("generation"),
            cleared = result.optBoolean("cleared", false),
            approvalId = result.optString("approvalId").takeIf(String::isNotBlank),
        )
    }

    override fun sendAcknowledgementSnapshot(
        accountSlot: String?,
        agentId: String,
    ): List<AndroidSendAcknowledgement> {
        val requestAccountEpoch = accountAccessOwner.currentProjection().accountEpoch
        val projection = host.request(
            "feature.sendAcknowledgement.snapshot",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId),
        )
        val records = projection.optJSONArray("records") ?: JSONArray()
        val parsed = buildList(records.length()) {
            for (index in 0 until records.length()) {
                add(parseSendAcknowledgement(records.getJSONObject(index)))
            }
        }
        // A snapshot begun under an older account epoch may still be returned to its caller, but
        // it must never repopulate the live transport scheduler after an account switch.
        if (accountAccessOwner.currentProjection().accountEpoch == requestAccountEpoch) {
            sendSubmissionQueue.discover(parsed)
            if (!isSendTransportDown()) scheduleSendSubmissionFlush()
        }
        return parsed
    }

    override fun composerDraftRead(accountSlot: String?, agentId: String): AndroidComposerDraft? =
        host.request(
            "feature.composerDraft.read",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId),
        ).optJSONObject("draft")?.let(::parseComposerDraft)

    override fun composerDraftWrite(
        accountSlot: String?,
        agentId: String,
        prompt: String,
    ): AndroidComposerDraft =
        parseComposerDraft(
            host.request(
                "feature.composerDraft.write",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("prompt", prompt),
            ),
        )

    override fun composerDraftClearIfCurrent(
        accountSlot: String?,
        agentId: String,
        draftId: String,
    ): Boolean =
        host.request(
            "feature.composerDraft.clearIfCurrent",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId)
                .put("draftId", draftId),
        ).optBoolean("cleared", false)

    private fun parseComposerDraft(value: JSONObject): AndroidComposerDraft =
        AndroidComposerDraft(
            draftId = value.getString("draftId"),
            prompt = value.getString("prompt"),
            attachments = parseAgentAttachments(value.optJSONArray("attachments") ?: JSONArray()),
            updatedAtMs = value.getLong("updatedAtMs"),
        ).also {
            check(it.draftId.isNotBlank()) { "composer draft returned an empty identity" }
        }

    override fun composerDraftStageAttachment(
        accountSlot: String?,
        agentId: String,
        contentUri: String,
    ): AndroidComposerDraft {
        require(contentUri.startsWith("content://")) { "Agent attachment URI must use ContentResolver content:// authority" }
        val uri = Uri.parse(contentUri)
        val resolver = applicationContext.contentResolver
        var name: String? = null
        var sizeBytes: Long? = null
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (nameIndex >= 0) name = cursor.getString(nameIndex)?.trim()?.takeIf(String::isNotEmpty)
                val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) sizeBytes = cursor.getLong(sizeIndex).takeIf { it >= 0L }
            }
        }
        val safeName = name ?: uri.lastPathSegment?.substringAfterLast('/')?.takeIf(String::isNotBlank)
            ?: "attachment"
        val mimeType = resolver.getType(uri)?.trim()?.takeIf(String::isNotEmpty)
        val begin = host.request(
            "feature.composerDraft.attachment.begin",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId)
                .put("name", safeName)
                .putNullable("mimeType", mimeType)
                .apply { if (sizeBytes != null && sizeBytes >= 0L) put("sizeBytes", sizeBytes) },
        )
        val attachmentId = begin.getString("attachmentId")
        var offset = 0L
        try {
            resolver.openInputStream(uri).use { input ->
                requireNotNull(input) { "Unable to open Agent attachment content stream" }
                val buffer = ByteArray(1024 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    if (read == 0) continue
                    val encoded = Base64.encodeToString(buffer, 0, read, Base64.NO_WRAP)
                    val appended = host.request(
                        "feature.composerDraft.attachment.append",
                        JSONObject()
                            .putNullable("accountSlot", accountSlot)
                            .put("agentId", agentId)
                            .put("attachmentId", attachmentId)
                            .put("offset", offset)
                            .put("dataBase64", encoded),
                    )
                    offset = appended.getLong("nextOffset")
                }
            }
            val finished = host.request(
                "feature.composerDraft.attachment.finish",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("attachmentId", attachmentId),
            )
            return parseComposerDraft(finished.getJSONObject("draft"))
        } catch (error: Throwable) {
            runCatching {
                host.request(
                    "feature.composerDraft.attachment.abort",
                    JSONObject()
                        .putNullable("accountSlot", accountSlot)
                        .put("agentId", agentId)
                        .put("attachmentId", attachmentId),
                )
            }
            throw error
        }
    }

    override fun composerDraftRemoveAttachment(
        accountSlot: String?,
        agentId: String,
        attachmentId: String,
    ): AndroidComposerDraft? {
        val response = host.request(
            "feature.composerDraft.attachment.remove",
            JSONObject()
                .putNullable("accountSlot", accountSlot)
                .put("agentId", agentId)
                .put("attachmentId", attachmentId),
        )
        return response.optJSONObject("draft")?.let(::parseComposerDraft)
    }

    override fun acceptClientSideToolV2(event: JSONObject): AndroidClientSideToolV2Event =
        parseClientSideToolV2Event(
            host.request(
                "coordinator.clientSideToolV2.accept",
                JSONObject().put("event", event),
            ),
        )

    override fun replayClientSideToolV2(): List<AndroidClientSideToolV2Event> {
        val rows = host.request("coordinator.clientSideToolV2.replay", JSONObject())
            .optJSONArray("value")
            ?: JSONArray()
        return buildList(rows.length()) {
            for (index in 0 until rows.length()) {
                add(parseClientSideToolV2Event(rows.getJSONObject(index)))
            }
        }
    }

    override fun respondToWidget(
        accountSlot: String?,
        agentId: String,
        entryId: String,
        value: String,
        generation: Long,
    ): AndroidWidgetActionResult =
        parseWidgetActionResult(
            method = "feature.widget.respond",
            expectedGeneration = generation,
            requireOperationId = true,
            reply = host.request(
                "feature.widget.respond",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("entryId", entryId)
                    .put("value", value)
                    .put("generation", generation),
            ),
        )

    override fun dismissWidget(
        accountSlot: String?,
        agentId: String,
        entryId: String,
        generation: Long,
    ): AndroidWidgetActionResult =
        parseWidgetActionResult(
            method = "feature.widget.dismiss",
            expectedGeneration = generation,
            requireOperationId = false,
            reply = host.request(
                "feature.widget.dismiss",
                JSONObject()
                    .putNullable("accountSlot", accountSlot)
                    .put("agentId", agentId)
                    .put("entryId", entryId)
                    .put("generation", generation),
            ),
        )

    private fun parseWidgetActionResult(
        method: String,
        expectedGeneration: Long,
        requireOperationId: Boolean,
        reply: JSONObject,
    ): AndroidWidgetActionResult {
        check(expectedGeneration > 0L) { "$method requires a positive presentation generation" }
        check(reply.has("accepted") && reply.opt("accepted") is Boolean) {
            "$method returned a malformed accepted flag"
        }
        val generation = reply.optLong("generation", -1L)
        check(generation == expectedGeneration) {
            "$method returned a stale generation"
        }
        val operationId = reply.optString("operationId").trim().takeIf(String::isNotEmpty)
        if (reply.getBoolean("accepted") && requireOperationId) {
            check(operationId != null) { "$method accepted without a continuation operation" }
        }
        return AndroidWidgetActionResult(
            accepted = reply.getBoolean("accepted"),
            generation = generation,
            operationId = operationId,
            reason = reply.optString("reason").trim().takeIf(String::isNotEmpty),
        )
    }

    override fun featureInterrupt(params: JSONObject) = host.request("feature.interrupt", params)
    override fun featureApprovalResolve(params: JSONObject) = host.request("feature.approval.resolve", params)
    override fun transcriptSnapshot(): JSONArray =
        host.requestValue("feature.transcript.snapshot") as? JSONArray ?: JSONArray()

    override fun transcriptPage(
        agentId: String,
        beforeSeq: Long?,
        limit: Int,
    ): AndroidTranscriptHistoryPage {
        require(agentId.isNotBlank()) { "Transcript page requires a non-empty Agent id" }
        require(limit in 1..500) { "Transcript page limit must be between 1 and 500" }
        val params = JSONObject()
            .put("agentId", agentId)
            .put("limit", limit)
        if (beforeSeq != null) {
            require(beforeSeq >= 0L) { "Transcript page cursor must be non-negative" }
            params.put("beforeSeq", beforeSeq)
        }
        val value = host.request("feature.transcript.page", params)
        val entries = value.optJSONArray("entries") ?: error("Transcript page is missing entries")
        val nextBeforeSeq = if (value.has("nextBeforeSeq") && !value.isNull("nextBeforeSeq")) {
            value.getLong("nextBeforeSeq")
        } else {
            null
        }
        return AndroidTranscriptHistoryPage(entries = entries, nextBeforeSeq = nextBeforeSeq)
    }

    override fun conversationOutlineSnapshot(agentId: String): List<AndroidConversationOutlineItem> =
        parseAndroidConversationOutlineItems(transcriptSnapshot(), agentId)

    private fun parseClientSideToolV2Event(value: JSONObject): AndroidClientSideToolV2Event =
        AndroidClientSideToolV2Event(
            accountSlot = value.getString("accountSlot"),
            agentId = value.getString("agentId"),
            epoch = value.getString("epoch"),
            sequence = value.getLong("sequence"),
            projection = value.optJSONObject("projection")?.let(::parseToolResultCard),
        ).also {
            check(it.accountSlot == "host") {
                "client-side-tool-v2 event crossed an unexpected account slot"
            }
        }

    private fun parseToolResultCard(value: JSONObject): AndroidToolResultCard =
        AndroidToolResultCard(
            kind = value.getString("kind"),
            toolCallId = value.getString("toolCallId"),
            status = value.getString("status"),
            path = value.optString("path").takeIf(String::isNotBlank),
            command = value.optString("command").takeIf(String::isNotBlank),
            workingDirectory = value.optString("workingDirectory").takeIf(String::isNotBlank),
            summary = value.optString("summary"),
            output = value.optString("output").replace("\r\n", "\n").replace('\r', '\n'),
            diff = value.optString("diff").replace("\r\n", "\n").replace('\r', '\n'),
            isStreaming = value.optBoolean("isStreaming", false),
            isBackground = value.optBoolean("isBackground", false),
        )

    private fun JSONObject.putNullable(key: String, value: String?): JSONObject =
        if (value == null) put(key, JSONObject.NULL) else put(key, value)

    private fun agentAttachmentsJson(
        attachments: List<AndroidAgentAttachment>,
    ): JSONArray = JSONArray().apply {
        attachments.forEach { attachment ->
            put(
                JSONObject()
                    .put("id", attachment.id)
                    .put("path", attachment.path)
                    .put("name", attachment.name)
                    .put("size", attachment.sizeBytes)
                    .putNullable("mimeType", attachment.mimeType),
            )
        }
    }

    private fun parseAgentAttachments(rows: JSONArray): List<AndroidAgentAttachment> =
        buildList(rows.length()) {
            for (index in 0 until rows.length()) {
                val value = rows.getJSONObject(index)
                add(
                    AndroidAgentAttachment(
                        id = value.getString("id"),
                        path = value.getString("path"),
                        name = value.getString("name"),
                        sizeBytes = value.getLong("size"),
                        mimeType = value.optString("mimeType").takeIf(String::isNotBlank),
                    ),
                )
            }
        }

    private fun sendAcknowledgementEntriesJson(
        entries: List<AndroidSendAcknowledgementEntry>,
    ): JSONArray = JSONArray().apply {
        entries.forEach { entry ->
            put(
                JSONObject()
                    .put("id", entry.id)
                    .put("kind", entry.kind)
                    .put("clientNonce", entry.clientNonce),
            )
        }
    }

    private fun parseSendAcknowledgement(value: JSONObject): AndroidSendAcknowledgement {
        val entriesJson = value.optJSONArray("entries") ?: JSONArray()
        val entries = buildList(entriesJson.length()) {
            for (index in 0 until entriesJson.length()) {
                val entry = entriesJson.getJSONObject(index)
                add(
                    AndroidSendAcknowledgementEntry(
                        id = entry.getString("id"),
                        kind = entry.getString("kind"),
                        clientNonce = entry.getString("clientNonce"),
                    ),
                )
            }
        }
        val priorJson = value.optJSONArray("priorNonces") ?: JSONArray()
        val prior = buildList(priorJson.length()) {
            for (index in 0 until priorJson.length()) add(priorJson.getString(index))
        }
        return AndroidSendAcknowledgement(
            accountSlot = value.optString("accountSlot").takeIf(String::isNotBlank),
            agentId = value.getString("agentId"),
            nonce = value.getString("nonce"),
            priorNonces = prior,
            entries = entries,
            messageText = value.getString("messageText"),
            messageRichText = value.optJSONObject("messageRichText"),
            attachments = parseAgentAttachments(value.optJSONArray("attachments") ?: JSONArray()),
            phase = AndroidSendAcknowledgementPhase.fromWire(value.getString("phase")),
            generation = value.getLong("generation"),
            failedAtMs = value.opt("failedAtMs").let { raw ->
                if (raw is Number) raw.toLong() else null
            },
            reason = value.optString("reason").takeIf(String::isNotBlank),
        )
    }

    private fun parseSendAcknowledgementDispatch(value: JSONObject): AndroidSendAcknowledgementDispatch {
        val accepted = value.optJSONObject("accepted") ?: JSONObject()
        return AndroidSendAcknowledgementDispatch(
            acknowledgement = parseSendAcknowledgement(value.getJSONObject("acknowledgement")),
            operationId = accepted.optString("operationId").takeIf(String::isNotBlank),
        )
    }

    private fun isSendTransportDown(): Boolean {
        val network = connectivityManager.activeNetwork ?: return true
        val capabilities = connectivityManager.getNetworkCapabilities(network) ?: return true
        return !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
    }

    private fun queueSendAcknowledgementDirect(
        scope: AndroidSendSubmissionScope,
    ): AndroidSendAcknowledgement =
        parseSendAcknowledgement(
            host.request("feature.sendAcknowledgement.queue", scope.toJson()),
        )

    private fun dispatchSendAcknowledgementDirect(
        scope: AndroidSendSubmissionScope,
    ): AndroidSendAcknowledgementDispatch =
        parseSendAcknowledgementDispatch(
            host.request("feature.sendAcknowledgement.dispatch", scope.toJson()),
        )

    private fun cancelQueuedSendAcknowledgementDirect(
        scope: AndroidSendSubmissionScope,
    ): Boolean =
        host.request(
            "feature.sendAcknowledgement.cancelQueued",
            scope.toJson(),
        ).optBoolean("cancelled", false)

    private fun publishFlushedSendAcknowledgement(
        scope: AndroidSendSubmissionScope,
        result: AndroidSendAcknowledgementDispatch,
    ) {
        val operationId = result.operationId ?: return
        host.publishFeatureEvent(
            JSONObject()
                .put("type", "send-acknowledgement-flushed")
                .putNullable("accountSlot", scope.accountSlot)
                .put("agentId", scope.agentId)
                .put("nonce", scope.nonce)
                .put("generation", scope.generation)
                .put("operationId", operationId),
        )
    }

    private fun scheduleSendSubmissionFlush() {
        sendSubmissionExecutor.execute {
            runCatching { sendSubmissionQueue.flush() }
        }
    }

    private fun AndroidSendSubmissionScope.toJson(): JSONObject =
        JSONObject()
            .putNullable("accountSlot", accountSlot)
            .put("agentId", agentId)
            .put("nonce", nonce)
            .put("generation", generation)

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

    override fun agentAsyncTasks(id: String): JSONArray =
        host.requestValue("feature.agent.asyncTasks", JSONObject().put("id", id)) as? JSONArray
            ?: JSONArray()

    override fun agentList(): JSONArray {
        runCatching { reconcileAgentRosterMutations() }
        return host.requestValue("listAgents") as? JSONArray ?: JSONArray()
    }

    override fun isUpdateHostIdle(): Boolean =
        runCatching {
            val agents = agentList()
            (0 until agents.length()).none { index ->
                agents.optJSONObject(index)?.optBoolean("isRunning", false) == true
            }
        }.getOrDefault(false)

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
                val result = outcome.opt("result")
                // The Host settlement is canonical. Projection publication is best-effort and
                // must be fenced again after the synchronous Host call: account/session identity
                // may have changed while the old operation was in flight. Settling the old
                // journal row is correct, but publishing its roster payload into the new account
                // would violate canonical account ownership.
                val currentFence = runCatching { currentAgentRosterAccountFence() }.getOrNull()
                if (agentRosterMutationProjectionMatchesCurrentFence(operation.accountFence, currentFence)) {
                    runCatching { publishAgentRosterMutationProjection(mutation, result) }
                }
                result
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

    private fun publishAgentRosterMutationProjection(mutation: JSONObject, result: Any?) {
        val kind = mutation.optString("kind")
        val resultObject = result as? JSONObject
        val agent = when (kind) {
            "update", "notifications", "hidden", "unread", "group-members" -> resultObject
            "duplicate" -> resultObject?.optJSONObject("agent")
            else -> null
        }
        if (agent != null) {
            publishFeatureEvent(
                JSONObject()
                    .put("type", "agent-upserted")
                    .put("agent", JSONObject(agent.toString())),
            )
        } else {
            publishFeatureEvent(JSONObject().put("type", "agents"))
        }
    }

    override fun agentCreate(name: String, description: String): JSONObject =
        agentCreateWithPersona(name, description, avatarShape = null, avatarColor = null)

    override fun agentCreateWithPersona(
        name: String,
        description: String,
        avatarShape: String?,
        avatarColor: String?,
    ): JSONObject =
        host.request(
            "createAgent",
            JSONObject()
                .put("name", name)
                .put("description", description)
                .put("origin", "user")
                .put("avatarShape", avatarShape ?: JSONObject.NULL)
                .put("avatarColor", avatarColor ?: JSONObject.NULL),
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

    override fun agentUpdateProfile(
        id: String,
        name: String,
        description: String,
        title: String?,
        avatarShape: String?,
        avatarColor: String?,
    ): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "update")
                .put("id", id)
                .put("name", name)
                .put("description", description)
                .put("title", title ?: JSONObject.NULL)
                .put("avatarShape", avatarShape ?: JSONObject.NULL)
                .put("avatarColor", avatarColor ?: JSONObject.NULL),
        ) as? JSONObject ?: JSONObject()

    override fun agentUpdateAvatarImage(id: String, avatarDataUrl: String?): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "update")
                .put("id", id)
                .put("avatarDataUrl", avatarDataUrl ?: JSONObject.NULL),
        ) as? JSONObject ?: JSONObject()

    override fun agentGenerateAvatarImage(description: String): AndroidGeneratedAvatarImage {
        val result = host.request(
            "feature.agent.avatar.generate",
            JSONObject().put("description", description),
        )
        val imageData = result.optString("imageData").trim()
        val mimeType = result.optString("mimeType").trim()
        require(imageData.isNotEmpty()) { "Agent avatar generation returned no image data" }
        require(mimeType.startsWith("image/")) { "Agent avatar generation returned an invalid MIME type" }
        return AndroidGeneratedAvatarImage(
            imageData = imageData,
            mimeType = mimeType,
        )
    }

    override fun agentSetNotifyOnUpdates(id: String, isEnabled: Boolean): JSONObject =
        durableAgentRosterMutation(
            JSONObject()
                .put("kind", "notifications")
                .put("id", id)
                .put("value", isEnabled),
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

    override fun agentPinnedIds(): List<String> {
        val result = host.requestValue("getPinnedAgents") as? JSONArray ?: JSONArray()
        return buildList {
            for (index in 0 until result.length()) {
                result.optString(index).trim().takeIf(String::isNotEmpty)?.let(::add)
            }
        }
    }

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

    override fun agentSidebarSections(): AndroidSidebarSectionsSnapshot {
        val result = host.request("feature.agent.sidebarSections")
        return AndroidSidebarSectionsSnapshot(
            accountFence = result.optString("accountFence"),
            revision = result.optLong("revision", 0L),
            sections = parseSidebarSections(result.optJSONArray("sections")),
        )
    }

    override fun agentSetSidebarSections(
        base: AndroidSidebarSectionsSnapshot,
        sections: List<AndroidSidebarSection>,
    ): AndroidSidebarSectionsSnapshot {
        fun encode(values: List<AndroidSidebarSection>): JSONArray = JSONArray().also { array ->
            values.forEach { section ->
                val agentIds = JSONArray()
                section.agentIds.forEach(agentIds::put)
                array.put(
                    JSONObject()
                        .put("id", section.id)
                        .put("name", section.name)
                        .put("agentIds", agentIds)
                        .put("isCollapsed", section.isCollapsed),
                )
            }
        }
        val result = durableAgentRosterMutation(
            JSONObject()
                .put("kind", "sidebar-sections")
                .put("accountFence", base.accountFence)
                .put("expectedRevision", base.revision)
                .put("baseSections", encode(base.sections))
                .put("sections", encode(sections)),
        ) as? JSONObject ?: JSONObject()
        return AndroidSidebarSectionsSnapshot(
            accountFence = result.optString("accountFence", base.accountFence),
            revision = result.optLong("revision", base.revision),
            sections = parseSidebarSections(result.optJSONArray("sections")),
        )
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
                        isCollapsed = section.optBoolean("isCollapsed", false),
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

    override fun rootNotificationTrays(): JSONArray =
        host.requestValue("getTrays") as? JSONArray ?: JSONArray()

    override fun dismissRootNotificationTray(id: String): Boolean =
        host.request("dismissTray", JSONObject().put("id", id))
            .optBoolean("dismissed", false)

    override fun clearRootNotificationTrays(): Boolean =
        host.request("clearTrays").optBoolean("cleared", false)

    override fun invokeRootNotificationDashboardAction(
        action: String,
        args: JSONObject,
    ): JSONObject =
        host.request(
            "feature.account.dashboardAction",
            JSONObject().put("action", action).put("args", JSONObject(args.toString())),
        )

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
            reconciledCreateOutcome = false,
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
            reconciledCreateOutcome = true,
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
        return dispatchComputerRebuildRequest(
            accountEpoch = accountEpoch,
            requestId = requestId,
            preserveData = preserveData,
            forceRecreate = forceRecreate,
        )
    }

    override fun computerRebuildForceUpdate(): JSONObject {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        require(accountEpoch > 0L) { "Computer forced update requires a settled account epoch" }
        val snapshot = computerRebuildOwner.snapshot(accountEpoch)
        require(snapshot.imageUpdateAvailable == true) { "Computer update is not available" }
        require(!snapshot.queuedWhenIdle && snapshot.kind == null && !snapshot.pending && !snapshot.outcomeUnknown) {
            "Computer rebuild episode is not settled"
        }
        val requestId = "android-rebuild-$processGeneration-" + UUID.randomUUID().toString()
        computerRebuildOwner.reserveRequest(accountEpoch, requestId)
        return dispatchComputerRebuildRequest(
            accountEpoch = accountEpoch,
            requestId = requestId,
            preserveData = true,
            forceRecreate = false,
            forceUpdate = true,
        )
    }

    override fun computerRebuildQueueWhenIdle(): JSONObject {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        require(accountEpoch > 0L) { "Computer update queue requires a settled account epoch" }
        computerRebuildOwner.queueUpdateWhenIdle(accountEpoch)
        resumeQueuedComputerUpdateIfNeeded()
        return computerRebuildStatus()
    }

    override fun computerRebuildCancelQueuedUpdate(): JSONObject {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        computerRebuildOwner.cancelQueuedUpdate(accountEpoch)
        return computerRebuildStatus()
    }

    private fun dispatchComputerRebuildRequest(
        accountEpoch: Long,
        requestId: String,
        preserveData: Boolean,
        forceRecreate: Boolean,
        forceUpdate: Boolean = false,
    ): JSONObject {
        require(!(forceRecreate && forceUpdate)) {
            "Computer rebuild cannot force update and force recreate simultaneously"
        }
        val reply = try {
            host.requestComputerRebuild(
                requestId = requestId,
                preserveData = preserveData,
                forceRecreate = forceRecreate,
                forceUpdate = forceUpdate,
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
        val visibility = computerRebuildOwner.visibilityTelemetry(accountEpoch)
        return JSONObject()
            .put("accountEpoch", snapshot.accountEpoch)
            .put("processGeneration", snapshot.processGeneration)
            .put("requestId", snapshot.requestId ?: JSONObject.NULL)
            .put("queuedWhenIdle", snapshot.queuedWhenIdle)
            .put("operationId", snapshot.operationId ?: JSONObject.NULL)
            .put("migrationOffsetKey", snapshot.migrationOffsetKey)
            .put("pending", snapshot.pending)
            .put("acknowledged", snapshot.acknowledged)
            .put("outcomeUnknown", snapshot.outcomeUnknown)
            .put("kind", snapshot.kind?.name?.lowercase() ?: JSONObject.NULL)
            .put("source", snapshot.source?.name?.lowercase() ?: JSONObject.NULL)
            .put("boxPhase", snapshot.boxPhase ?: JSONObject.NULL)
            .put("imageUpdateAvailable", snapshot.imageUpdateAvailable ?: JSONObject.NULL)
            .put("pullPercent", snapshot.pullPercent ?: JSONObject.NULL)
            .put("migrationPhases", JSONArray(snapshot.migrationPhases.map { phase ->
                phase.name.lowercase().replace('_', '-')
            }))
            .put("connected", snapshot.connected)
            .put("leftHealthy", snapshot.leftHealthy)
            .put("teardown", snapshot.teardown.name.lowercase())
            .put("reconnectedSinceLeft", snapshot.reconnectedSinceLeft)
            .put("terminalMigration", snapshot.terminalMigration)
            .put("lastResolution", snapshot.lastResolution?.name?.lowercase() ?: JSONObject.NULL)
            .put("elapsedMs", visibility?.elapsedMs ?: JSONObject.NULL)
            .put("stageElapsedMs", visibility?.stageElapsedMs ?: JSONObject.NULL)
            .put("isStalled", visibility?.isStalled ?: false)
            .put("visibilityStage", visibility?.stage?.name?.lowercase()?.replace('_', '-') ?: JSONObject.NULL)
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
        reconciledCreateOutcome: Boolean,
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
            .put("reconciledCreateOutcome", reconciledCreateOutcome)
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
                    .onSuccess { observeRootConnectionTransport(RootConnectionTransportState.CONNECTED) }
                    .onFailure {
                        observeRootConnectionTransport(RootConnectionTransportState.DOWN)
                        Thread.sleep(20)
                    }
                while (!closed.get() && featureEventListeners.isNotEmpty()) {
                    val event = try {
                        host.request(
                            "feature.receive",
                            JSONObject().put("timeoutMs", 250),
                        ).also {
                            observeRootConnectionTransport(RootConnectionTransportState.CONNECTED)
                        }
                    } catch (_: Throwable) {
                        observeRootConnectionTransport(RootConnectionTransportState.DOWN)
                        runCatching { replayCoordinatorEvents() }
                            .onSuccess { observeRootConnectionTransport(RootConnectionTransportState.CONNECTED) }
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
                if (!closed.get() && featureEventListeners.isNotEmpty()) ensureFeatureEventPump()
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
        // Host-owned async task truth is reconstructed by a typed read rather than replaying
        // UI state. A feature-stream reconnect/resync therefore invalidates any subscribed
        // panel so it performs the Desktop-equivalent noteReconnect refresh.
        dispatchFeatureEvent(JSONObject().put("type", "agent.async-tasks.refresh"))
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
            while (!closed.get()) {
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
                val box = projectForeverBoxRebuildEvent(event) ?: return
                runCatching {
                    computerRebuildOwner.observeBox(
                        epoch,
                        box.boxId,
                        box.phase,
                        box.pullPercent,
                        box.imageUpdateAvailable,
                    )
                }
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

    private fun resumeQueuedComputerUpdateIfNeeded() {
        val accountEpoch = accountAccessOwner.currentProjection().accountEpoch
        val snapshot = runCatching { computerRebuildOwner.snapshot(accountEpoch) }.getOrNull() ?: return
        if (!snapshot.queuedWhenIdle || snapshot.kind != null || snapshot.pending || snapshot.outcomeUnknown) return
        if (!computerRebuildQueueRunning.compareAndSet(false, true)) return
        val expectedEpoch = accountEpoch
        val expectedGeneration = processGeneration
        computerRebuildQueueExecutor.execute {
            try {
                if (accountAccessOwner.currentProjection().accountEpoch != expectedEpoch) return@execute
                val current = computerRebuildOwner.snapshot(expectedEpoch)
                if (
                    current.processGeneration != expectedGeneration ||
                    !current.queuedWhenIdle ||
                    current.kind != null ||
                    current.pending ||
                    current.outcomeUnknown
                ) {
                    return@execute
                }
                if (current.imageUpdateAvailable == false) {
                    computerRebuildOwner.cancelQueuedUpdate(expectedEpoch)
                    return@execute
                }
                if (current.imageUpdateAvailable != true) return@execute

                val roster = host.requestValue("listAgents") as? JSONArray
                    ?: error("Canonical Agent roster is unavailable")
                var hasWorkingAgent = false
                for (index in 0 until roster.length()) {
                    val row = roster.optJSONObject(index) ?: continue
                    if (row.optBoolean("isRunning", false)) {
                        hasWorkingAgent = true
                        break
                    }
                }
                if (hasWorkingAgent) return@execute

                val requestId = "android-rebuild-$processGeneration-" + UUID.randomUUID().toString()
                computerRebuildOwner.reserveQueuedRequest(expectedEpoch, requestId)
                dispatchComputerRebuildRequest(
                    accountEpoch = expectedEpoch,
                    requestId = requestId,
                    preserveData = true,
                    forceRecreate = false,
                )
            } catch (_: Throwable) {
                // A queued intent is safe to retry only while no rebuild request has been reserved.
                // Once request identity is reserved, dispatch marks ambiguous transport as
                // outcome-unknown and this loop must not replay the side effect.
            } finally {
                computerRebuildQueueRunning.set(false)
                val pending = runCatching {
                    val epoch = accountAccessOwner.currentProjection().accountEpoch
                    if (epoch != expectedEpoch) null else computerRebuildOwner.snapshot(epoch)
                }.getOrNull()
                if (pending?.queuedWhenIdle == true) {
                    computerRebuildQueueExecutor.schedule(
                        { resumeQueuedComputerUpdateIfNeeded() },
                        COMPUTER_REBUILD_RECONNECT_MS,
                        TimeUnit.MILLISECONDS,
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

    override fun close() {
        if (!closed.compareAndSet(false, true)) return

        // Android may tear the process down without callbacks, but whenever the process runtime
        // is explicitly disposed we must mirror Desktop's coordinator-first quiesce semantics.
        // Stop producers before closing the two shared Host consumers so no background task can
        // race a destroyed JNI handle or recreate process-owned work after disposal.
        runCatching { connectivityManager.unregisterNetworkCallback(sendNetworkCallback) }
        featureEventListeners.clear()
        rootConnectionListeners.clear()
        eventPumpExecutor.shutdownNow()
        computerRebuildEventExecutor.shutdownNow()
        computerRebuildMigrationExecutor.shutdownNow()
        computerRebuildQueueExecutor.shutdownNow()
        remoteComputerReconnectExecutor.shutdownNow()
        storagePressureExecutor.shutdownNow()
        processMetricsExecutor.shutdownNow()
        processSessionHeartbeatExecutor.shutdownNow()
        sendSubmissionExecutor.shutdownNow()
        rootConnectionRetryExecutor.shutdownNow()
        computerRebuildEventHost.close()
        host.close()
        clearInstance(this)
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
        private const val PROCESS_METRICS_INITIAL_DELAY_SECONDS = 10L
        private const val PROCESS_METRICS_SAMPLE_SECONDS = 10L
        private const val PROCESS_SESSION_HEARTBEAT_SECONDS = 60L
        private const val COMPUTER_REBUILD_RECONNECT_MS = 3_000L
        @Volatile private var instance: AndroidCoordinatorRuntime? = null

        fun get(application: Application): AndroidCoordinatorRuntime =
            instance ?: synchronized(this) {
                instance ?: AndroidCoordinatorRuntime(application).also { instance = it }
            }

        fun closeProcessRuntime() {
            instance?.close()
        }

        private fun clearInstance(expected: AndroidCoordinatorRuntime) {
            synchronized(this) {
                if (instance === expected) instance = null
            }
        }
    }
}
