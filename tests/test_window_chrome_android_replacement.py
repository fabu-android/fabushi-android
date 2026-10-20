import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
ACTIVITY = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt"
BOOTSTRAP = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/bootstrap_view.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
BOT = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokBotComponents.kt"
LIGHT_STYLES = ROOT / "mobile/android/app/src/main/res/values/styles.xml"
LIGHT_COLORS = ROOT / "mobile/android/app/src/main/res/values/colors.xml"
DARK_STYLES = ROOT / "mobile/android/app/src/main/res/values-night/styles.xml"
DARK_COLORS = ROOT / "mobile/android/app/src/main/res/values-night/colors.xml"


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

    def test_native_window_background_tracks_resolved_light_and_dark_shell(self):
        activity = ACTIVITY.read_text(encoding="utf-8")
        light_styles = LIGHT_STYLES.read_text(encoding="utf-8")
        light_colors = LIGHT_COLORS.read_text(encoding="utf-8")
        dark_styles = DARK_STYLES.read_text(encoding="utf-8")
        dark_colors = DARK_COLORS.read_text(encoding="utf-8")

        self.assertIn("enableEdgeToEdge()", activity)
        self.assertIn('android:windowBackground">@color/fabushi_window_background', light_styles)
        self.assertIn('android:navigationBarColor">@color/fabushi_window_background', light_styles)
        self.assertIn('android:windowLightStatusBar">true', light_styles)
        self.assertNotIn("android:windowLightNavigationBar", light_styles)
        self.assertIn("#FCFCFC", light_colors)

        self.assertIn('android:style/Theme.Material.NoActionBar', dark_styles)
        self.assertIn('android:windowBackground">@color/fabushi_window_background', dark_styles)
        self.assertIn('android:navigationBarColor">@color/fabushi_window_background', dark_styles)
        self.assertIn('android:windowLightStatusBar">false', dark_styles)
        self.assertNotIn("android:windowLightNavigationBar", dark_styles)
        self.assertIn("#0B0B0B", dark_colors)

    def test_workspace_indicator_is_replaced_by_native_agent_header(self):
        bot = BOT.read_text(encoding="utf-8")
        self.assertIn("internal fun GrokBotChatAndroid(", bot)
        self.assertIn("Text(bot.name", bot)
        self.assertIn('testTag("mobile-bot-settings")', bot)
        self.assertIn('FabushiAppAgentSurface.Element("mobile-bot-chat", "application", "Bot ${bot.name}")', bot)


if __name__ == "__main__":
    unittest.main()
