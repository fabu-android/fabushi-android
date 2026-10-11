import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOME = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokHomeSurface.kt"
ROW = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokBotComponents.kt"
VM = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MobileBotViewModel.kt"
ACTIONS = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/AgentRowActions_view.kt"
DELETE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/AgentDeleteConfirmation_view.kt"


class AgentRosterPresentationWiringTest(unittest.TestCase):
    def test_shipping_roster_composes_desktop_equivalent_controls(self):
        home = HOME.read_text(encoding="utf-8")
        row = ROW.read_text(encoding="utf-8")
        self.assertIn("AgentRowActions(", home)
        self.assertIn("AgentDeleteConfirmation(", home)
        self.assertIn("AgentNameEditor(", row)
        for callback in (
            "onHideFromSidebar = onHideBot",
            "onDuplicateAgent = onDuplicateBot",
            "onTogglePin = onSetBotPinned",
            "onSetAgentUnread = onSetBotUnread",
            "onRequestDelete = { deleteTarget = it }",
        ):
            self.assertIn(callback, home)

    def test_presentation_delegates_mutations_to_coordinator(self):
        source = VM.read_text(encoding="utf-8")
        for call in (
            "coordinator.agentUpdate(",
            "coordinator.agentSetHidden(",
            "coordinator.agentSetUnread(",
            "coordinator.agentDuplicate(",
            "coordinator.agentDelete(",
            "coordinator.agentSetPinned(",
        ):
            self.assertIn(call, source)
        self.assertIn("committedAgentName(bot.name, name)", source)
        mutation_slice = source[source.index("fun renameBot("):source.index("fun openBot(")]
        self.assertNotIn("SharedPreferences", mutation_slice)

    def test_delete_dialog_stays_nonoptimistic_while_host_result_is_pending(self):
        source = DELETE.read_text(encoding="utf-8")
        self.assertIn("if (!pending) onClose()", source)
        self.assertIn("enabled = !pending", source)
        self.assertIn("runCatching { onConfirm(agent.id) }", source)
        self.assertIn(".onSuccess { onClose() }", source)
        self.assertIn("Deleting failed. Check your connection and try again.", source)

    def test_row_menu_owns_only_ephemeral_menu_state(self):
        source = ACTIONS.read_text(encoding="utf-8")
        self.assertIn("var expanded by remember(agentId) { mutableStateOf(false) }", source)
        self.assertNotIn("SharedPreferences", source)
        self.assertNotIn("AndroidCoordinatorRuntime(", source)
        self.assertNotIn("MahayanaHost(", source)


if __name__ == "__main__":
    unittest.main()
