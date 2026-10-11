package com.ombhrum.fabushi.androidpreload
sealed interface EdgeReply {
    data class Success(val value: Any?) : EdgeReply
    data class Failure(val code: String, val message: String) : EdgeReply
}
class RpcEdgeRuntime {
    private val handlers = linkedMapOf<String, (Any?) -> Any?>()
    fun register(method: String, handler: (Any?) -> Any?) {
        require(method.isNotBlank()) { "RPC method must not be blank" }
        check(method !in handlers) { "RPC method already registered: " + method }
        handlers[method] = handler
    }
    fun call(method: String, payload: Any?): EdgeReply {
        val handler = handlers[method] ?: return EdgeReply.Failure("unknown-method", "No Android edge method named " + method)
        return runCatching { EdgeReply.Success(handler(payload)) }
            .getOrElse { EdgeReply.Failure("handler-failed", it.message ?: it::class.java.simpleName) }
    }
}
