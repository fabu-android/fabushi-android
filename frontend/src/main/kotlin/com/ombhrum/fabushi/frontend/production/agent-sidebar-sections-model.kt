package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection

internal const val AGENT_UNASSIGNED_SECTION_ID = "__agents__"

internal fun currentAgentSidebarSectionId(
    sections: List<AndroidSidebarSection>,
    agentId: String,
): String? {
    if (sections.isEmpty()) return null
    return sections.firstOrNull { section -> agentId in section.agentIds }?.id
        ?: AGENT_UNASSIGNED_SECTION_ID
}

internal fun moveAgentToSidebarSection(
    sections: List<AndroidSidebarSection>,
    agentId: String,
    targetSectionId: String,
): List<AndroidSidebarSection>? {
    if (agentId.isBlank() || sections.isEmpty()) return null
    if (
        targetSectionId != AGENT_UNASSIGNED_SECTION_ID &&
        sections.none { it.id == targetSectionId && it.id != AGENT_UNASSIGNED_SECTION_ID }
    ) return null
    if (currentAgentSidebarSectionId(sections, agentId) == targetSectionId) return null

    val editable = sections
        .filterNot { it.id == AGENT_UNASSIGNED_SECTION_ID }
        .map { section ->
            val withoutAgent = section.agentIds.filterNot { it == agentId }
            if (section.id == targetSectionId) {
                section.copy(agentIds = withoutAgent + agentId)
            } else {
                section.copy(agentIds = withoutAgent)
            }
        }
    return editable
}

internal fun createSidebarSectionForAgent(
    sections: List<AndroidSidebarSection>,
    agentId: String,
    sectionId: String,
    name: String = "New section",
): List<AndroidSidebarSection>? {
    val cleanId = sectionId.trim()
    if (
        agentId.isBlank() ||
        cleanId.isBlank() ||
        cleanId == AGENT_UNASSIGNED_SECTION_ID ||
        sections.any { it.id == cleanId }
    ) return null
    val editable = sections
        .filterNot { it.id == AGENT_UNASSIGNED_SECTION_ID }
        .map { section -> section.copy(agentIds = section.agentIds.filterNot { it == agentId }) }
    return listOf(
        AndroidSidebarSection(
            id = cleanId,
            name = name,
            agentIds = listOf(agentId),
        ),
    ) + editable
}


internal enum class AgentSidebarSectionMovePosition { BEFORE, AFTER }

internal fun renameAgentSidebarSection(
    sections: List<AndroidSidebarSection>,
    sectionId: String,
    name: String,
): List<AndroidSidebarSection>? {
    if (sectionId == AGENT_UNASSIGNED_SECTION_ID) return null
    val cleanName = name.replace(Regex("\\s+"), " ").trim().take(120)
    if (cleanName.isBlank()) return null
    val current = sections.firstOrNull { it.id == sectionId } ?: return null
    if (current.name == cleanName) return null
    return sections
        .filterNot { it.id == AGENT_UNASSIGNED_SECTION_ID }
        .map { section -> if (section.id == sectionId) section.copy(name = cleanName) else section }
}

internal fun deleteAgentSidebarSection(
    sections: List<AndroidSidebarSection>,
    sectionId: String,
): List<AndroidSidebarSection>? {
    if (sectionId == AGENT_UNASSIGNED_SECTION_ID) return null
    if (sections.none { it.id == sectionId }) return null
    return sections.filterNot {
        it.id == sectionId || it.id == AGENT_UNASSIGNED_SECTION_ID
    }
}

internal fun moveAgentSidebarSection(
    sections: List<AndroidSidebarSection>,
    sectionId: String,
    targetId: String,
    position: AgentSidebarSectionMovePosition,
): List<AndroidSidebarSection>? {
    if (
        sectionId == AGENT_UNASSIGNED_SECTION_ID ||
        targetId == AGENT_UNASSIGNED_SECTION_ID ||
        sectionId == targetId
    ) return null
    val editable = sections.filterNot { it.id == AGENT_UNASSIGNED_SECTION_ID }
    val moved = editable.firstOrNull { it.id == sectionId } ?: return null
    val targetIndexBeforeRemoval = editable.indexOfFirst { it.id == targetId }
    if (targetIndexBeforeRemoval < 0) return null
    val remaining = editable.filterNot { it.id == sectionId }
    val targetIndex = remaining.indexOfFirst { it.id == targetId }
    if (targetIndex < 0) return null
    val insertAt = when (position) {
        AgentSidebarSectionMovePosition.BEFORE -> targetIndex
        AgentSidebarSectionMovePosition.AFTER -> targetIndex + 1
    }
    return buildList {
        addAll(remaining.subList(0, insertAt))
        add(moved)
        addAll(remaining.subList(insertAt, remaining.size))
    }.takeIf { next -> next.map { it.id } != editable.map { it.id } }
}

internal data class AgentSidebarSectionDeleteCopy(
    val title: String,
    val description: String,
    val confirmLabel: String = "Delete",
    val cancelLabel: String = "Cancel",
)

internal fun agentSidebarSectionDeleteCopy(name: String): AgentSidebarSectionDeleteCopy =
    AgentSidebarSectionDeleteCopy(
        title = "Delete “${name.ifBlank { "Section" }}”",
        description = "Its agents move to Unassigned. No agents are deleted.",
    )
