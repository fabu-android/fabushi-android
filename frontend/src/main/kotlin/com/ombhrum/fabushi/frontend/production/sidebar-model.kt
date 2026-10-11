package com.ombhrum.fabushi

internal interface SidebarOrderAgent {
    val id: String
    val isPinned: Boolean
}

internal data class SidebarPartition<T : SidebarOrderAgent>(
    val pinned: List<T>,
    val unpinned: List<T>,
)

internal enum class PinnedMovePosition { BEFORE, AFTER }

internal fun <T : SidebarOrderAgent> partitionSidebarAgents(
    agents: List<T>,
    pinnedAgentIds: List<String>,
): SidebarPartition<T> {
    val pinnedById = agents.filter { it.isPinned }.associateBy { it.id }
    val pinned = mutableListOf<T>()
    val pinnedIds = linkedSetOf<String>()

    pinnedAgentIds.forEach { id ->
        val agent = pinnedById[id] ?: return@forEach
        if (pinnedIds.add(agent.id)) pinned += agent
    }
    agents.forEach { agent ->
        if (agent.isPinned && pinnedIds.add(agent.id)) pinned += agent
    }

    return SidebarPartition(
        pinned = pinned,
        unpinned = agents.filter { !it.isPinned && it.id !in pinnedIds },
    )
}

internal fun movePinnedAgent(
    storedIds: List<String>,
    movedId: String,
    targetId: String,
    position: PinnedMovePosition,
): List<String> {
    if (movedId == targetId) return storedIds.toList()
    val withoutMoved = storedIds.filterNot { it == movedId }
    val targetIndex = withoutMoved.indexOf(targetId)
    if (targetIndex < 0) return storedIds.toList()
    val insertionIndex = if (position == PinnedMovePosition.BEFORE) {
        targetIndex
    } else {
        targetIndex + 1
    }
    return buildList {
        addAll(withoutMoved.subList(0, insertionIndex))
        add(movedId)
        addAll(withoutMoved.subList(insertionIndex, withoutMoved.size))
    }
}
