package com.ombhrum.fabushi

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateQuiesceContractTest {
    @Test
    fun installerHandoffUsesExtractedPolicyAndProductionRestoresHostOnAmbiguousReturn() {
        val order = mutableListOf<String>()
        val transitions = mutableListOf<Boolean>()
        var idleChecks = 0

        val preparation = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = {
                idleChecks += 1
                order += "idle"
                true
            },
            setQuiescing = { quiescing ->
                transitions += quiescing
                order += if (quiescing) "quiesce" else "resume"
                true
            },
            persistRequested = { order += "marker" },
        )

        assertEquals(AndroidUpdateHandoffPreparation.READY, preparation)
        assertEquals(2, idleChecks)
        assertEquals(listOf(true), transitions)
        assertEquals(listOf("idle", "quiesce", "idle", "marker"), order)

        val updater = File(
            repositoryRoot(),
            "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/update/AndroidUpdateViewModel.kt",
        ).readText()
        val requestMethod = updater.substringAfter("private fun requestPackageInstall(apk: File)")
            .substringBefore("private fun markerFor(")

        val prepareCall = requestMethod.indexOf("prepareAndroidUpdateInstallerHandoff(")
        val idle = requestMethod.indexOf("isHostIdle = { coordinator.isUpdateHostIdle() }", prepareCall)
        val quiesce = requestMethod.indexOf("setQuiescing = ::setHostUpgradeQuiescing", prepareCall)
        val marker = requestMethod.indexOf("persistRequested = { applyStore.write(requested) }", prepareCall)
        val installer = requestMethod.indexOf("applicationContext.startActivity(intent)", prepareCall)
        assertTrue("production updater must call the extracted handoff policy", prepareCall >= 0)
        assertTrue("production policy call must use Coordinator Host idle truth", idle > prepareCall)
        assertTrue("production policy call must use the typed Host quiesce setter", quiesce > prepareCall)
        assertTrue("production policy call must persist the durable REQUESTED marker", marker > prepareCall)
        assertTrue("installer must start only after handoff preparation", installer > marker)

        val spawnFailure = requestMethod.substring(installer)
        assertTrue(
            "installer spawn failure must restore Host turns",
            spawnFailure.contains("setHostUpgradeQuiescing(false)"),
        )

        val foreground = updater.substringAfter("fun setForeground(foreground: Boolean)")
            .substringBefore("private fun startIdleAutoStageMonitor()")
        val installing = foreground.indexOf("AndroidUpdatePhase.INSTALLING")
        val resumeOnReturn = foreground.indexOf("setHostUpgradeQuiescing(false)", installing)
        val ambiguousReturn = foreground.indexOf("AndroidUpdatePhase.OUTCOME_UNKNOWN", installing)
        assertTrue("foreground return must recognize an in-flight installer handoff", installing >= 0)
        assertTrue(
            "ambiguous installer return must restore Host before projecting outcome-unknown",
            resumeOnReturn > installing && ambiguousReturn > resumeOnReturn,
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
