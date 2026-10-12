import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
APPLICATION = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/FabushiApplication.kt"
FEED = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/notifications/agents-control-feed.kt"
COORDINATOR = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"


class AndroidNotificationProductionWiringTest(unittest.TestCase):
    def test_process_runtime_seeds_then_subscribes_to_canonical_agent_feed(self):
        application = APPLICATION.read_text(encoding="utf-8")
        self.assertIn("runCatching { coordinator.agentList() }", application)
        self.assertIn('.put("kind", "agents-roster-seed")', application)
        self.assertIn("coordinator.addFeatureEventListener(::handleNotificationFeatureEvent)", application)
        self.assertIn("private fun handleNotificationFeatureEvent", application)
        self.assertIn('kind == "agents" && eventPayload.optJSONArray("agents") == null', application)
        self.assertGreaterEqual(application.count("coordinator.agentList()"), 2)
        self.assertLess(
            application.index("runCatching { coordinator.agentList() }"),
            application.index("coordinator.addFeatureEventListener(::handleNotificationFeatureEvent)"),
        )

    def test_native_agent_projection_shape_is_accepted_by_notification_feed(self):
        feed = FEED.read_text(encoding="utf-8")
        coordinator = COORDINATOR.read_text(encoding="utf-8")
        self.assertIn('event.optString("type")', feed)
        self.assertIn('.put("type", "agent-upserted")', coordinator)
        self.assertIn('.put("type", "agents")', coordinator)

    def test_notification_owner_does_not_create_second_roster_store(self):
        application = APPLICATION.read_text(encoding="utf-8")
        feed = FEED.read_text(encoding="utf-8")
        self.assertNotIn("SharedPreferences", feed)
        self.assertIn("coordinator.agentList()", application)


if __name__ == "__main__":
    unittest.main()
