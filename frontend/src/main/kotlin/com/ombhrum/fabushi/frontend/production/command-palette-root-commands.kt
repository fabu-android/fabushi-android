package com.ombhrum.fabushi

internal enum class CommandPaletteInfoSection {
    MEMBERS,
    CHANNELS,
    SETTINGS,
}

internal enum class CommandPaletteComputerUpdateAction {
    READY,
    BUSY_OVERRIDE,
}


internal data class CommandPaletteComputerUpdateSnapshot(
    val imageUpdateAvailable: Boolean?,
    val pending: Boolean,
    val outcomeUnknown: Boolean,
    val rebuildKind: String?,
)

/**
 * Android exposes the computer update action only when the durable Coordinator snapshot proves
 * that an image update exists and no rebuild episode is active. Desktop also offers a durable
 * "update when idle" path while agents are working. Android does not yet have that queue owner, so
 * the working-agent case deliberately fails closed instead of creating a process-local queue that
 * could be lost and replay a side effect after process death.
 */
internal fun commandPaletteComputerUpdateAction(
    activeAgentIsGroup: Boolean?,
    workingAgentCount: Int,
    snapshot: CommandPaletteComputerUpdateSnapshot?,
): CommandPaletteComputerUpdateAction? {
    if (activeAgentIsGroup == null || activeAgentIsGroup) return null
    if (snapshot == null || snapshot.imageUpdateAvailable != true) return null
    if (snapshot.pending || snapshot.outcomeUnknown || snapshot.rebuildKind != null) return null
    if (workingAgentCount > 0) return null
    return CommandPaletteComputerUpdateAction.READY
}

internal fun commandPaletteRootCommands(
    activeAgentIsGroup: Boolean?,
    activeAgentIsSharedRoom: Boolean,
    hasChannels: Boolean,
    openInfoSection: (CommandPaletteInfoSection) -> Unit,
    computerUpdateAction: CommandPaletteComputerUpdateAction? = null,
    openComputerUpdateConfirm: (CommandPaletteComputerUpdateAction) -> Unit = {},
): List<CommandPaletteEntry> {
    if (activeAgentIsGroup == null) return emptyList()

    return buildList {
        if (activeAgentIsGroup && !activeAgentIsSharedRoom) {
            add(
                CommandPaletteEntry(
                    id = "info:members",
                    kind = CommandPaletteEntryKind.COMMAND,
                    label = "Members",
                    detail = "Current chat",
                    searchText = "Members people group participants Current chat",
                    activate = { openInfoSection(CommandPaletteInfoSection.MEMBERS) },
                ),
            )
        }
        if (hasChannels) {
            add(
                CommandPaletteEntry(
                    id = "info:channels",
                    kind = CommandPaletteEntryKind.COMMAND,
                    label = "Channels",
                    detail = "Current chat",
                    searchText = "Channels messaging platforms connect Current chat",
                    activate = { openInfoSection(CommandPaletteInfoSection.CHANNELS) },
                ),
            )
        }
        add(
            CommandPaletteEntry(
                id = "info:settings",
                kind = CommandPaletteEntryKind.COMMAND,
                label = "Chat Settings",
                detail = "Current chat",
                searchText = "Chat Settings details notifications Current chat",
                activate = { openInfoSection(CommandPaletteInfoSection.SETTINGS) },
            ),
        )
        computerUpdateAction?.let { action ->
            add(
                CommandPaletteEntry(
                    id = "update:computer",
                    kind = CommandPaletteEntryKind.COMMAND,
                    label = "Update Fabushi's Computer",
                    detail = "Updates",
                    searchText = "Update Fabushi Computer box image machine recreate latest shared Updates",
                    activate = { openComputerUpdateConfirm(action) },
                ),
            )
        }
    }
}
