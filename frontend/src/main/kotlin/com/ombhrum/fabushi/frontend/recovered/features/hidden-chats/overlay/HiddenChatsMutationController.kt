package com.ombhrum.fabushi

internal data class HiddenChatsMutationAgent(
    val id: String,
    val isHidden: Boolean,
)

internal enum class HiddenChatsMutationOutcome {
    CONFIRMED,
    DETERMINISTIC_REJECTION,
    TRANSPORT_UNKNOWN,
}

internal data class HiddenChatsMutationToken(
    val agentId: String,
    val generation: Long,
)

private data class HiddenChatsMutation(
    val id: String,
    val value: Boolean,
    val previousValue: Boolean,
    val generation: Long,
    var inFlight: Boolean = true,
    var confirmed: Boolean? = null,
    var held: Boolean = false,
)

/**
 * Transient presentation fence for Desktop-equivalent hidden-chat mutation behavior.
 *
 * This is deliberately not a durable owner. Android's AgentRosterMutationOwner and Rust roster
 * own request identity, process-death recovery and canonical hidden state. This controller only
 * keeps optimistic UI stable until canonical roster reconciliation settles the same value.
 */
internal class HiddenChatsMutationController(
    private val readAgent: (String) -> HiddenChatsMutationAgent?,
    private val onOptimisticChange: (String, Boolean) -> Unit,
    private val onRollback: (String, Boolean, Boolean) -> Unit,
    private val onPendingChange: (String, Boolean) -> Unit,
) {
    private val mutations = linkedMapOf<String, HiddenChatsMutation>()
    private var accountSlot: String? = null
    private var activeAgentId: String? = null
    private var generation = 0L
    private var disposed = false

    fun setScope(nextAccountSlot: String?, nextActiveAgentId: String?) {
        if (
            disposed ||
            (accountSlot == nextAccountSlot && activeAgentId == nextActiveAgentId)
        ) return
        generation += 1
        clearTransient()
        accountSlot = nextAccountSlot
        activeAgentId = nextActiveAgentId
    }

    fun begin(agentId: String, isHidden: Boolean): HiddenChatsMutationToken? {
        if (
            disposed ||
            accountSlot == null ||
            agentId.isBlank() ||
            mutations[agentId]?.let { it.inFlight || it.held } == true
        ) return null
        val agent = readAgent(agentId) ?: return null
        if (agent.isHidden == isHidden) return null
        val mutation = HiddenChatsMutation(
            id = agentId,
            value = isHidden,
            previousValue = agent.isHidden,
            generation = generation,
        )
        mutations[agentId] = mutation
        onOptimisticChange(agentId, isHidden)
        onPendingChange(agentId, true)
        return HiddenChatsMutationToken(agentId, generation)
    }

    fun settle(token: HiddenChatsMutationToken, outcome: HiddenChatsMutationOutcome) {
        if (!current(token.generation)) return
        val mutation = mutations[token.agentId] ?: return
        if (mutation.generation != token.generation) return
        when (outcome) {
            HiddenChatsMutationOutcome.CONFIRMED -> {
                mutation.inFlight = false
                mutation.confirmed = mutation.value
                mutation.held = false
                onPendingChange(mutation.id, false)
            }
            HiddenChatsMutationOutcome.TRANSPORT_UNKNOWN -> {
                // Durable operation identity remains below Presentation. Keep the optimistic
                // projection held until a canonical roster refresh proves the final value.
                mutation.inFlight = false
                mutation.confirmed = mutation.value
                mutation.held = true
                onPendingChange(mutation.id, true)
            }
            HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION -> {
                mutations.remove(mutation.id)
                onPendingChange(mutation.id, false)
                onRollback(mutation.id, mutation.value, mutation.previousValue)
            }
        }
    }

    fun ingestAgents(agents: List<HiddenChatsMutationAgent>) {
        if (disposed) return
        val byId = agents.associateBy(HiddenChatsMutationAgent::id)
        mutations.values.toList().forEach { mutation ->
            val confirmed = mutation.confirmed ?: return@forEach
            if (byId[mutation.id]?.isHidden == confirmed) {
                mutations.remove(mutation.id)
                onPendingChange(mutation.id, false)
            }
        }
    }

    fun projectHidden(agentId: String, canonicalValue: Boolean): Boolean =
        if (disposed) canonicalValue else mutations[agentId]?.value ?: canonicalValue

    fun isPending(agentId: String): Boolean {
        val mutation = mutations[agentId] ?: return false
        return !disposed && (mutation.inFlight || mutation.held)
    }

    fun reset() {
        generation += 1
        clearTransient()
        accountSlot = null
        activeAgentId = null
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        generation += 1
        clearTransient()
        accountSlot = null
        activeAgentId = null
    }

    private fun current(expectedGeneration: Long): Boolean =
        !disposed && generation == expectedGeneration

    private fun clearTransient() {
        mutations.keys.toList().forEach { onPendingChange(it, false) }
        mutations.clear()
    }
}

internal fun hiddenChatsMutationFailureOutcome(error: Throwable): HiddenChatsMutationOutcome {
    val message = error.message.orEmpty().lowercase()
    val deterministic =
        "rejected" in message ||
            "not found" in message ||
            "invalid" in message ||
            "unsupported" in message ||
            "required" in message
    return if (deterministic) {
        HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION
    } else {
        HiddenChatsMutationOutcome.TRANSPORT_UNKNOWN
    }
}
