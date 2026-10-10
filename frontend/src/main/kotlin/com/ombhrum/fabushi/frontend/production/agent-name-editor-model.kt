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
