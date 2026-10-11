package com.ombhrum.fabushi

internal fun committedAgentName(
    initialValue: String,
    draftValue: String,
): String? {
    val trimmed = draftValue.trim()
    return trimmed.takeIf { it.isNotEmpty() && it != initialValue }
}

internal data class CommittedAgentProfile(
    val name: String,
    val title: String?,
    val description: String,
    val avatarShape: String?,
    val avatarColor: String?,
)

internal fun committedAgentProfile(
    initialName: String,
    initialTitle: String? = null,
    initialDescription: String,
    draftName: String,
    draftTitle: String? = initialTitle,
    draftDescription: String,
    initialAvatarShape: String? = null,
    initialAvatarColor: String? = null,
    draftAvatarShape: String? = initialAvatarShape,
    draftAvatarColor: String? = initialAvatarColor,
): CommittedAgentProfile? {
    val name = draftName.replace(Regex("\\s+"), " ").trim().take(72)
    if (name.isEmpty()) return null
    val title = normalizeAgentTitle(draftTitle)
    val description = draftDescription.trim().take(240)
    val avatarShape = normalizeAgentAvatarShape(draftAvatarShape)
    val avatarColor = normalizeAgentAvatarColor(draftAvatarColor)
    if (
        name == initialName &&
        title == normalizeAgentTitle(initialTitle) &&
        description == initialDescription &&
        avatarShape == normalizeAgentAvatarShape(initialAvatarShape) &&
        avatarColor == normalizeAgentAvatarColor(initialAvatarColor)
    ) return null
    return CommittedAgentProfile(name, title, description, avatarShape, avatarColor)
}

internal fun normalizeAgentTitle(value: String?): String? =
    value?.trim()?.take(120)?.takeIf(String::isNotEmpty)

internal fun normalizeAgentAvatarShape(value: String?): String? =
    value?.trim()?.lowercase()?.takeIf { it in setOf("circle", "square", "squircle") }

internal fun normalizeAgentAvatarColor(value: String?): String? {
    val normalized = value?.trim()?.uppercase() ?: return null
    return normalized.takeIf {
        it.length == 7 && it.first() == '#' &&
            it.drop(1).all { character -> character.isDigit() || character in 'A'..'F' }
    }
}


internal data class AgentSettingsMutationToken(
    val agentId: String,
    val accountSlot: String?,
    val generation: Long,
)

internal class AgentSettingsMutationFence {
    private var selectedAgentId: String? = null
    private var selectedAccountSlot: String? = null
    private var generation: Long = 0L
    private var disposed: Boolean = false

    fun select(agentId: String, accountSlot: String?) {
        if (disposed) return
        if (selectedAgentId == agentId && selectedAccountSlot == accountSlot) return
        generation += 1
        selectedAgentId = agentId
        selectedAccountSlot = accountSlot
    }

    fun clear(agentId: String? = null) {
        if (disposed) return
        if (agentId != null && selectedAgentId != agentId) return
        if (selectedAgentId == null) return
        generation += 1
        selectedAgentId = null
    }

    fun accountChanged(accountSlot: String?) {
        if (disposed || selectedAccountSlot == accountSlot) return
        generation += 1
        selectedAgentId = null
        selectedAccountSlot = accountSlot
    }

    fun beginMutation(agentId: String, accountSlot: String?): AgentSettingsMutationToken? {
        if (
            disposed ||
            selectedAgentId != agentId ||
            selectedAccountSlot != accountSlot
        ) {
            return null
        }
        generation += 1
        return AgentSettingsMutationToken(agentId, accountSlot, generation)
    }

    fun isCurrent(token: AgentSettingsMutationToken): Boolean =
        !disposed &&
            token.agentId == selectedAgentId &&
            token.accountSlot == selectedAccountSlot &&
            token.generation == generation

    fun dispose() {
        if (disposed) return
        disposed = true
        generation += 1
        selectedAgentId = null
    }
}
