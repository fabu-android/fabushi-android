import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOST = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/RootShellNotificationHost.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
PORT = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidPresentationRuntimePort.kt"
HOST_RUST = ROOT / "source/host/src/android_json_runtime.rs"
TRAYS = ROOT / "source/host/src/extensions/trays/trays_service.rs"

class RootShellNotificationHostWiringTest(unittest.TestCase):
    def test_host_is_canonical_and_renderer_only_projects_it(self):
        host = HOST.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")
        port = PORT.read_text(encoding="utf-8")
        rust = HOST_RUST.read_text(encoding="utf-8")
        trays = TRAYS.read_text(encoding="utf-8")
        self.assertIn("RootShellNotificationHost(", renderer)
        self.assertIn("runtimePort.rootNotificationTrays()", host)
        self.assertIn("addRootNotificationEventListener", host)
        self.assertIn("fun rootNotificationTrays(): JSONArray", port)
        self.assertIn('"getTrays"', rust)
        self.assertIn('"dismissTray"', rust)
        self.assertIn('"clearTrays"', rust)
        self.assertIn("pub const MAX_TRAYS: usize = 20", trays)
        self.assertNotIn("SharedPreferences", host)

    def test_lifecycle_is_account_transport_and_generation_fenced(self):
        host = HOST.read_text(encoding="utf-8")
        for token in (
            "requestGeneration = ++generation",
            "requestGeneration != generation",
            "queuedEvents",
            "observeAccount",
            "observeTransport",
            "RootConnectionTransportState.CONNECTED",
            "if (!active || !loggedIn) return",
        ):
            self.assertIn(token, host)

    def test_actions_are_bounded_and_fail_closed(self):
        host = HOST.read_text(encoding="utf-8")
        self.assertIn("if (size >= 3) break", host)
        self.assertIn('scheme != "http" && scheme != "https"', host)
        self.assertIn("rawArgs.opt(key) as? String ?: return null", host)
        self.assertIn("invokeRootNotificationDashboardAction", host)
        self.assertIn("key in _pendingActions.value", host)

if __name__ == "__main__":
    unittest.main()
