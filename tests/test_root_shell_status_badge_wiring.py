import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
BADGE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/RootShellStatusBadge.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"

class RootShellStatusBadgeWiringTest(unittest.TestCase):
    def test_badge_uses_canonical_root_connection_projection(self):
        badge = BADGE.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")
        self.assertIn("RootShellStatusBadge(", renderer)
        self.assertIn("connection = state.rootConnectionProjection", renderer)
        self.assertIn("RootConnectionTransportState.CONNECTING", badge)
        self.assertIn("RootConnectionTransportState.CONNECTED", badge)
        self.assertIn("RootConnectionTransportState.DOWN", badge)
        self.assertIn('"Connecting"', badge)
        self.assertIn('"Connected"', badge)
        self.assertIn('"Disconnected"', badge)
        self.assertNotIn("mutableStateOf", badge)

    def test_native_hidden_state_replaces_desktop_browser_fullscreen_visibility(self):
        badge = BADGE.read_text(encoding="utf-8")
        self.assertIn("!loggedIn || connection.phase == RootConnectionPhase.HIDDEN", badge)

if __name__ == "__main__":
    unittest.main()
