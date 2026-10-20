import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

PORT = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt"
RUNTIME = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"
BRIDGE = ROOT / "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/miniapp/MiniAppPlatformBridge.kt"
VIEW_MODEL = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MarketplaceViewModel.kt"
SURFACE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/miniapp/MiniAppWebMcpSurface.kt"
LEGACY_SURFACE = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi/MiniAppWebMcpSurface.kt"


class MiniAppHostWiringTest(unittest.TestCase):
    def test_typed_port_routes_every_host_owned_lifecycle_operation(self):
        port = PORT.read_text(encoding="utf-8")
        runtime = RUNTIME.read_text(encoding="utf-8")
        for method in (
            "listApprovedMiniApps",
            "listInstalledMiniApps",
            "installApprovedMiniApp",
            "getInstalledMiniAppResources",
            "readMiniAppResource",
            "disposeMiniAppLoad",
            "uninstallMiniApp",
        ):
            self.assertIn(method, port)
        for dispatch in (
            "feature.miniapp.listApproved",
            "feature.miniapp.listInstalled",
            "feature.miniapp.installApproved",
            "feature.miniapp.getResources",
            "feature.miniapp.readResource",
            "feature.miniapp.disposeLoad",
            "feature.miniapp.uninstall",
        ):
            self.assertIn(dispatch, runtime)

    def test_legacy_installed_projection_and_install_endpoint_are_not_production_owners(self):
        bridge = BRIDGE.read_text(encoding="utf-8")
        view_model = VIEW_MODEL.read_text(encoding="utf-8")
        self.assertNotIn("CanonicalInstalledProjectionCache", bridge)
        self.assertNotIn("/v1/marketplace/added", bridge)
        self.assertNotIn("/v1/marketplace/plugins/$pluginId/add", bridge)
        self.assertNotIn("coordinator.pluginInstall(", view_model)
        self.assertIn("coordinator.installApprovedMiniApp(", view_model)
        self.assertIn("coordinator.getInstalledMiniAppResources(", view_model)
        self.assertIn("coordinator.readMiniAppResource(", view_model)
        self.assertIn("coordinator.disposeMiniAppLoad(", view_model)


    def test_offline_capabilities_probe_stays_in_the_single_current_webmcp_owner(self):
        surface = SURFACE.read_text(encoding="utf-8")
        self.assertFalse(LEGACY_SURFACE.exists(), "legacy mobile MiniApp owner must not return")
        self.assertIn('CAPABILITY_PROBE_TOOL = "offline.capabilities"', surface)
        self.assertIn('MINI_APP_CAPABILITY_PREFIXES = setOf("offline", "selection", "transfer", "scan", "hotspot", "update")', surface)
        self.assertIn('miniAppCapabilityAnswer(plugin.tools)', surface)
        self.assertIn('.put("contract", "fabushi.miniapp.tools.v1")', surface)
        self.assertIn('.put("platform", "android")', surface)
        self.assertIn('.put("targetPools", JSONArray())', surface)
        self.assertIn('.put("name", CAPABILITY_PROBE_TOOL)', surface)
        self.assertLess(surface.index("if (name == CAPABILITY_PROBE_TOOL)"), surface.index("if (!pending.add(requestId))"))
        self.assertLess(surface.index("if (name == CAPABILITY_PROBE_TOOL)"), surface.index("callRuntimeToolJson("))
        self.assertLess(surface.index('if ("tools/call" !in session.grants)'), surface.index("if (name == CAPABILITY_PROBE_TOOL)"))


if __name__ == "__main__":
    unittest.main()
