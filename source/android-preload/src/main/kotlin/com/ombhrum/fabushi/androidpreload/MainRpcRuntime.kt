package com.ombhrum.fabushi.androidpreload
class MainRpcRuntime(private val edge: RpcEdgeRuntime) {
    fun call(method: String, payload: Any?): EdgeReply = edge.call(method, payload)
}
