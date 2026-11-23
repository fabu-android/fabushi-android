package com.ombhrum.fabushi.core

import android.content.Context
import org.json.JSONObject
import java.io.Closeable
import java.util.ArrayDeque
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.withLock

/**
 * Android process-owned Mahayana AppHost session.
 *
 * Production callers share one native AppHost handle for the same app-data root so Marketplace,
 * Messenger Bot and Mini App WebMCP operate on one auth/runtime/event truth. Each Kotlin caller has
 * an independent bounded feature-event cursor: whichever caller drains the native `feature.receive`
 * queue fans that event out to every other caller, preventing competing ViewModels from stealing
 * operation events from one another. Deterministic feature-host tests stay isolated by design.
 *
 * Concurrency: `nativeDispatch` acquires a shared read lock so multiple JNI calls (including
 * blocking HTTP RPCs where Rust releases its internal SQLite mutex) run concurrently without
 * stalling each other. `close()` acquires the exclusive write lock outside `registryLock` so
 * `nativeDestroy` waits cleanly for any in-flight dispatches without blocking unrelated hosts.
 */
class MahayanaHost(context: Context, private val featureHostTest: Boolean = false) : Closeable {
    private val appDataDir = context.filesDir.absolutePath
    private val consumerId = UUID.randomUUID().toString()
    private val ownedListenerIds = mutableSetOf<String>()
    private val shared: SharedHost?
    private val isolatedHandleLock = ReentrantReadWriteLock()
    @Volatile private var isolatedHandle: Long = 0L
    private val closed = AtomicBoolean(false)

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
                    synchronized(existing.eventLock) {
                        existing.refCount += 1
                        existing.eventQueues.putIfAbsent(consumerId, ArrayDeque())
                    }
                    existing
                } else {
                    val value = nativeCreate(appDataDir)
                    check(value != 0L) { "Failed to initialize Mahayana Rust host" }
                    SharedHost(handle = value).also { created ->
                        created.refCount = 1
                        created.eventQueues[consumerId] = ArrayDeque()
                        sharedHosts[appDataDir] = created
                    }
                }
            }
        }
    }

    fun request(method: String, params: JSONObject = JSONObject()): JSONObject {
        check(!closed.get()) { "Mahayana host is closed" }
        if (!featureHostTest && method == "feature.receive") {
            return receiveShared(params)
        }
        val response = dispatch(method, params)
        return response.optJSONObject("result") ?: JSONObject().put("value", response.opt("result"))
    }

    fun requestResult(method: String, params: JSONObject = JSONObject()): Result<JSONObject> =
        runCatching { request(method, params) }

    fun requestValue(method: String, params: JSONObject = JSONObject()): Any? {
        check(!closed.get()) { "Mahayana host is closed" }
        if (!featureHostTest && method == "feature.receive") return receiveShared(params)
        return dispatch(method, params).opt("result")
    }

    /**
     * Publish an Android Host-adapter event into the same process-owned event truth as native
     * FeatureHost events. This is used for transports such as official Streamable HTTP MCP whose
     * work is still Host-owned but does not traverse the native FeatureHost queue itself.
     */
    fun publishFeatureEvent(event: JSONObject) {
        check(!closed.get()) { "Mahayana host is closed" }
        if (featureHostTest || event.optString("type").isBlank()) return
        val state = checkNotNull(shared)
        val listeners: List<(JSONObject) -> Unit>
        val serialized = event.toString()
        synchronized(state.eventLock) {
            check(!closed.get() && state.handle != 0L) { "Mahayana host is closed" }
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
        check(!closed.get()) { "Mahayana host is closed" }
        val state = shared ?: return AutoCloseable { }
        val listenerId = UUID.randomUUID().toString()
        synchronized(state.eventLock) {
            check(!closed.get() && state.handle != 0L) { "Mahayana host is closed" }
            state.listeners[listenerId] = listener
            ownedListenerIds += listenerId
        }
        return AutoCloseable {
            synchronized(state.eventLock) {
                state.listeners.remove(listenerId)
                ownedListenerIds.remove(listenerId)
            }
        }
    }

    private fun dispatch(method: String, params: JSONObject): JSONObject {
        val request = JSONObject().put("method", method).put("params", params)
        val response = if (featureHostTest) {
            isolatedHandleLock.readLock().withLock {
                val active = isolatedHandle
                check(!closed.get() && active != 0L) { "Mahayana host is closed" }
                JSONObject(nativeDispatch(active, request.toString()))
            }
        } else {
            val state = checkNotNull(shared)
            state.handleLock.readLock().withLock {
                val active = state.handle
                check(!closed.get() && active != 0L) { "Mahayana host is closed" }
                JSONObject(nativeDispatch(active, request.toString()))
            }
        }
        if (!response.optBoolean("ok", false)) {
            error(response.optString("error", "Mahayana host request failed"))
        }
        return response
    }

    private fun receiveShared(params: JSONObject): JSONObject {
        val state = checkNotNull(shared)
        val queued = synchronized(state.eventLock) {
            check(!closed.get() && state.handle != 0L) { "Mahayana host is closed" }
            state.eventQueues.getOrPut(consumerId) { ArrayDeque() }.pollFirst()
        }
        if (queued != null) return JSONObject(queued.toString())

        val request = JSONObject().put("method", "feature.receive").put("params", params)
        val response = state.handleLock.readLock().withLock {
            val active = state.handle
            check(!closed.get() && active != 0L) { "Mahayana host is closed" }
            JSONObject(nativeDispatch(active, request.toString()))
        }
        if (!response.optBoolean("ok", false)) {
            error(response.optString("error", "Mahayana host request failed"))
        }
        val event = response.optJSONObject("result") ?: JSONObject().put("value", response.opt("result"))
        val listeners: List<(JSONObject) -> Unit> = if (event.optString("type").isNotBlank()) {
            val serialized = event.toString()
            synchronized(state.eventLock) {
                state.eventQueues.forEach { (id, target) ->
                    if (id != consumerId) {
                        if (target.size >= MAX_REPLAY_EVENTS) target.pollFirst()
                        target.addLast(JSONObject(serialized))
                    }
                }
                state.listeners.values.toList()
            }
        } else {
            emptyList()
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
        if (!closed.compareAndSet(false, true)) return
        if (featureHostTest) {
            val active = isolatedHandleLock.writeLock().withLock {
                val h = isolatedHandle
                isolatedHandle = 0L
                h
            }
            if (active != 0L) nativeDestroy(active)
            return
        }

        val state = shared ?: return
        var shouldDestroy = false
        synchronized(registryLock) {
            synchronized(state.eventLock) {
                state.eventQueues.remove(consumerId)
                ownedListenerIds.forEach(state.listeners::remove)
                ownedListenerIds.clear()
                state.refCount -= 1
                if (state.refCount <= 0) {
                    sharedHosts.remove(appDataDir)
                    shouldDestroy = true
                }
            }
        }
        if (shouldDestroy) {
            val active = state.handleLock.writeLock().withLock {
                val h = state.handle
                state.handle = 0L
                h
            }
            if (active != 0L) nativeDestroy(active)
        }
    }

    private class SharedHost(
        @Volatile var handle: Long,
        var refCount: Int = 0,
        val handleLock: ReentrantReadWriteLock = ReentrantReadWriteLock(),
        val eventLock: Any = Any(),
        val eventQueues: MutableMap<String, ArrayDeque<JSONObject>> = linkedMapOf(),
        val listeners: MutableMap<String, (JSONObject) -> Unit> = linkedMapOf(),
    )

    private external fun nativeCreate(appDataDir: String): Long
    private external fun nativeCreateTest(appDataDir: String): Long
    private external fun nativeDispatch(handle: Long, requestJson: String): String
    private external fun nativeDestroy(handle: Long)

    private companion object {
        const val MAX_REPLAY_EVENTS = 256
        val registryLock = Any()
        val sharedHosts = mutableMapOf<String, SharedHost>()
    }
}
