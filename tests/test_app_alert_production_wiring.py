import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROLLER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/app-alert/AppAlertController.kt"
HOST = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/window-chrome/app-alert/AppAlertHost.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"
TEST = ROOT / "mobile/android/app/src/test/java/com/ombhrum/fabushi/AppAlertControllerTest.kt"


class AppAlertProductionWiringTest(unittest.TestCase):
    def test_controller_preserves_queue_single_flight_failure_and_epoch_contracts(self):
        source = CONTROLLER.read_text(encoding="utf-8")
        for needle in (
            "private var waiting: PendingAlert? = null",
            "request.cancelLabel != null || waiting != null",
            "if (current.isPerforming) return",
            "val actionEpoch = epoch",
            "actionEpoch != epoch || active !== entry || disposed",
            "snapshot?.let { publish(it.copy(isPerforming = false, failure = result)) }",
            "subscribers.clear()",
        ):
            self.assertIn(needle, source)

    def test_native_host_suppresses_pending_dismiss_and_exposes_semantics(self):
        source = HOST.read_text(encoding="utf-8")
        self.assertIn("if (!current.isPerforming) controller.cancel()", source)
        self.assertIn("enabled = !current.isPerforming", source)
        self.assertIn("request.pendingLabel ?: request.confirmLabel", source)
        self.assertIn("app-alert-confirm-destructive", source)
        self.assertIn("app-alert-secondary-destructive", source)
        self.assertIn("LiveRegionMode.Assertive", source)
        self.assertIn("focusRequester.requestFocus()", source)

    def test_shipping_renderer_mounts_and_disposes_single_alert_owner(self):
        source = RENDERER.read_text(encoding="utf-8")
        self.assertIn("val appAlertController = remember(coroutineScope)", source)
        self.assertIn("AppAlertController(coroutineScope)", source)
        self.assertIn("DisposableEffect(appAlertController)", source)
        self.assertIn("onDispose(appAlertController::dispose)", source)
        self.assertIn("AppAlertHost(controller = appAlertController)", source)
        self.assertIn("appAlertController.alert(", source)
        self.assertNotIn("AlertDialog(", source)

    def test_focused_jvm_contract_covers_recreation_and_late_completion(self):
        source = TEST.read_text(encoding="utf-8")
        for needle in (
            "confirmCancelQueueAndThirdRequestFollowDesktopContract",
            "performFailureThrowRetrySecondaryAndPendingSuppressionAreDeterministic",
            "resetDisposeLateCompletionSubscribersAndRecreationAreFenced",
            "val recreated = AppAlertController",
        ):
            self.assertIn(needle, source)


if __name__ == "__main__":
    unittest.main()
