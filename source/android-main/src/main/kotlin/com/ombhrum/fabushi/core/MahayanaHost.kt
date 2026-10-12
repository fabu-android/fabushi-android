package com.ombhrum.fabushi.core

import android.content.Context
import com.ombhrum.fabushi.androidmain.security.AndroidAccountSessionStore
import com.ombhrum.fabushi.androidmain.security.AndroidMachineIdStore
import com.ombhrum.fabushi.androidmain.security.AndroidCredentialVaultStore
import com.ombhrum.fabushi.androidmain.security.AndroidPluginVariableSecretStore
import com.ombhrum.fabushi.androidmain.security.AndroidRemoteBindingStore
import com.ombhrum.fabushi.androidmain.startup.AndroidDataRootBootstrap
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
    private val appDataDir = AndroidDataRootBootstrap.settle(context).absolutePath
    private val accountSessionStore = AndroidAccountSessionStore(context)
    private val machineIdStore = AndroidMachineIdStore(context)
    private val pluginSecretStore = AndroidPluginVariableSecretStore(context)
    private val credentialVaultStore = AndroidCredentialVaultStore(context)
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
                    val initialAccountSessionJson = accountSessionStore.readSessionJson().orEmpty()
                    val value = nativeCreate(
                        appDataDir,
                        processGeneration,
                        initialAccountSessionJson,
                        machineIdStore.getOrCreate(),
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
            reportProtectedAccountStoreUnavailableIfNeeded()
            val state = checkNotNull(shared)
            synchronized(state.lock) {
                installProtectedRemoteBindingForCurrentAccountLocked(state)
            }
        }
    }

    /**
     * Bridges the Android-only credential protection failure into the canonical Rust Host
     * telemetry owner. This is intentionally internal platform plumbing: Presentation never owns
     * or synthesizes account-store health.
     */
    private fun reportProtectedAccountStoreUnavailableIfNeeded() {
        if (!accountSessionStore.consumeProtectedStoreUnavailable()) return
        val state = shared ?: return
        synchronized(state.lock) {
            if (state.handle == 0L) return
            val request = JSONObject()
                .put("method", "feature.account.telemetry.protectedStoreUnavailable")
                .put("params", JSONObject())
            // Telemetry reporting must not replace the original account-store outcome.
            runCatching { nativeDispatch(state.handle, request.toString()) }
        }
    }

    internal fun refreshProtectedRemoteBinding() {
        check(!featureHostTest && !closed) { "Protected Remote binding refresh requires production Host" }
        val state = checkNotNull(shared)
        synchronized(state.lock) {
            installProtectedRemoteBindingForCurrentAccountLocked(state)
        }
    }

    internal fun clearProtectedRemoteBinding() {
        check(!featureHostTest && !closed) { "Protected Remote binding clear requires production Host" }
        remoteBindingStore.clear()
        val state = checkNotNull(shared)
        synchronized(state.lock) {
            check(state.handle != 0L) { "Mahayana host is closed" }
            check(nativeSetRemoteBinding(state.handle, "")) {
                "Native Host rejected protected Remote binding clear"
            }
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
        if (!featureHostTest && method.startsWith("feature.credentialVault.")) {
            return credentialVaultRequest(method, params)
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

    internal fun requestComputerRebuild(
        requestId: String,
        preserveData: Boolean,
        forceRecreate: Boolean,
        forceUpdate: Boolean = false,
    ): JSONObject {
        check(!featureHostTest && !closed) { "Computer rebuild requires production Host" }
        // Resolve the canonical Host account fence before dispatching a side effect. The backend
        // uses its own Sand credential; this check is solely the Android account/epoch fence.
        request("feature.account.fence").getString("accountFence")
        val state = checkNotNull(shared)
        val active = synchronized(state.lock) {
            check(state.handle != 0L) { "Mahayana host is closed" }
            state.handle
        }
        val response = JSONObject(
            nativeComputerRebuildRequest(active, requestId, preserveData, forceRecreate, forceUpdate),
        )
        check(response.optBoolean("ok", false)) {
            response.optString("error", "Computer rebuild backend request failed")
        }
        return response.optJSONObject("result")
            ?: error("Computer rebuild backend returned no result")
    }

    internal fun watchComputerRebuildOnce(
        requestId: String,
        fromOffsetKey: String,
    ): JSONObject {
        check(!featureHostTest && !closed) { "Computer rebuild watch requires production Host" }
        val state = checkNotNull(shared)
        val active = synchronized(state.lock) {
            check(state.handle != 0L) { "Mahayana host is closed" }
            state.handle
        }
        // The JNI backend is independent of the mutable Host runtime, so the 30-second streaming
        // stall watchdog never holds SharedHost.lock and cannot block chat/tool/account dispatch.
        val response = JSONObject(
            nativeComputerRebuildWatchOnce(active, requestId, fromOffsetKey),
        )
        check(response.optBoolean("ok", false)) {
            response.optString("error", "Computer rebuild migration watch failed")
        }
        return response.optJSONObject("result")
            ?: error("Computer rebuild migration watch returned no event")
    }

    internal fun probeUpdatePolicy(requestId: String): JSONObject {
        check(!featureHostTest && !closed) { "Update policy probe requires production Host" }
        val state = checkNotNull(shared)
        val active = synchronized(state.lock) {
            check(state.handle != 0L) { "Mahayana host is closed" }
            state.handle
        }
        val response = JSONObject(nativeProbeUpdatePolicy(active, requestId))
        check(response.optBoolean("ok", false)) {
            response.optString("error", "Native update-policy probe failed")
        }
        return response.optJSONObject("result")
            ?: error("Native update-policy probe returned no result")
    }

    internal fun resolveUpdateTrackPolicy(
        managedTrack: String?,
        unlockInternalTracks: Boolean,
        userOverride: String?,
        buildDefault: String?,
    ): JSONObject {
        check(!closed) { "Mahayana host is closed" }
        val response = JSONObject(
            nativeResolveUpdateTrackPolicy(
                managedTrack.orEmpty(),
                unlockInternalTracks,
                userOverride.orEmpty(),
                buildDefault.orEmpty(),
            ),
        )
        check(response.optBoolean("ok", false)) {
            response.optString("error", "Native update-track policy projection failed")
        }
        return response.optJSONObject("result")
            ?: error("Native update-track policy projection returned no result")
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


    private fun credentialVaultRequest(method: String, params: JSONObject): JSONObject {
        val accountFence = currentCredentialAccountFence()
        return when (method) {
            "feature.credentialVault.list" -> JSONObject().put(
                "credentials",
                credentialVaultStore.listMetadata(accountFence),
            )
            "feature.credentialVault.upsert", "feature.credentialVault.rotate" -> {
                val name = params.optString("name").takeIf(String::isNotBlank)
                    ?: error("Credential Vault requires a secret reference")
                val secretValue = params.optString("secretValue").takeIf(String::isNotEmpty)
                    ?: error("Credential Vault requires a secret value")
                val originsJson = params.optJSONArray("allowedOrigins")
                    ?: error("Credential Vault requires allowedOrigins")
                val origins = buildList {
                    for (index in 0 until originsJson.length()) add(originsJson.getString(index))
                }
                val metadata = if (method.endsWith("rotate")) {
                    credentialVaultStore.rotate(
                        accountFence,
                        name,
                        params.optString("label"),
                        secretValue,
                        origins,
                    )
                } else {
                    credentialVaultStore.upsert(
                        accountFence,
                        name,
                        params.optString("label"),
                        secretValue,
                        origins,
                    )
                }
                JSONObject().put("credential", metadata)
            }
            "feature.credentialVault.revoke" -> {
                val name = params.optString("name").takeIf(String::isNotBlank)
                    ?: error("Credential Vault requires a secret reference")
                JSONObject().put("revoked", credentialVaultStore.revoke(accountFence, name))
            }
            else -> error("Unsupported Credential Vault operation")
        }
    }

    /** Internal execution-boundary consumption; Presentation has no secret reveal route. */
    internal fun credentialForOrigin(secretRef: String, origin: String): String {
        check(!featureHostTest && !closed) { "Credential resolution requires production Host" }
        return credentialVaultStore.resolveForOrigin(currentCredentialAccountFence(), secretRef, origin)
    }

    private fun currentCredentialAccountFence(): String {
        val response = dispatch("feature.account.fence", JSONObject())
        return response.optJSONObject("result")
            ?.optString("accountFence")
            ?.trim()
            ?.takeIf(String::isNotEmpty)
            ?: error("Credential Vault requires a signed-in account")
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
                try {
                    accountSessionStore.writeSessionJson(mutation.getString("sessionJson"))
                } catch (error: Throwable) {
                    reportProtectedAccountStoreUnavailableIfNeeded()
                    throw error
                }
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
        machineId: String,
    ): Long
    private external fun nativeCreateTest(appDataDir: String): Long
    private external fun nativeDispatch(handle: Long, requestJson: String): String
    private external fun nativeSetRemoteBinding(handle: Long, bindingJson: String): Boolean
    private external fun nativeProbeUpdatePolicy(handle: Long, requestId: String): String
    private external fun nativeResolveUpdateTrackPolicy(
        managedTrack: String,
        unlockInternalTracks: Boolean,
        userOverride: String,
        buildDefault: String,
    ): String
    private external fun nativeComputerRebuildRequest(
        handle: Long,
        requestId: String,
        preserveData: Boolean,
        forceRecreate: Boolean,
        forceUpdate: Boolean,
    ): String
    private external fun nativeComputerRebuildWatchOnce(
        handle: Long,
        requestId: String,
        fromOffsetKey: String,
    ): String
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
