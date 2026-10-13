package com.ombhrum.fabushi

internal enum class AgentChannelAvailability {
    AVAILABLE,
    COMING_SOON,
}

internal data class AgentChannelManifest(
    val platform: String,
    val displayName: String,
    val blurb: String,
    val credentialLabel: String,
    val availability: AgentChannelAvailability,
    val connectGuide: String,
)

data class AgentChannelConnection(
    val platform: String,
    val label: String,
    val status: String,
    val detail: String? = null,
)

internal enum class AgentChannelRowKind {
    COMING_SOON,
    AVAILABLE,
    CONNECTED,
    ERROR,
    CONNECTING,
}

internal data class AgentChannelRowProjection(
    val manifest: AgentChannelManifest,
    val connection: AgentChannelConnection?,
    val kind: AgentChannelRowKind,
    val statusLabel: String?,
    val detail: String,
)

internal val desktopMainAgentChannelManifests: List<AgentChannelManifest> = listOf(
    AgentChannelManifest(
        platform = "discord",
        displayName = "Discord",
        blurb = "Message in Discord servers and DMs (coming soon).",
        credentialLabel = "bot token",
        availability = AgentChannelAvailability.COMING_SOON,
        connectGuide = "",
    ),
    AgentChannelManifest(
        platform = "slack",
        displayName = "Slack",
        blurb = "Message in Slack channels and DMs (coming soon).",
        credentialLabel = "app token",
        availability = AgentChannelAvailability.COMING_SOON,
        connectGuide = "",
    ),
)

internal fun projectAgentChannelRow(
    manifest: AgentChannelManifest,
    connection: AgentChannelConnection?,
): AgentChannelRowProjection {
    val kind = when {
        connection?.status == "connected" -> AgentChannelRowKind.CONNECTED
        connection?.status == "error" -> AgentChannelRowKind.ERROR
        connection != null -> AgentChannelRowKind.CONNECTING
        manifest.availability == AgentChannelAvailability.COMING_SOON -> AgentChannelRowKind.COMING_SOON
        else -> AgentChannelRowKind.AVAILABLE
    }
    val statusLabel = when (kind) {
        AgentChannelRowKind.COMING_SOON -> "Soon"
        AgentChannelRowKind.CONNECTED -> "Connected"
        AgentChannelRowKind.ERROR -> "Needs attention"
        AgentChannelRowKind.CONNECTING -> "Connecting"
        AgentChannelRowKind.AVAILABLE -> null
    }
    val detail = when (kind) {
        AgentChannelRowKind.CONNECTED -> "Connected as ${connection?.label.orEmpty()}"
        AgentChannelRowKind.ERROR -> connection?.detail ?: "The platform rejected this connection."
        else -> manifest.blurb
    }
    return AgentChannelRowProjection(
        manifest = manifest,
        connection = connection,
        kind = kind,
        statusLabel = statusLabel,
        detail = detail,
    )
}
