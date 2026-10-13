package com.ombhrum.fabushi

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateQuiesceContractTest {
    @Test
    fun installerHandoffIsFencedByTypedHostQuiesceAndRestoresOnAmbiguousReturn() {
        val root = repositoryRoot()
        val port = File(
            root,
            "source/android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt",
        ).readText()
        val runtime = File(
            root,
            "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt",
        ).readText()
        val updater = File(
            root,
            "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/update/AndroidUpdateViewModel.kt",
        ).readText()

        assertTrue(port.contains("fun setUpgradeQuiescing(quiescing: Boolean)"))
        assertTrue(runtime.contains("\"feature.agent.upgradeQuiesce\""))
        assertTrue(runtime.contains("JSONObject().put(\"quiescing\", quiescing)"))

        val requestMethod = updater.substringAfter("private fun requestPackageInstall(apk: File)")
            .substringBefore("private fun markerFor(")
        val quiesce = requestMethod.indexOf("setHostUpgradeQuiescing(true)")
        val markerWrite = requestMethod.indexOf("applyStore.write(requested)")
        val installer = requestMethod.indexOf("applicationContext.startActivity(intent)")
        val resumeOnSpawnFailure = requestMethod.indexOf("setHostUpgradeQuiescing(false)")
        assertTrue("Host must quiesce before durable installer request marker", quiesce >= 0 && quiesce < markerWrite)
        assertTrue("durable request marker must precede external installer handoff", markerWrite >= 0 && markerWrite < installer)
        assertTrue(
            "installer spawn failure must restore Host turns",
            resumeOnSpawnFailure > installer,
        )

        val foreground = updater.substringAfter("fun setForeground(foreground: Boolean)")
            .substringBefore("private fun startIdleAutoStageMonitor()")
        val ambiguousReturn = foreground.indexOf("AndroidUpdatePhase.OUTCOME_UNKNOWN")
        val resumeOnReturn = foreground.indexOf("setHostUpgradeQuiescing(false)")
        assertTrue(
            "ambiguous installer return must restore Host turns without replaying install",
            resumeOnReturn >= 0 && resumeOnReturn < ambiguousReturn,
        )
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
