import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
PRIVACY = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/roster/privacy-blocked_view.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"


class RosterPrivacyBlockWiringTest(unittest.TestCase):
    def test_native_dialog_is_fail_closed_and_non_dismissible(self):
        source = PRIVACY.read_text(encoding="utf-8")
        self.assertIn('code == "CLOUD_AGENT_STORAGE_DISABLED"', source)
        self.assertIn('transportKind == "no_storage"', source)
        self.assertIn("dismissOnBackPress = false", source)
        self.assertIn("dismissOnClickOutside = false", source)
        self.assertIn("enabled = !logoutBusy", source)
        self.assertIn("logoutError?.takeIf(String::isNotBlank)", source)

    def test_shipping_renderer_uses_canonical_projection_and_owner_actions(self):
        source = RENDERER.read_text(encoding="utf-8")
        self.assertIn("isRosterPrivacyBlocked(state.accountAccessProjection)", source)
        self.assertIn("onSignOut = model::logout", source)
        self.assertIn("runtimePort.launchExternalUrl(ROSTER_PRIVACY_SETTINGS_URL)", source)
        self.assertNotIn("SharedPreferences", source)

    def test_privacy_settings_destination_is_pinned(self):
        source = PRIVACY.read_text(encoding="utf-8")
        self.assertIn(
            '"https://cursor.com/dashboard/settings?openPrivacy=true"',
            source,
        )


if __name__ == "__main__":
    unittest.main()
