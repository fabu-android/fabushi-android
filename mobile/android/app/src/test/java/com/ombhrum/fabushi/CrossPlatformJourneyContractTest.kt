package com.ombhrum.fabushi

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CrossPlatformJourneyContractTest {
    @Test
    fun desktopCrossPlatformJourneysRemainMachineReadableAndBoundToAndroidOwners() {
        val root = repositoryRoot()
        val contract = JSONObject(
            File(root, "contracts/automation/cross-platform-journeys.json").readText(),
        )
        assertEquals(1, contract.getInt("schemaVersion"))
        assertEquals("android", contract.getString("platform"))
        assertEquals(
            "3bc92400826cc4ca7ac665b467708e22261edc61",
            contract.getJSONObject("sourceAuthority").getString("commit"),
        )

        val features = contract.getJSONArray("features")
        val expected = linkedMapOf(
            "auth.login" to "oauthLogin",
            "runtime.boot" to "expectReady",
            "chat.send" to "sendChat",
            "marketplace.install" to "installMiniApp",
            "miniapp.open" to "openMiniApp",
            "capability.approval" to "approveCapability",
            "operation.interrupt" to "interruptOperation",
            "session.clear" to "clearSession",
        )
        val actual = linkedMapOf<String, String>()
        for (index in 0 until features.length()) {
            val feature = features.getJSONObject(index)
            val step = feature.getJSONArray("steps").getJSONObject(0)
            assertTrue(step.getString("androidOwner").isNotBlank())
            actual[feature.getString("id")] = step.getString("action")
        }
        assertEquals(expected, actual)

        val coordinator = File(
            root,
            "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt",
        ).readText()
        val host = File(root, "source/host/src/android_json_runtime.rs").readText()
        val miniApp = File(
            root,
            "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/miniapp/MiniAppWebMcpSurface.kt",
        )
        val marketplace = File(
            root,
            "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MarketplaceViewModel.kt",
        )
        val broker = File(root, "source/host/src/capability_broker.rs")

        assertTrue(coordinator.contains("feature.execute"))
        assertTrue(host.contains("\"chat.send\""))
        assertTrue(host.contains("\"feature.interrupt\""))
        assertTrue(host.contains("\"feature.approval.resolve\""))
        assertTrue(miniApp.isFile)
        assertTrue(marketplace.isFile)
        assertTrue(broker.isFile)
    }

    private fun repositoryRoot(): File {
        var current = File(System.getProperty("user.dir")).canonicalFile
        repeat(8) {
            if (File(current, "docs/specs/desktop-main-android-full-parity.md").isFile) {
                return current
            }
            current = current.parentFile ?: return@repeat
        }
        error("repository root not found")
    }
}
