import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
OWNER = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AccountAccessProjectionOwner.kt"
RUNTIME = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
PORT = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt"
VIEWMODEL = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MarketplaceViewModel.kt"
HOST = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/root-resilience/connection-state_view.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"


class RootConnectionResilienceWiringTest(unittest.TestCase):
    def test_coordinator_is_the_single_root_connection_owner(self):
        owner = OWNER.read_text(encoding="utf-8")
        runtime = RUNTIME.read_text(encoding="utf-8")
        viewmodel = VIEWMODEL.read_text(encoding="utf-8")

        self.assertIn("internal class RootConnectionResilienceOwner", owner)
        self.assertIn("private val rootConnectionOwner = RootConnectionResilienceOwner()", runtime)
        self.assertIn("override fun rootConnectionRetry()", runtime)
        self.assertIn("override fun addRootConnectionResilienceListener", runtime)
        self.assertIn("rootConnectionOwner.observeAccount(", runtime)
        self.assertIn("rootConnectionOwner.observeReadiness(settled)", runtime)
        self.assertNotIn("RootConnectionResilienceOwner(", viewmodel)
        self.assertIn("coordinator.rootConnectionRetry()", viewmodel)

    def test_typed_port_exposes_projection_retry_and_subscription(self):
        source = PORT.read_text(encoding="utf-8")
        self.assertIn("fun rootConnectionResilienceProjection(): RootConnectionResilienceProjection", source)
        self.assertIn("fun rootConnectionRetry(): RootConnectionResilienceProjection", source)
        self.assertIn("fun addRootConnectionResilienceListener(", source)

    def test_shipping_renderer_consumes_immutable_projection(self):
        renderer = RENDERER.read_text(encoding="utf-8")
        viewmodel = VIEWMODEL.read_text(encoding="utf-8")
        self.assertIn("RootConnectionResilienceHost(", renderer)
        self.assertIn("projection = state.rootConnectionProjection", renderer)
        self.assertIn("onRetry = model::retryRootConnection", renderer)
        self.assertIn("val rootConnectionProjection: RootConnectionResilienceProjection", viewmodel)

    def test_native_host_reuses_existing_roster_surfaces(self):
        source = HOST.read_text(encoding="utf-8")
        self.assertIn("RosterStatus(", source)
        self.assertIn("RosterReconnectNotice(", source)
        self.assertIn("RootConnectionPhase.LOADING", source)
        self.assertIn("RootConnectionPhase.RECONNECTING", source)
        self.assertIn("RootConnectionPhase.UNREACHABLE", source)


if __name__ == "__main__":
    unittest.main()
