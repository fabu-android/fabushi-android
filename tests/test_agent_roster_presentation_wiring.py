import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOME = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokHomeSurface.kt"
ROW = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokBotComponents.kt"
VM = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MobileBotViewModel.kt"
ACTIONS = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/AgentRowActions_view.kt"
DELETE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/AgentDeleteConfirmation_view.kt"
READINESS = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/roster/access-readiness.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"


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

    def test_shipping_roster_readiness_is_derived_from_coordinator_truth(self):
        readiness = READINESS.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")
        home = HOME.read_text(encoding="utf-8")
        row = ROW.read_text(encoding="utf-8")

        self.assertIn("access: AccountAccessProjection", readiness)
        self.assertIn("connection: RootConnectionResilienceProjection", readiness)
        self.assertIn('?.let { "epoch:$it" }', readiness)
        self.assertNotIn("SharedPreferences", readiness)

        self.assertIn("val rosterAccessReadiness = selectRosterAccessReadiness(", renderer)
        self.assertIn("access = state.accountAccessProjection", renderer)
        self.assertIn("connection = state.rootConnectionProjection", renderer)
        self.assertIn("rosterAccessReadiness = rosterAccessReadiness", renderer)

        self.assertIn("rosterAccessReadiness.isLoaded && rosterAccessReadiness.isConnected", home)
        self.assertIn("internal fun GrokHomeSurface(", home)
        self.assertNotIn("\nfun GrokHomeSurface(", home)
        self.assertIn("interactionReady = !canonicalRosterAgent || rosterAccessReadiness.isSelectionReady", home)
        self.assertIn("interactionReady: Boolean", row)
        self.assertIn("interactionReady &&", row)


if __name__ == "__main__":
    unittest.main()
