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
