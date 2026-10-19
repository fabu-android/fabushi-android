package com.ombhrum.fabushi.androidmain.notifications

internal data class DockBadgeAgent(
    val hasUnread: Boolean = false,
    val isHiddenFromSidebar: Boolean = false,
    val unreadCount: Int? = null,
)

internal fun computeDockBadgeTotal(agents: Collection<DockBadgeAgent>): Int {
    var total = 0L
    for (agent in agents) {
        if (!agent.hasUnread || agent.isHiddenFromSidebar) continue
        total += maxOf(agent.unreadCount ?: 1, 1)
    }
    return total.coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
}
