import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
ACTIVITY = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt"
BOOTSTRAP = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/bootstrap_view.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
BOT = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokBotComponents.kt"


class WindowChromeAndroidReplacementTest(unittest.TestCase):
    def test_system_window_owns_chrome_and_root_consumes_safe_drawing_insets(self):
        activity = ACTIVITY.read_text(encoding="utf-8")
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")

        self.assertIn("enableEdgeToEdge()", activity)
        self.assertIn(".safeDrawingPadding()", bootstrap)
        self.assertIn(".fillMaxSize()", bootstrap)
        self.assertIn("ProductionRenderer(", bootstrap)
        self.assertNotIn("windowControls.minimize", renderer)
        self.assertNotIn("windowControls.toggleMaximize", renderer)
        self.assertNotIn("windowControls.close", renderer)

    def test_workspace_indicator_is_replaced_by_native_agent_header(self):
        bot = BOT.read_text(encoding="utf-8")
        self.assertIn("internal fun GrokBotChatAndroid(", bot)
        self.assertIn("Text(bot.name", bot)
        self.assertIn('testTag("mobile-bot-settings")', bot)
        self.assertIn('FabushiAppAgentSurface.Element("mobile-bot-chat", "application", "Bot ${bot.name}")', bot)


if __name__ == "__main__":
    unittest.main()
