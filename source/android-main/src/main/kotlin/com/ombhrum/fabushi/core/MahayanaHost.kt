package com.ombhrum.fabushi.core

import android.content.Context
import com.ombhrum.fabushi.androidmain.security.AndroidAccountSessionStore
import com.ombhrum.fabushi.androidmain.security.AndroidPluginVariableSecretStore
import com.ombhrum.fabushi.androidmain.security.AndroidRemoteBindingStore
import org.json.JSONObject
import java.io.Closeable
import java.util.ArrayDeque
import java.util.UUID

/**
 * Android process-owned Mahayana AppHost session.
 *
 * Production callers share one native AppHost handle for the same app-data root so Marketplace,
 * Messenger Bot and Mini App WebMCP operate on one auth/runtime/event truth. Each Kotlin caller has
 * an independent bounded feature-event cursor: whichever caller drains the native `feature.receive`
 * queue fans that event out to every other caller, preventing competing ViewModels from stealing
 * operation events from one another. Deterministic feature-host tests stay isolated by design.
 */
class MahayanaHost(
    context: Context,
    private val featureHostTest: Boolean = false,
    processGeneration: Long = 1L,
) : Closeable {
    private val appDataDir = context.filesDir.absolutePath
    private val accountSessionStore = AndroidAccountSessionStore(context)
    private val pluginSecretStore = AndroidPluginVariableSecretStore(context)
    private val remoteBindingStore = AndroidRemoteBindingStore(context)
    private val consumerId = UUID.randomUUID().toString()
    private val ownedListenerIds = mutableSetOf<String>()
    private val shared: SharedHost?
    @Volatile private var isolatedHandle: Long = 0L
    @Volatile private var closed = false

    init {
        System.loadLibrary("mahayana_app_host")
        if (featureHostTest) {
            val value = nativeCreateTest(appDataDir)
            check(value != 0L) { "Failed to initialize Mahayana Rust test host" }
            isolatedHandle = value
            shared = null
        } else {
            shared = synchronized(registryLock) {
                val existing = sharedHosts[appDataDir]
                if (existing != null) {
                    check(existing.generation == processGeneration) {
                        "Mahayana native runtime generation mismatch"
                    }
                    existing.refCount += 1
                    existing.eventQueues.putIfAbsent(consumerId, ArrayDeque())
                    existing
                } else {
                    require(processGeneration > 0L) { "processGeneration must be positive" }
                    val value = nativeCreate(
                        appDataDir,
                        processGeneration,
                        accountSessionStore.readSessionJson().orEmpty(),
                    )
                    check(value != 0L) { "Failed to initialize Mahayana Rust host" }
                    SharedHost(handle = value, generation = processGeneration).also { created ->
                        created.refCount = 1
                        created.eventQueues[consumerId] = ArrayDeque()
                        sharedHosts[appDataDir] = created
                    }
                }
            }
        }
        if (!featureHostTest) {
            val state = checkNotNull(shared)
            synchronized(state.lock) {
                installProtectedRemoteBindingForCurrentAccountLocked(state)
            }
        }
    }

    internal fun refreshProtectedRemoteBinding() {
        check(!featureHostTest && !closed) { "Protected Remote binding refresh requires production Host" }
        val state = checkNotNull(shared)
        synchronized(state.lock) {
            installProtectedRemoteBindingForCurrentAccountLocked(state)
        }
    }

    /**
     * The native Host is the canonical account-fence owner. Protected Remote
     * credentials may only be reinstalled after that fence is read from the live
     * Host and matched by the platform secret store. This keeps Presentation out
     * of the credential lifecycle and makes stale bindings fail closed.
     */
    private fun installProtectedRemoteBindingForCurrentAccountLocked(state: SharedHost) {
        check(state.handle != 0L) { "Mahayana host is closed" }
        val fenceRequest = JSONObject()
            .put("method", "feature.account.fence")
            .put("params", JSONObject())
        val fenceResponse = JSONObject(nativeDispatch(state.handle, fenceRequest.toString()))
        val currentFence = if (fenceResponse.optBoolean("ok", false)) {
            fenceResponse.optJSONObject("result")
                ?.optString("accountFence")
                ?.trim()
                ?.takeIf(String::isNotEmpty)
        } else {
            null
        }
        val protectedBinding = if (currentFence == null) {
            remoteBindingStore.clear()
            ""
        } else {
            remoteBindingStore.readBindingJsonForAccountFence(currentFence).orEmpty()
        }
        check(nativeSetRemoteBinding(state.handle, protectedBinding)) {
            "Protected Remote binding was rejected by native Host"
        }
    }

    fun request(method: String, params: JSONObject = JSONObject()): JSONObject {
        check(!closed) { "Mahayana host is closed" }
        if (!featureHostTest && method == "feature.receive") {
            return receiveShared(params)
        }
        if (!featureHostTest && method == "feature.plugin.variables.configure") {
            return configurePluginVariables(params)
        }
        if (!featureHostTest && method == "runtime.start") {
            return startRuntimeWithProtectedVariables(params)
        }
        val response = dispatch(method, params)
        return response.optJSONObject("result") ?: JSONObject().put("value", response.opt("result"))
    }

    fun coordinatorStatus(): JSONObject = request("coordinator.status")

    fun coordinatorResync(generation: Long, afterSequence: Long): JSONObject =
        request(
            "coordinator.resync",
            JSONObject()
                .put("generation", generation)
                .put("afterSequence", afterSequence),
        )

    fun requestValue(method: String, params: JSONObject = JSONObject()): Any? {
        check(!closed) { "Mahayana host is closed" }
        if (!featureHostTest && method == "feature.receive") return receiveShared(params)
        return dispatch(method, params).opt("result")
    }

    /**
     * Publish an Android Host-adapter event into the same process-owned event truth as native
     * FeatureHost events. This is used for transports such as official Streamable HTTP MCP whose
     * work is still Host-owned but does not traverse the native FeatureHost queue itself.
     */
    fun publishFeatureEvent(event: JSONObject) {
        check(!closed) { "Mahayana host is closed" }
        if (featureHostTest || event.optString("type").isBlank()) return
        val state = checkNotNull(shared)
        val listeners: List<(JSONObject) -> Unit>
        val serialized: String
        synchronized(state.lock) {
            check(state.handle != 0L) { "Mahayana host is closed" }
            val request = JSONObject()
                .put("method", "coordinator.publishEvent")
                .put("params", JSONObject().put("event", event))
            val response = JSONObject(nativeDispatch(state.handle, request.toString()))
            check(response.optBoolean("ok", false)) {
                response.optString("error", "Coordinator rejected Android adapter event")
            }
            val projected = JSONObject(event.toString())
            response.optJSONObject("result")?.let { metadata ->
                projected.put("_coordinator", JSONObject(metadata.toString()))
            }
            serialized = projected.toString()
            state.eventQueues.values.forEach { target ->
                if (target.size >= MAX_REPLAY_EVENTS) target.pollFirst()
                target.addLast(JSONObject(serialized))
            }
            listeners = state.listeners.values.toList()
        }
        listeners.forEach { listener ->
            runCatching { listener(JSONObject(serialized)) }
        }
    }

    /**
     * Observe the same FeatureHost events consumed by Messenger/Marketplace without starting a
     * second native event pump. Listeners never receive credentials; they see only FeatureHost
     * event envelopes already exposed to app surfaces.
     */
    fun addFeatureEventListener(listener: (JSONObject) -> Unit): AutoCloseable {
        check(!closed) { "Mahayana host is closed" }
        val state = shared ?: return AutoCloseable { }
        val listenerId = UUID.randomUUID().toString()
        synchronized(state.lock) {
            state.listeners[listenerId] = listener
            ownedListenerIds += listenerId
        }
        return AutoCloseable {
            synchronized(state.lock) {
                state.listeners.remove(listenerId)
                ownedListenerIds.remove(listenerId)
            }
        }
    }


    private fun configurePluginVariables(params: JSONObject): JSONObject {
        val preparedResponse = dispatch("feature.plugin.variables.prepare", JSONObject(params.toString()))
        val prepared = preparedResponse.optJSONObject("result")
            ?: error("Mahayana Host did not return a plugin-variable prepare result")
        val writeId = prepared.getString("writeId")
        val pluginId = prepared.getString("pluginId")
        val accountKey = prepared.getString("accountKey")
        val secretValues = prepared.optJSONObject("secretValues") ?: JSONObject()
        pluginSecretStore.replace(accountKey, pluginId, secretValues)
        val committedResponse = dispatch(
            "feature.plugin.variables.commit",
            JSONObject().put("writeId", writeId),
        )
        return committedResponse.optJSONObject("result")
            ?: error("Mahayana Host did not commit plugin variables")
    }

    private fun startRuntimeWithProtectedVariables(params: JSONObject): JSONObject {
        val pluginId = params.optString("pluginId").takeIf(String::isNotBlank)
            ?: error("runtime.start requires pluginId")
        val projectionResponse = dispatch(
            "feature.plugin.variables.runtimeConfig",
            JSONObject().put("pluginId", pluginId),
        )
        val projection = projectionResponse.optJSONObject("result")
            ?: error("Mahayana Host did not return plugin variable runtime config")
        val config = JSONObject(
            (projection.optJSONObject("publicConfig") ?: JSONObject()).toString(),
        )
        if (projection.optBoolean("configured", false)) {
            val accountKey = projection.getString("accountKey")
            val secretKeys = projection.optJSONArray("secretKeys") ?: org.json.JSONArray()
            val secrets = pluginSecretStore.read(accountKey, pluginId, secretKeys)
            val keys = secrets.keys()
            while (keys.hasNext()) {
                val key = keys.next()
                config.put(key, secrets.getString(key))
            }
        }
        val runtimeParams = JSONObject(params.toString()).apply {
            remove("config")
            put("config", config)
        }
        val response = dispatch("runtime.start", runtimeParams)
        return response.optJSONObject("result")
            ?: error("Mahayana Host did not return runtime.start result")
    }

    private fun dispatch(method: String, params: JSONObject): JSONObject {
        val request = JSONObject().put("method", method).put("params", params)
        val response = if (featureHostTest) {
            synchronized(this) {
                val active = isolatedHandle
                check(active != 0L) { "Mahayana host is closed" }
                JSONObject(nativeDispatch(active, request.toString()))
            }
        } else {
            val state = checkNotNull(shared)
            preSignalRuntimeControl(method, params, state)?.let { return it }
            synchronized(state.lock) {
                check(state.handle != 0L) { "Mahayana host is closed" }
                JSONObject(nativeDispatch(state.handle, request.toString()))
            }
        }
        if (!response.optBoolean("ok", false)) {
            error(response.optString("error", "Mahayana host request failed"))
        }
        consumePrivateAccountSessionMutation(response)
        return response
    }

    private fun preSignalRuntimeControl(
        method: String,
        params: JSONObject,
        state: SharedHost,
    ): JSONObject? {
        val active = state.handle
        if (active == 0L) return null
        when (method) {
            "runtime.cancel" -> {
                val requestId = params.optString("requestId").takeIf { it.isNotBlank() } ?: return null
                if (nativeSignalRuntimeCancel(active, requestId)) {
                    return JSONObject()
                        .put("ok", true)
                        .put(
                            "result",
                            JSONObject()
                                .put("requestId", requestId)
                                .put("cancelled", true)
                                .put("pendingSettlement", true),
                        )
                }
            }
            "runtime.stop" -> {
                params.optString("pluginId")
                    .takeIf { it.isNotBlank() }
                    ?.let { nativeSignalRuntimePluginCancel(active, it) }
            }
            "plugin.permission.revoke" -> {
                val pluginId = params.optString("pluginId").takeIf { it.isNotBlank() }
                val permission = params.optString("permission").takeIf { it.isNotBlank() }
                if (pluginId != null && permission != null) {
                    nativeSignalRuntimePermissionCancel(active, pluginId, permission)
                }
            }
            "feature.auth.logout" -> nativeSignalAllRuntimeCalls(active)
        }
        return null
    }

    private fun consumePrivateAccountSessionMutation(response: JSONObject) {
        val result = response.optJSONObject("result") ?: return
        val mutation = result.optJSONObject("_accountSessionMutation") ?: return
        when (mutation.optString("action")) {
            "save" -> {
                accountSessionStore.writeSessionJson(mutation.getString("sessionJson"))
                shared?.let { state ->
                    synchronized(state.lock) {
                        installProtectedRemoteBindingForCurrentAccountLocked(state)
                    }
                }
            }
            "clear" -> {
                accountSessionStore.clear()
                remoteBindingStore.clear()
                shared?.let { state ->
                    synchronized(state.lock) {
                        if (state.handle != 0L) nativeSetRemoteBinding(state.handle, "")
                    }
                }
            }
            else -> error("Unknown private account-session mutation")
        }
        result.remove("_accountSessionMutation")
    }

    private fun receiveShared(params: JSONObject): JSONObject {
        val state = checkNotNull(shared)
        val listeners: List<(JSONObject) -> Unit>
        val event: JSONObject
        synchronized(state.lock) {
            check(!closed && state.handle != 0L) { "Mahayana host is closed" }
            val queue = state.eventQueues.getOrPut(consumerId) { ArrayDeque() }
            val queued = queue.pollFirst()
            if (queued != null) return JSONObject(queued.toString())

            val request = JSONObject().put("method", "feature.receive").put("params", params)
            val response = JSONObject(nativeDispatch(state.handle, request.toString()))
            if (!response.optBoolean("ok", false)) {
                error(response.optString("error", "Mahayana host request failed"))
            }
            event = response.optJSONObject("result") ?: JSONObject().put("value", response.opt("result"))
            if (event.optString("type").isNotBlank()) {
                val serialized = event.toString()
                state.eventQueues.forEach { (id, target) ->
                    if (id != consumerId) {
                        if (target.size >= MAX_REPLAY_EVENTS) target.pollFirst()
                        target.addLast(JSONObject(serialized))
                    }
                }
                listeners = state.listeners.values.toList()
            } else {
                listeners = emptyList()
            }
        }
        if (listeners.isNotEmpty()) {
            val serialized = event.toString()
            listeners.forEach { listener ->
                runCatching { listener(JSONObject(serialized)) }
            }
        }
        return event
    }

    override fun close() {
        if (closed) return
        closed = true
        if (featureHostTest) {
            val observed = isolatedHandle
            if (observed != 0L) nativeSignalAllRuntimeCalls(observed)
            synchronized(this) {
                val active = isolatedHandle
                isolatedHandle = 0L
                if (active != 0L) nativeDestroy(active)
            }
            return
        }

        val state = shared ?: return
        synchronized(registryLock) {
            // When this is the last Java/Kotlin owner, dispose must be able to cancel an
            // in-flight runtime.call before waiting for the single mutable Host lock.
            if (state.refCount <= 1 && state.handle != 0L) {
                nativeSignalAllRuntimeCalls(state.handle)
            }
            synchronized(state.lock) {
                state.eventQueues.remove(consumerId)
                ownedListenerIds.forEach(state.listeners::remove)
                ownedListenerIds.clear()
                state.refCount -= 1
                if (state.refCount <= 0) {
                    val active = state.handle
                    state.handle = 0L
                    sharedHosts.remove(appDataDir)
                    if (active != 0L) nativeDestroy(active)
                }
            }
        }
    }

    private class SharedHost(
        @Volatile var handle: Long,
        val generation: Long,
        var refCount: Int = 0,
        val lock: Any = Any(),
        val eventQueues: MutableMap<String, ArrayDeque<JSONObject>> = linkedMapOf(),
        val listeners: MutableMap<String, (JSONObject) -> Unit> = linkedMapOf(),
    )

    private external fun nativeCreate(
        appDataDir: String,
        processGeneration: Long,
        initialAccountSessionJson: String,
    ): Long
    private external fun nativeCreateTest(appDataDir: String): Long
    private external fun nativeDispatch(handle: Long, requestJson: String): String
    private external fun nativeSetRemoteBinding(handle: Long, bindingJson: String): Boolean
    private external fun nativeSignalRuntimeCancel(handle: Long, requestId: String): Boolean
    private external fun nativeSignalRuntimePluginCancel(handle: Long, pluginId: String): Int
    private external fun nativeSignalRuntimePermissionCancel(
        handle: Long,
        pluginId: String,
        permission: String,
    ): Int
    private external fun nativeSignalAllRuntimeCalls(handle: Long): Int
    private external fun nativeDestroy(handle: Long)

    private companion object {
        const val MAX_REPLAY_EVENTS = 256
        val registryLock = Any()
        val sharedHosts = mutableMapOf<String, SharedHost>()
    }
}
