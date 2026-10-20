package com.ombhrum.fabushi

internal enum class GlobalShortcutId {
    NEW_AGENT, COMMAND_PALETTE, OPEN_SETTINGS, OPEN_TOOLS, FOCUS_INPUT, FIND_IN_CHAT,
    FOCUS_SEARCH, PREVIOUS_AGENT, NEXT_AGENT, NAVIGATE_BACK, NAVIGATE_FORWARD,
    TOGGLE_SIDEBAR, FOCUS_AGENT,
}

internal data class GlobalShortcutKeyEvent(
    val key: String,
    val ctrl: Boolean = false,
    val meta: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
    val defaultPrevented: Boolean = false,
    val repeat: Boolean = false,
)

internal data class GlobalShortcutAction(
    val id: GlobalShortcutId,
    val label: String,
    val hotkeys: List<String>,
    val enabledInEditable: Boolean = true,
    val agentIndex: Int? = null,
    val run: () -> Unit,
)

internal data class RootOverlayEscapeState(
    val isArmed: Boolean = false,
    val isOverlayStacked: Boolean = false,
    val allowPreventedRootSearchEscape: Boolean = false,
    val close: () -> Unit = {},
)

internal data class GlobalShortcutHandlers(
    val newAgent: () -> Unit,
    val toggleCommandPalette: () -> Unit,
    val openSettings: () -> Unit,
    val openTools: () -> Unit,
    val focusPrompt: () -> Unit,
    val openSearch: () -> Unit,
    val findInChat: (() -> Unit)?,
    val previousAgent: () -> Unit,
    val nextAgent: () -> Unit,
    val navigateBack: () -> Unit,
    val navigateForward: () -> Unit,
    val focusAgent: (Int) -> Unit,
    val toggleSidebar: () -> Unit,
)

internal fun createRootShellShortcutActions(
    handlers: GlobalShortcutHandlers,
    agentCount: Int,
): List<GlobalShortcutAction> = buildList {
    add(GlobalShortcutAction(GlobalShortcutId.NEW_AGENT, "New Bot", listOf("mod+n"), run = handlers.newAgent))
    add(GlobalShortcutAction(GlobalShortcutId.COMMAND_PALETTE, "Jump to", listOf("mod+k"), run = handlers.toggleCommandPalette))
    add(GlobalShortcutAction(GlobalShortcutId.OPEN_SETTINGS, "Open settings", listOf("mod+comma"), run = handlers.openSettings))
    add(GlobalShortcutAction(GlobalShortcutId.OPEN_TOOLS, "Customize", listOf("mod+shift+m"), run = handlers.openTools))
    add(GlobalShortcutAction(GlobalShortcutId.FOCUS_INPUT, "Focus prompt", listOf("mod+i", "mod+l"), enabledInEditable = false, run = handlers.focusPrompt))
    handlers.findInChat?.let {
        add(GlobalShortcutAction(GlobalShortcutId.FIND_IN_CHAT, "Find in chat", listOf("mod+f"), run = it))
    }
    add(GlobalShortcutAction(GlobalShortcutId.FOCUS_SEARCH, "Search agents", listOf("mod+shift+f"), run = handlers.openSearch))
    add(GlobalShortcutAction(GlobalShortcutId.PREVIOUS_AGENT, "Previous agent", listOf("alt+up"), run = handlers.previousAgent))
    add(GlobalShortcutAction(GlobalShortcutId.NEXT_AGENT, "Next agent", listOf("alt+down"), run = handlers.nextAgent))
    add(GlobalShortcutAction(GlobalShortcutId.NAVIGATE_BACK, "Back", listOf("mod+bracketleft"), run = handlers.navigateBack))
    add(GlobalShortcutAction(GlobalShortcutId.NAVIGATE_FORWARD, "Forward", listOf("mod+bracketright"), run = handlers.navigateForward))
    for (index in 1..agentCount.coerceIn(0, 9)) {
        add(GlobalShortcutAction(GlobalShortcutId.FOCUS_AGENT, "Focus sidebar agent $index", listOf("mod+$index"), agentIndex = index, run = { handlers.focusAgent(index) }))
    }
    add(GlobalShortcutAction(GlobalShortcutId.TOGGLE_SIDEBAR, "Toggle compact sidebar", listOf("mod+b"), run = handlers.toggleSidebar))
}

internal interface GlobalKeyboardShortcutPresentationBinding {
    fun setActions(value: List<GlobalShortcutAction>)
    fun acceptOverlayState(value: RootOverlayEscapeState)
    fun setEditableFocused(value: Boolean)
    fun clear()
}

/** Activity-lifecycle input owner; canonical product state stays in existing owners. */
internal class GlobalKeyboardShortcutController {
    private var actions: List<GlobalShortcutAction> = emptyList()
    private var overlay = RootOverlayEscapeState()
    private var editableFocused = false
    private var bindingEpoch = 0L

    fun bindPresentation(): GlobalKeyboardShortcutPresentationBinding {
        val epoch = ++bindingEpoch
        resetPresentationState()
        return object : GlobalKeyboardShortcutPresentationBinding {
            override fun setActions(value: List<GlobalShortcutAction>) =
                writeIfCurrent(epoch) { actions = value }

            override fun acceptOverlayState(value: RootOverlayEscapeState) =
                writeIfCurrent(epoch) { overlay = value }

            override fun setEditableFocused(value: Boolean) =
                writeIfCurrent(epoch) { editableFocused = value }

            override fun clear() {
                clearPresentationBinding(epoch)
            }
        }
    }

    fun clearPresentationBinding() {
        bindingEpoch += 1
        resetPresentationState()
    }

    private fun clearPresentationBinding(epoch: Long) {
        if (epoch != bindingEpoch) return
        clearPresentationBinding()
    }

    private inline fun writeIfCurrent(epoch: Long, block: () -> Unit) {
        if (epoch == bindingEpoch) block()
    }

    private fun resetPresentationState() {
        actions = emptyList()
        overlay = RootOverlayEscapeState()
        editableFocused = false
    }

    fun dispatch(event: GlobalShortcutKeyEvent): Boolean {
        if (event.repeat) return false
        val normalized = event.copy(key = normalizeKey(event.key))
        if (normalized.key == "escape") {
            if (overlay.isArmed && !overlay.isOverlayStacked &&
                (!normalized.defaultPrevented || overlay.allowPreventedRootSearchEscape)
            ) {
                overlay.close()
                return true
            }
            return false
        }
        if (normalized.defaultPrevented) return false
        val selected = actions.firstOrNull { candidate ->
            (!editableFocused || candidate.enabledInEditable) &&
                candidate.hotkeys.any { chordMatches(normalized, it) }
        } ?: return false
        selected.run()
        return true
    }

    internal fun actionsForTest(): List<Pair<GlobalShortcutId, Int?>> = actions.map { it.id to it.agentIndex }
    internal fun bindingEpochForTest(): Long = bindingEpoch

    private fun chordMatches(event: GlobalShortcutKeyEvent, chord: String): Boolean {
        val parts = chord.split("+")
        val expectedKey = parts.lastOrNull() ?: return false
        if (event.key != expectedKey.lowercase()) return false
        val mod = event.meta || event.ctrl
        if (parts.contains("mod") != mod) return false
        if (parts.contains("alt") != event.alt) return false
        if (parts.contains("shift") != event.shift) return false
        return true
    }

    private fun normalizeKey(value: String): String = when (value.lowercase()) {
        "arrowup" -> "up"
        "arrowdown" -> "down"
        "[" -> "bracketleft"
        "]" -> "bracketright"
        "," -> "comma"
        else -> value.lowercase()
    }
}
