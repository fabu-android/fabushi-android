package com.ombhrum.fabushi.androidmain.coordinator
class CoordinatorMainPortClient {
    private var nextId = 0L
    private val pending = linkedSetOf<String>()
    fun begin(): String = ("m-" + (++nextId)).also(pending::add)
    fun settle(requestId: String): Boolean = pending.remove(requestId)
    fun settleAll(): Set<String> = pending.toSet().also { pending.clear() }
    fun pendingCount(): Int = pending.size
}
