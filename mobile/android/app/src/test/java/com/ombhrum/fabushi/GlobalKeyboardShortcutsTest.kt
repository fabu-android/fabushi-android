package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GlobalKeyboardShortcutsTest {
    private data class Fixture(val controller: GlobalKeyboardShortcutController, val calls: MutableList<String>)

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
        return Fixture(GlobalKeyboardShortcutController().also {
            it.setActions(createRootShellShortcutActions(handlers, agentCount))
        }, calls)
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
        f.controller.setEditableFocused(true)
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("i", ctrl = true)))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true)))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true, defaultPrevented = true)))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true, repeat = true)))
        f.controller.setEditableFocused(false)
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("l", meta = true)))
        assertEquals("focus", f.calls.last())
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("f", ctrl = true)))
        assertEquals("find", f.calls.last())
        assertFalse(fixture(agentCount = 1, includeFind = false).controller.dispatch(GlobalShortcutKeyEvent("f", ctrl = true)))
    }

    @Test fun escapeOnlyClosesRootOverlayWhenUnstackedAndAllowed() {
        val f = fixture()
        f.controller.acceptOverlayState(RootOverlayEscapeState(isArmed = true, close = { f.calls += "close" }))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("escape")))
        f.controller.acceptOverlayState(RootOverlayEscapeState(isArmed = true, isOverlayStacked = true, close = { f.calls += "bad" }))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("escape")))
        assertFalse(f.calls.contains("bad"))
        f.controller.acceptOverlayState(RootOverlayEscapeState(isArmed = true, close = { f.calls += "prevented" }))
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("escape", defaultPrevented = true)))
        f.controller.acceptOverlayState(RootOverlayEscapeState(isArmed = true, allowPreventedRootSearchEscape = true, close = { f.calls += "root-search" }))
        assertTrue(f.controller.dispatch(GlobalShortcutKeyEvent("escape", defaultPrevented = true)))
        assertEquals("root-search", f.calls.last())
    }

    @Test fun presentationBindingClearsAcrossActivityRecreation() {
        val f = fixture()
        val before = f.controller.bindingEpochForTest()
        f.controller.clearPresentationBinding()
        assertEquals(before + 1, f.controller.bindingEpochForTest())
        assertTrue(f.controller.actionsForTest().isEmpty())
        assertFalse(f.controller.dispatch(GlobalShortcutKeyEvent("k", ctrl = true)))
        assertTrue(GlobalKeyboardShortcutController().actionsForTest().isEmpty())
    }
}
