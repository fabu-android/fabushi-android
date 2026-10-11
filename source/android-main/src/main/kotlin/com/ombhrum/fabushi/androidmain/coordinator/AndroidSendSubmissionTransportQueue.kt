package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementDispatch
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase

internal data class AndroidSendSubmissionScope(
    val accountSlot: String?,
    val agentId: String,
    val nonce: String,
    val generation: Long,
) {
    init {
        require(agentId.isNotBlank()) { "Queued send requires an Agent id" }
        require(nonce.isNotBlank()) { "Queued send requires a nonce" }
        require(generation > 0L) { "Queued send requires a positive generation" }
    }

    val key: String
        get() = listOf(accountSlot.orEmpty(), agentId, nonce, generation.toString()).joinToString("\u0000")
}

internal class AndroidSendSubmissionTransportQueue(
    private val isTransportDown: () -> Boolean,
    private val queue: (AndroidSendSubmissionScope) -> AndroidSendAcknowledgement,
    private val dispatch: (AndroidSendSubmissionScope) -> AndroidSendAcknowledgementDispatch,
    private val cancelQueuedPersisted: (AndroidSendSubmissionScope) -> Boolean,
    private val onFlushed: (AndroidSendSubmissionScope, AndroidSendAcknowledgementDispatch) -> Unit,
) {
    private val lock = Any()
    private val queued = linkedMapOf<String, AndroidSendSubmissionScope>()
    private val activeAgents = mutableSetOf<String>()
    private var lifecycleGeneration = 1L
    private var disposed = false

    fun submit(scope: AndroidSendSubmissionScope): AndroidSendAcknowledgementDispatch {
        val shouldQueue = synchronized(lock) {
            check(!disposed) { "Send submission queue is disposed" }
            isTransportDown() || scope.agentId in activeAgents
        }
        if (shouldQueue) return queueScope(scope)

        synchronized(lock) {
            check(!disposed) { "Send submission queue is disposed" }
            activeAgents += scope.agentId
        }
        return try {
            dispatch(scope)
        } finally {
            synchronized(lock) { activeAgents -= scope.agentId }
            flushAgent(scope.agentId)
        }
    }

    fun discover(records: List<AndroidSendAcknowledgement>) {
        synchronized(lock) {
            if (disposed) return
            records
                .filter { it.phase == AndroidSendAcknowledgementPhase.QUEUED }
                .forEach { record ->
                    val scope = AndroidSendSubmissionScope(
                        accountSlot = record.accountSlot,
                        agentId = record.agentId,
                        nonce = record.nonce,
                        generation = record.generation,
                    )
                    queued.putIfAbsent(scope.key, scope)
                }
        }
    }

    fun flush() {
        if (isTransportDown()) return
        val agents = synchronized(lock) {
            if (disposed) return
            queued.values.map(AndroidSendSubmissionScope::agentId).distinct()
        }
        agents.forEach(::flushAgent)
    }

    fun cancelQueued(scope: AndroidSendSubmissionScope): Boolean {
        val known = synchronized(lock) {
            check(!disposed) { "Send submission queue is disposed" }
            queued[scope.key]
        } ?: scope
        val cancelled = cancelQueuedPersisted(known)
        if (cancelled) synchronized(lock) { queued.remove(known.key) }
        return cancelled
    }

    fun dispose() {
        synchronized(lock) {
            lifecycleGeneration += 1L
            disposed = true
            queued.clear()
            activeAgents.clear()
        }
    }

    private fun queueScope(scope: AndroidSendSubmissionScope): AndroidSendAcknowledgementDispatch {
        val acknowledgement = queue(scope)
        synchronized(lock) {
            if (!disposed) queued[scope.key] = scope
        }
        return AndroidSendAcknowledgementDispatch(
            acknowledgement = acknowledgement,
            operationId = null,
        )
    }

    private fun flushAgent(agentId: String) {
        while (!isTransportDown()) {
            val token: Long
            val scope = synchronized(lock) {
                if (disposed || agentId in activeAgents) return
                val next = queued.values.firstOrNull { it.agentId == agentId } ?: return
                activeAgents += agentId
                token = lifecycleGeneration
                next
            }
            val result = runCatching { dispatch(scope) }
            synchronized(lock) {
                activeAgents -= agentId
                if (!disposed && token == lifecycleGeneration) queued.remove(scope.key)
            }
            result.onSuccess { dispatched ->
                val current = synchronized(lock) { !disposed && token == lifecycleGeneration }
                if (current) onFlushed(scope, dispatched)
            }
            if (result.isFailure) return
        }
    }
}
