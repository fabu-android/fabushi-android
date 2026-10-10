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
    val description: String,
)

internal fun committedAgentProfile(
    initialName: String,
    initialDescription: String,
    draftName: String,
    draftDescription: String,
): CommittedAgentProfile? {
    val name = draftName.replace(Regex("\\s+"), " ").trim().take(72)
    if (name.isEmpty()) return null
    val description = draftDescription.trim().take(240)
    if (name == initialName && description == initialDescription) return null
    return CommittedAgentProfile(name = name, description = description)
}
