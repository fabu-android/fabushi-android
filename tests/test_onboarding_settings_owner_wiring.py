import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
VIEW_MODEL = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/SignedInOnboardingViewModel.kt"
PORT = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt"
RUNTIME = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
HOST = ROOT / "source/host/src/android_json_runtime.rs"
SETTINGS = ROOT / "source/host/src/android_user_settings.rs"
CI_BOOTSTRAP = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/startup/FabushiCiBootstrap.kt"
APPLICATION = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/FabushiApplication.kt"

class OnboardingSettingsOwnerWiringTest(unittest.TestCase):
    def test_presentation_does_not_own_durable_onboarding_seen_truth(self):
        source = VIEW_MODEL.read_text(encoding="utf-8")
        self.assertNotIn("SignedInOnboardingSeenStore", source)
        self.assertNotIn("SharedPreferences", source)
        self.assertNotIn("MessageDigest", source)
        self.assertIn("coordinator.onboardingSeen()", source)
        self.assertIn("coordinator.setOnboardingSeen(true)", source)

    def test_typed_coordinator_routes_onboarding_seen_to_host(self):
        port = PORT.read_text(encoding="utf-8")
        runtime = RUNTIME.read_text(encoding="utf-8")
        self.assertIn("fun onboardingSeen(): JSONObject", port)
        self.assertIn("fun setOnboardingSeen(hasSeenOnboarding: Boolean): JSONObject", port)
        self.assertIn('host.request("feature.settings.onboardingSeen")', runtime)
        self.assertIn('"feature.settings.onboardingSeen.set"', runtime)

    def test_host_owns_account_scoped_durable_onboarding_seen_state(self):
        host = HOST.read_text(encoding="utf-8")
        settings = SETTINGS.read_text(encoding="utf-8")
        self.assertIn('"feature.settings.onboardingSeen" => self.onboarding_seen()', host)
        self.assertIn('"feature.settings.onboardingSeen.set" => self.set_onboarding_seen(params)', host)
        self.assertIn("self.account.account_storage_scope()", host)
        self.assertIn("onboarding_seen_by_account: BTreeMap<String, bool>", settings)
        self.assertIn("pub fn onboarding_seen(&self, account_scope: &str) -> bool", settings)
        self.assertIn("pub fn set_onboarding_seen(", settings)

    def test_ci_bootstrap_uses_host_owned_onboarding_seen_truth(self):
        bootstrap = CI_BOOTSTRAP.read_text(encoding="utf-8")
        application = APPLICATION.read_text(encoding="utf-8")
        self.assertNotIn('"onboarding-complete"', bootstrap)
        self.assertNotIn('getSharedPreferences("fabushi.mobile"', bootstrap)
        self.assertIn("if (ciBootstrapActive)", application)
        self.assertIn("it.setOnboardingSeen(true)", application)
        self.assertIn('optBoolean("hasSeenOnboarding", false)', application)
        self.assertIn("CI authenticated session could not settle canonical onboarding state", application)

    def test_create_success_never_reports_agent_creation_failure_when_seen_persist_fails(self):
        source = VIEW_MODEL.read_text(encoding="utf-8")
        create = source[source.index("fun createFirstAgent()"):]
        self.assertIn("val onboardingSeenPersisted = runCatching", create)
        self.assertIn("Your Agent was created, but onboarding completion could not be saved.", create)
        self.assertIn("step = SignedInOnboardingStep.HAND_OFF", create)

if __name__ == "__main__":
    unittest.main()
