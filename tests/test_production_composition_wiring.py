import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
APPLICATION = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/FabushiApplication.kt"
ACTIVITY = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt"
COORDINATOR = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
PORTS = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorPorts.kt"
BRIDGE = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorBridge.kt"
HOST = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/core/MahayanaHost.kt"
BOOTSTRAP = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/bootstrap_view.kt"
RENDERER = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/ProductionRenderer_view.kt"


class ProductionCompositionWiringTest(unittest.TestCase):
    def test_process_scope_owns_the_typed_runtime(self):
        application = APPLICATION.read_text(encoding="utf-8")
        self.assertIn("private var processRuntime: FabushiProcessRuntime? = null", application)
        self.assertIn("internal fun ensurePresentationRuntime", application)
        self.assertIn("AndroidCoordinatorPorts.presentation(application)", application)
        self.assertIn("AndroidCoordinatorBridge.installTrustedRuntime(it)", application)
        self.assertIn("processRuntime?.close()", application)

    def test_activity_only_forwards_lifecycle_and_runtime_port(self):
        activity = ACTIVITY.read_text(encoding="utf-8")
        self.assertIn("(application as FabushiApplication).ensurePresentationRuntime(intent)", activity)
        self.assertIn("FabushiApplicationRoot(", activity)
        self.assertIn("runtimePort = runtimePort", activity)
        self.assertIn("attachInteractiveActivity(this@MainActivity)", activity)
        self.assertIn("setForeground(true)", activity)
        self.assertNotIn("MahayanaHost(", activity)
        self.assertNotIn("AndroidCoordinatorRuntime(", activity)

    def test_coordinator_is_the_only_production_host_constructor_above_jni(self):
        coordinator = COORDINATOR.read_text(encoding="utf-8")
        renderer = RENDERER.read_text(encoding="utf-8")
        self.assertIn("class AndroidCoordinatorRuntime private constructor", coordinator)
        self.assertIn("private val host = MahayanaHost(application, processGeneration = processGeneration)", coordinator)
        self.assertNotIn("MahayanaHost(", renderer)
        self.assertIn("CoordinatorClient.presentation()", renderer)

    def test_bootstrap_delegates_to_shipping_native_renderer(self):
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertIn("internal fun FabushiApplicationRoot(", bootstrap)
        self.assertIn("ProductionRenderer(", bootstrap)
        self.assertIn("runtimePort = runtimePort", bootstrap)
        self.assertNotIn("WebView(", bootstrap)
        self.assertNotIn("MahayanaHost(", bootstrap)

    def test_production_host_never_uses_test_native_constructor(self):
        host = HOST.read_text(encoding="utf-8")
        self.assertIn("if (featureHostTest)", host)
        self.assertIn("nativeCreateTest(appDataDir)", host)
        self.assertIn("machineIdStore.getOrCreate()", host)
        self.assertIn("nativeCreate(", host)
        production_branch = host.split("if (featureHostTest)", 1)[1].split("if (!featureHostTest)", 1)[0]
        self.assertIn("nativeCreateTest(appDataDir)", production_branch)
        self.assertIn("nativeCreate(", production_branch)
        self.assertLess(production_branch.index("nativeCreateTest(appDataDir)"), production_branch.index("nativeCreate("))


    def test_process_runtime_quiesces_and_releases_the_single_coordinator_owner(self):
        runtime = COORDINATOR.read_text(encoding="utf-8")
        ports = PORTS.read_text(encoding="utf-8")
        bridge = BRIDGE.read_text(encoding="utf-8")
        application = APPLICATION.read_text(encoding="utf-8")

        self.assertIn("AndroidCoordinatorPort, AutoCloseable", runtime)
        self.assertIn("private val closed = AtomicBoolean(false)", runtime)
        self.assertIn("connectivityManager.unregisterNetworkCallback(sendNetworkCallback)", runtime)
        for executor in (
            "eventPumpExecutor",
            "computerRebuildEventExecutor",
            "computerRebuildMigrationExecutor",
            "computerRebuildQueueExecutor",
            "remoteComputerReconnectExecutor",
            "storagePressureExecutor",
            "sendSubmissionExecutor",
            "rootConnectionRetryExecutor",
        ):
            self.assertIn(f"{executor}.shutdownNow()", runtime)
        self.assertIn("computerRebuildEventHost.close()", runtime)
        self.assertIn("host.close()", runtime)

        pump = runtime[
            runtime.index("private fun startComputerRebuildEventPump()"):
            runtime.index("private fun observeComputerRebuildTransport")
        ]
        self.assertIn("while (!closed.get())", pump)
        self.assertNotIn("while (true)", pump)
        self.assertIn("AndroidCoordinatorRuntime.closeProcessRuntime()", ports)
        self.assertIn("internal fun clearTrustedRuntime(expected: AndroidCoordinatorPort)", bridge)
        self.assertIn("if (installed === expected) installed = null", bridge)
        self.assertIn("AndroidCoordinatorBridge.clearTrustedRuntime(coordinator)", application)
        self.assertIn("AndroidCoordinatorPorts.closeProcessRuntime()", application)


if __name__ == "__main__":
    unittest.main()
