package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GlobalKeyboardShortcutsTest {
    private data class Fixture(
        val controller: GlobalKeyboardShortcutController,
        val binding: GlobalKeyboardShortcutPresentationBinding,
        val calls: MutableList<String>,
    )

    private fun fixture(agentCount: Int = 12, includeFind: Boolean = true): Fixture {
        val calls = mutableListOf<String>()
        val handlers = GlobalShortcutHandlers(
            newAgent = { calls += "new" }, toggleCommandPalette = { calls += "palette" },
            openSettings = { calls += "settings" }, openTools = { calls += "tools" },
            focusPrompt = { calls += "focus" }, openSearch = { calls += "search" },
            findInChat = if (includeFind) ({ calls += "find" }) else null,
            previousAgent = { calls += "previous" }, nextAgent = { calls += "next" },
            navigateBack = { calls += "back" }, navigateForward = { calls += "forward" },
            focusAgent = { calls += "agent:$it" }, toggleSidebar = { calls += "sidebar" },
        )
        val controller = GlobalKeyboardShortcutController()
        val binding = controller.bindPresentation()
        binding.setActions(createRootShellShortcutActions(handlers, agentCount))
        return Fixture(controller, binding, calls)
    }

    @Test fun orderedRegistryCoversDesktopChordsAndBoundsFocusAgents() {
        val f = fixture()
        val cases = listOf(
            GlobalShortcutKeyEvent("n", ctrl = true) to "new",
            GlobalShortcutKeyEvent("k", meta = true) to "palette",
            GlobalShortcutKeyEvent(",", ctrl = true) to "settings",
            GlobalShortcutKeyEvent("m", ctrl = true, shift = true) to "tools",
            GlobalShortcutKeyEvent("f", ctrl = true, shift = true) to "search",
            GlobalShortcutKeyEvent("ArrowUp", alt = true) to "previous",
            GlobalShortcutKeyEvent("ArrowDown", alt = true) to "next",
            GlobalShortcutKeyEvent("[", ctrl = true) to "back",
            GlobalShortcutKeyEvent("]", ctrl = true) to "forward",
            GlobalShortcutKeyEvent("b", ctrl = true) to "sidebar",
            GlobalShortcutKeyEvent("9", ctrl = true) to "agent:9",
        )
        cases.forEach { (event, expected) ->
            assertTrue(f.controller.dispatch(event))
            assertEquals(expected, f.calls.last())
        }
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("0", ctrl = true)))
        assertEquals(9, f.controller.actionsForTest().count { it.first == GlobalShortcutId.FOCUS_AGENT })
    }

    @Test fun editableDefaultPreventedRepeatAndOptionalFindAreFenced() {
        val f = fixture(agentCount = 2)
        f.binding.setEditableFocused(true)
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("i", ctrl = true)))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true)))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true, defaultPrevented = true)))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true, repeat = true)))
        f.binding.setEditableFocused(false)
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("l", meta = true)))
        assertEquals("focus", f.calls.last())
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("f", ctrl = true)))
        assertEquals("find", f.calls.last())
        assertFalse(fixture(agentCount = 1, includeFind = false).controller.dispatch(GlobalShortcutKeyEvent("f", ctrl = true)))
    }

    @Test fun escapeOnlyClosesRootOverlayWhenUnstackedAndAllowed() {
        val f = fixture()
        f.binding.acceptOverlayState(RootOverlayEscapeState(isArmed = true, close = { f.calls += "close" }))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("escape")))
        f.binding.acceptOverlayState(RootOverlayEscapeState(isArmed = true, isOverlayStacked = true, close = { f.calls += "bad" }))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("escape")))
        assertFalse(f.calls.contains("bad"))
        f.binding.acceptOverlayState(RootOverlayEscapeState(isArmed = true, close = { f.calls += "prevented" }))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("escape", defaultPrevented = true)))
        f.binding.acceptOverlayState(RootOverlayEscapeState(isArmed = true, allowPreventedRootSearchEscape = true, close = { f.calls += "root-search" }))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("escape", defaultPrevented = true)))
        assertEquals("root-search", f.calls.last())
    }

    @Test fun presentationBindingClearsAcrossActivityRecreation() {
        val controller = GlobalKeyboardShortcutController()
        val calls = mutableListOf<String>()
        val staleBinding = controller.bindPresentation()
        staleBinding.setActions(
            listOf(
                GlobalShortcutAction(
                    GlobalShortcutId.COMMAND_PALETTE,
                    "old",
                    listOf("mod+k"),
                    run = { calls += "old" },
                ),
            ),
        )
        val before = controller.bindingEpochForTest()
        controller.clearPresentationBinding()
        assertEquals(before + 1, controller.bindingEpochForTest())
        assertTrue(controller.actionsForTest().isEmpty())
        assertFalse(controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true)))

        val currentBinding = controller.bindPresentation()
        currentBinding.setActions(
            listOf(
                GlobalShortcutAction(
                    GlobalShortcutId.COMMAND_PALETTE,
                    "fresh",
                    listOf("mod+k"),
                    run = { calls += "fresh" },
                ),
            ),
        )
        staleBinding.setActions(
            listOf(
                GlobalShortcutAction(
                    GlobalShortcutId.COMMAND_PALETTE,
                    "stale-write",
                    listOf("mod+k"),
                    run = { calls += "stale-write" },
                ),
            ),
        )
        staleBinding.clear()

        assertTrue(controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true)))
        assertEquals(listOf("fresh"), calls)
    }
}
