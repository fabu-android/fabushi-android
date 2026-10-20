import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROLLER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/GlobalKeyboardShortcuts.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
ACTIVITY = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt"
BOOTSTRAP = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/bootstrap_view.kt"
HOME = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokHomeSurface.kt"
CHAT = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokBotComponents.kt"
TEST = ROOT / "mobile/android/app/src/test/java/com/ombhrum/fabushi/GlobalKeyboardShortcutsTest.kt"

class GlobalKeyboardShortcutWiringTest(unittest.TestCase):
    def test_activity_uses_framework_child_first_key_dispatch_without_restricted_api(self):
        s = ACTIVITY.read_text(encoding="utf-8")
        self.assertIn("internal val globalKeyboardShortcutController = GlobalKeyboardShortcutController()", s)
        self.assertIn("override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean", s)
        self.assertNotIn("super.dispatchKeyEvent(event)", s)
        self.assertIn("defaultPrevented = false", s)
        self.assertLess(
            s.index("globalKeyboardShortcutController.dispatch("),
            s.index("super.onKeyDown(keyCode, event)"),
        )
        self.assertIn("globalKeyboardShortcutController.clearPresentationBinding()", s)
        self.assertIn("keyboardShortcutController = globalKeyboardShortcutController", s)

    def test_presentation_binding_is_typed_and_recreation_fenced(self):
        controller = CONTROLLER.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertIn("interface GlobalKeyboardShortcutPresentationBinding", controller)
        self.assertIn("fun bindPresentation(): GlobalKeyboardShortcutPresentationBinding", controller)
        self.assertIn("writeIfCurrent(epoch)", controller)
        self.assertIn("keyboardShortcutController: GlobalKeyboardShortcutController", bootstrap)
        self.assertIn("keyboardShortcutController = keyboardShortcutController", bootstrap)
        self.assertIn("keyboardShortcutController: GlobalKeyboardShortcutController", renderer)
        self.assertIn("keyboardShortcutController.bindPresentation()", renderer)
        self.assertNotIn("activity.globalKeyboardShortcutController", renderer)
        self.assertNotIn("activity as MainActivity", renderer)

    def test_root_shell_navigation_uses_durable_multi_step_owner(self):
        s = RENDERER.read_text(encoding="utf-8")
        owner = (ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/RootShellNavigationHistory.kt").read_text(encoding="utf-8")
        self.assertNotIn("KeyboardForwardTarget", s)
        self.assertIn("createRootShellNavigationOwner(activity.applicationContext)", s)
        self.assertIn("rootShellNavigationOwner.observeAccount(", s)
        self.assertIn("rootShellNavigationOwner.observeAppliedAgent(botState.activeBot?.id)", s)
        self.assertIn("RootShellNavigationDirection.BACK", s)
        self.assertIn("RootShellNavigationDirection.FORWARD", s)
        self.assertIn("val entries: List<String>", owner)
        self.assertIn("val cursor: Int", owner)
        self.assertIn("val pendingTarget: String?", owner)
        self.assertIn("val appliedAgentId: String?", owner)
        self.assertIn("candidate in availableAgentIds", owner)
        self.assertIn("persistence.load(nextScope)", owner)
        self.assertIn("nextAccountEpoch < current.accountEpoch", owner)

    def test_registry_covers_contract_and_agent_bounds(self):
        s = CONTROLLER.read_text(encoding="utf-8")
        for n in ('listOf("mod+n")','listOf("mod+k")','listOf("mod+comma")','listOf("mod+shift+m")',
                  'listOf("mod+i", "mod+l")','listOf("mod+shift+f")','listOf("alt+up")',
                  'listOf("alt+down")','listOf("mod+bracketleft")','listOf("mod+bracketright")',
                  'listOf("mod+b")',"agentCount.coerceIn(0, 9)","overlay.isOverlayStacked",
                  "allowPreventedRootSearchEscape"):
            self.assertIn(n, s)

    def test_renderer_uses_existing_product_owners(self):
        s = RENDERER.read_text(encoding="utf-8")
        self.assertIn("createRootShellShortcutActions(", s)
        self.assertIn("CommandPaletteTab.AGENTS", s)
        self.assertIn("CommandPaletteTab.ACTIONS", s)
        self.assertIn("botState.bots.getOrNull(index - 1)", s)
        self.assertIn("agentCount = botState.bots.size", s)
        self.assertNotIn("keyboardForwardTarget", s)
        self.assertIn("rootShellNavigationOwner.navigate(", s)
        self.assertIn("onPromptEditableFocusChanged = keyboardBinding::setEditableFocused", s)

    def test_native_new_agent_focus_and_find_triggers_exist(self):
        home = HOME.read_text(encoding="utf-8")
        chat = CHAT.read_text(encoding="utf-8")
        self.assertIn("shortcutNewAgentNonce", home)
        self.assertIn("createOpen = true", home)
        self.assertIn("shortcutFocusPromptNonce", home)
        self.assertIn("shortcutFindInChatNonce", home)
        self.assertIn("promptFocusRequester.requestFocus()", chat)
        self.assertIn("findOpen = true", chat)
        self.assertIn("onPromptEditableFocusChanged(it.isFocused)", chat)

    def test_jvm_contract_has_required_cases(self):
        s = TEST.read_text(encoding="utf-8")
        for n in ("orderedRegistryCoversDesktopChordsAndBoundsFocusAgents",
                  "editableDefaultPreventedRepeatAndOptionalFindAreFenced",
                  "escapeOnlyClosesRootOverlayWhenUnstackedAndAllowed",
                  "presentationBindingClearsAcrossActivityRecreation"):
            self.assertIn(n, s)

if __name__ == "__main__":
    unittest.main()
