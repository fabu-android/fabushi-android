package com.ombhrum.fabushi.androidpreload

const val EDGE_UNKNOWN_METHOD = "edge/unknown-method"
const val EDGE_HANDLER_FAILED = "edge/handler-failed"

sealed interface EdgeReply {
    data class Success(val value: Any?) : EdgeReply
    data class Failure(val code: String, val message: String) : EdgeReply
}

/**
 * Android replacement for Desktop's preload RPC edge contract.
 *
 * This adapter owns no canonical product state. The production path remains the typed
 * AndroidCoordinatorPort -> android-main -> JNI -> Rust Host boundary.
 */
class RpcEdgeRuntime {
    private val handlers = linkedMapOf<String, (Any?) -> Any?>()
    private val eventListeners = linkedMapOf<String, LinkedHashSet<(Any?) -> Unit>>()

    @Synchronized
    fun register(method: String, handler: (Any?) -> Any?) {
        require(method.isNotBlank()) { "RPC method must not be blank" }
        check(method !in handlers) { "RPC method already registered: $method" }
        handlers[method] = handler
    }

    fun call(method: String, payload: Any?): EdgeReply {
        val handler = synchronized(this) { handlers[method] }
            ?: return EdgeReply.Failure(EDGE_UNKNOWN_METHOD, "No Android edge method named $method")
        return runCatching { EdgeReply.Success(handler(payload)) }
            .getOrElse { EdgeReply.Failure(EDGE_HANDLER_FAILED, it.message ?: it::class.java.simpleName) }
    }

    @Synchronized
    fun subscribe(handlers: Map<String, (Any?) -> Unit>): AutoCloseable {
        val installed = handlers
            .filterKeys(String::isNotBlank)
            .map { (event, listener) ->
                eventListeners.getOrPut(event) { linkedSetOf() }.add(listener)
                event to listener
            }
        return AutoCloseable {
            synchronized(this) {
                for ((event, listener) in installed) {
                    eventListeners[event]?.let { listeners ->
                        listeners.remove(listener)
                        if (listeners.isEmpty()) eventListeners.remove(event)
                    }
                }
            }
        }
    }

    fun emit(event: String, payload: Any?) {
        val listeners = synchronized(this) { eventListeners[event]?.toList().orEmpty() }
        for (listener in listeners) listener(payload)
    }

    @Synchronized
    internal fun listenerCount(event: String): Int = eventListeners[event]?.size ?: 0
}
