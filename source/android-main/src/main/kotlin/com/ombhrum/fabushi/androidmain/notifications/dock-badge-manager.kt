package com.ombhrum.fabushi.androidmain.notifications

internal data class DockBadgeRosterAgent(
    val id: String,
    val hasUnread: Boolean,
    val isHiddenFromSidebar: Boolean = false,
    val unreadCount: Int? = null,
    val snapshotEpoch: String = "",
    val snapshotSeq: Long = 0L,
)

private data class TrackedDockAgent(
    val snapshot: DockBadgeAgent,
    val epoch: String,
    val seq: Long,
)

internal fun isStaleDockBadgeRow(
    trackedEpoch: String,
    trackedSeq: Long,
    incoming: DockBadgeRosterAgent,
): Boolean = trackedEpoch == incoming.snapshotEpoch && incoming.snapshotSeq < trackedSeq

internal class AndroidDockBadgeManager(
    private val setBadgeCount: (Int) -> Unit,
    private val reportFailure: (Throwable) -> Unit = {},
) {
    private var agents = linkedMapOf<String, TrackedDockAgent>()
    private var projectedTotal: Int? = null

    fun handleAgents(agents: List<DockBadgeRosterAgent>) {
        applyRoster(agents)
    }

    fun handleAgentUpserted(agent: DockBadgeRosterAgent) {
        val tracked = agents[agent.id]
        if (tracked != null && isStaleDockBadgeRow(tracked.epoch, tracked.seq, agent)) return
        agents[agent.id] = agent.toTracked()
        project()
    }

    fun seedRoster(agents: List<DockBadgeRosterAgent>) {
        applyRoster(agents)
    }

    fun forget(agentId: String) {
        if (agents.remove(agentId) != null) project()
    }

    fun reset() {
        agents.clear()
        project()
    }

    private fun applyRoster(incoming: List<DockBadgeRosterAgent>) {
        if (incoming.isEmpty() && agents.isNotEmpty()) return

        var stampEpoch = ""
        var stampSeq = 0L
        incoming.forEach { agent ->
            if (agent.snapshotSeq > stampSeq) {
                stampSeq = agent.snapshotSeq
                stampEpoch = agent.snapshotEpoch
            }
        }

        val next = linkedMapOf<String, TrackedDockAgent>()
        incoming.forEach { agent ->
            val tracked = agents[agent.id]
            next[agent.id] =
                if (tracked != null && isStaleDockBadgeRow(tracked.epoch, tracked.seq, agent)) {
                    tracked
                } else {
                    agent.toTracked()
                }
        }
        agents.forEach { (id, tracked) ->
            if (id !in next && tracked.epoch == stampEpoch && tracked.seq > stampSeq) {
                next[id] = tracked
            }
        }
        agents = next
        project()
    }

    private fun project() {
        val total = computeDockBadgeTotal(agents.values.map { it.snapshot })
        if (total == projectedTotal) return
        runCatching { setBadgeCount(total) }
            .onSuccess { projectedTotal = total }
            .onFailure(reportFailure)
    }

    private fun DockBadgeRosterAgent.toTracked(): TrackedDockAgent =
        TrackedDockAgent(
            snapshot = DockBadgeAgent(
                hasUnread = hasUnread,
                isHiddenFromSidebar = isHiddenFromSidebar,
                unreadCount = unreadCount,
            ),
            epoch = snapshotEpoch,
            seq = snapshotSeq,
        )
}
