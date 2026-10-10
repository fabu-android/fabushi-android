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
