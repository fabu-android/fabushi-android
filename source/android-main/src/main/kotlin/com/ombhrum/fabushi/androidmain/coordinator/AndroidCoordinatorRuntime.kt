package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidMcpOAuthCompletion
import com.ombhrum.fabushi.core.MahayanaHost
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
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
    private val featureEventListeners = CopyOnWriteArrayList<(JSONObject) -> Unit>()
    private val eventPumpRunning = AtomicBoolean(false)
    private val lastEventSequence = AtomicLong(0L)
    private val eventPumpExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-coordinator-feature-events").apply { isDaemon = true }
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

    override fun authStatus() = host.request("feature.auth.status")
    override fun authDeviceAgentSession() = host.request("feature.auth.deviceAgentSession")
    override fun authBrowserStart() = host.request("feature.auth.browserStart")
    override fun authBrowserReopen(params: JSONObject) = host.request("feature.auth.browserReopen", params)
    override fun authBrowserCancel(params: JSONObject) = host.request("feature.auth.browserCancel", params)
    override fun authBrowserPoll(params: JSONObject) = host.request("feature.auth.browserPoll", params)
    override fun authLogout() = host.request("feature.auth.logout")
    override fun automationUpsert(params: JSONObject) = host.request("feature.automation.upsert", params)
    override fun automationList(): JSONArray = host.requestValue("feature.automation.list") as? JSONArray ?: JSONArray()
    override fun automationStart(params: JSONObject) = host.request("feature.automation.start", params)
    override fun automationAdvanceStep(params: JSONObject) = host.request("feature.automation.advanceStep", params)
    override fun automationAwaitApproval(params: JSONObject) = host.request("feature.automation.awaitApproval", params)
    override fun automationResolveApproval(params: JSONObject) = host.request("feature.automation.resolveApproval", params)
    override fun automationCancel(params: JSONObject) = host.request("feature.automation.cancel", params)
    override fun automationSettle(params: JSONObject) = host.request("feature.automation.settle", params)
    override fun automationSnapshot(params: JSONObject) = host.request("feature.automation.snapshot", params)

    override fun featureExecute(params: JSONObject) = host.request("feature.execute", params)
    override fun featureInterrupt(params: JSONObject) = host.request("feature.interrupt", params)
    override fun transcriptSnapshot(): JSONArray =
        host.requestValue("feature.transcript.snapshot") as? JSONArray ?: JSONArray()

    override fun agentList(): JSONArray =
        host.requestValue("listAgents") as? JSONArray ?: JSONArray()

    override fun agentCreate(name: String, description: String): JSONObject =
        host.request(
            "createAgent",
            JSONObject()
                .put("name", name)
                .put("description", description)
                .put("origin", "user"),
        )

    override fun agentUpdate(id: String, name: String, description: String): JSONObject =
        host.request(
            "updateAgent",
            JSONObject()
                .put("id", id)
                .put(
                    "profile",
                    JSONObject()
                        .put("name", name)
                        .put("description", description),
                ),
        )

    override fun agentSetHidden(id: String, isHidden: Boolean): JSONObject =
        host.request(
            "setAgentHiddenFromSidebar",
            JSONObject().put("id", id).put("isHidden", isHidden),
        )

    override fun agentSetUnread(id: String, isUnread: Boolean): JSONObject =
        host.request(
            "setAgentUnread",
            JSONObject().put("id", id).put("isUnread", isUnread),
        )

    override fun agentDuplicate(id: String): JSONObject =
        host.request("duplicateAgent", JSONObject().put("id", id))

    override fun agentDelete(id: String): JSONObject =
        host.request(
            "deleteAgents",
            JSONObject().put("ids", JSONArray().put(id)),
        )

    override fun agentSetPinned(ids: List<String>): List<String> {
        val values = JSONArray()
        ids.forEach(values::put)
        val result = host.requestValue(
            "setPinnedAgents",
            JSONObject().put("ids", values),
        ) as? JSONArray ?: JSONArray()
        return buildList {
            for (index in 0 until result.length()) {
                result.optString(index).takeIf(String::isNotBlank)?.let(::add)
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
        @Volatile private var instance: AndroidCoordinatorRuntime? = null

        fun get(application: Application): AndroidCoordinatorRuntime =
            instance ?: synchronized(this) {
                instance ?: AndroidCoordinatorRuntime(application).also { instance = it }
            }
    }
}
