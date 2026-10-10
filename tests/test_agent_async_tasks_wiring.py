import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOST = ROOT / "source/host/src/android_json_runtime.rs"
PORT = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt"
RUNTIME = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
VM = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MobileBotViewModel.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
PANEL = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/AgentAsyncTasksPanel_view.kt"


class AgentAsyncTasksWiringTest(unittest.TestCase):
    def test_host_truth_is_durable_parent_and_account_fenced(self):
        source = HOST.read_text(encoding="utf-8")
        self.assertIn('"feature.agent.asyncTasks" => self.agent_async_tasks(params)', source)
        self.assertIn(".list_running_for_parent(parent_agent_id)", source)
        self.assertIn(".filter(|record| record.account_fence == account_fence)", source)
        self.assertNotIn('"mobileToken"', source[source.index("fn agent_async_tasks"):source.index("fn project_agent_roster")])

    def test_typed_port_reaches_host_without_presentation_bypass(self):
        port = PORT.read_text(encoding="utf-8")
        runtime = RUNTIME.read_text(encoding="utf-8")
        self.assertIn("fun agentAsyncTasks(id: String): JSONArray", port)
        self.assertIn('host.requestValue("feature.agent.asyncTasks"', runtime)

    def test_shipping_surface_uses_native_panel_not_command_palette_placeholder(self):
        renderer = RENDERER.read_text(encoding="utf-8")
        self.assertIn("onShowBotAsyncTasks = botModel::openAsyncTasks", renderer)
        self.assertIn("AgentAsyncTasksPanel(", renderer)
        callback_start = renderer.index("onShowBotAsyncTasks =")
        callback_slice = renderer[callback_start:callback_start + 180]
        self.assertNotIn("commandPaletteOpen", callback_slice)

    def test_viewmodel_fences_late_results_and_panel_refreshes_at_desktop_interval(self):
        vm = VM.read_text(encoding="utf-8")
        panel = PANEL.read_text(encoding="utf-8")
        self.assertIn("asyncTasksGeneration", vm)
        self.assertIn("generation != asyncTasksGeneration", vm)
        self.assertIn("mutableState.value.asyncTasksAgent?.id != agent.id", vm)
        self.assertIn("asyncTasksJob?.cancel()", vm)
        self.assertIn("ASYNC_TASKS_REFRESH_INTERVAL_MS = 30_000L", panel)
        self.assertIn("delay(ASYNC_TASKS_REFRESH_INTERVAL_MS)", panel)


if __name__ == "__main__":
    unittest.main()
