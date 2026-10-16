package com.ombhrum.fabushi

internal fun commandPaletteUpdateCommand(
    state: AndroidUpdateUiState,
    check: () -> Unit,
    install: () -> Unit,
    openUpdates: () -> Unit,
): CommandPaletteEntry? {
    if (state.phase == AndroidUpdatePhase.DISABLED) return null

    val label: String
    val activate: () -> Unit
    when (state.phase) {
        AndroidUpdatePhase.CHECKING -> {
            label = "Checking for Updates…"
            activate = openUpdates
        }
        AndroidUpdatePhase.AVAILABLE -> {
            label = "Download Update…"
            activate = install
        }
        AndroidUpdatePhase.DOWNLOADING -> {
            label = "Downloading Update…"
            activate = openUpdates
        }
        AndroidUpdatePhase.WAITING_FOR_PERMISSION -> {
            label = "Continue Update…"
            activate = install
        }
        AndroidUpdatePhase.INSTALLING -> {
            label = "Installing Update…"
            activate = openUpdates
        }
        AndroidUpdatePhase.OUTCOME_UNKNOWN -> {
            label = "Verify and Continue Update…"
            // OUTCOME_UNKNOWN means the prior installer side effect cannot be proven.
            // Only an explicit user activation may enter the updater's re-verification path;
            // never convert this state into an automatic check/replay.
            activate = install
        }
        AndroidUpdatePhase.UP_TO_DATE,
        AndroidUpdatePhase.ERROR -> {
            label = "Check for Updates"
            activate = {
                check()
                openUpdates()
            }
        }
        AndroidUpdatePhase.DISABLED -> return null
    }

    return CommandPaletteEntry(
        id = "update:app",
        kind = CommandPaletteEntryKind.COMMAND,
        label = label,
        detail = state.availableVersion?.let { "Updates · $it" } ?: "Updates",
        searchText = "app check version upgrade install latest release download update $label",
        activate = activate,
    )
}
