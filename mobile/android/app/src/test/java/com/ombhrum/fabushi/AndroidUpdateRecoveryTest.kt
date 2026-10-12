package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class AndroidUpdateRecoveryTest {
    private fun marker(
        phase: AndroidUpdateApplyPhase,
        recordedAtMs: Long = 1_000L,
    ) = AndroidUpdateApplyMarker(
        phase = phase,
        targetVersion = "1.2.4",
        targetVersionCode = 124L,
        fromVersion = "1.2.3",
        fromVersionCode = 123L,
        apkName = "fabushi-1.2.4.apk",
        sha256 = "a".repeat(64),
        size = 42L,
        recordedAtMs = recordedAtMs,
    )

    @Test
    fun stagedPackageRecoversWithoutReplayingInstaller() {
        assertEquals(
            AndroidUpdateRecoveryDecision.RECOVER_STAGED,
            AndroidUpdateRecoveryPolicy.decide(marker(AndroidUpdateApplyPhase.STAGED), "1.2.3", 123L, 2_000L),
        )
    }

    @Test
    fun requestedInstallerBecomesOutcomeUnknownAfterProcessRestart() {
        assertEquals(
            AndroidUpdateRecoveryDecision.OUTCOME_UNKNOWN,
            AndroidUpdateRecoveryPolicy.decide(marker(AndroidUpdateApplyPhase.REQUESTED), "1.2.3", 123L, 2_000L),
        )
    }

    @Test
    fun confirmedTargetClearsMarkerEvenWhenVersionCodeAdvancedFurther() {
        assertEquals(
            AndroidUpdateRecoveryDecision.CONFIRMED,
            AndroidUpdateRecoveryPolicy.decide(marker(AndroidUpdateApplyPhase.REQUESTED), "1.2.4", 125L, 2_000L),
        )
    }

    @Test
    fun failedInstallerLaunchRequiresExplicitRetry() {
        assertEquals(
            AndroidUpdateRecoveryDecision.RETRYABLE_FAILURE,
            AndroidUpdateRecoveryPolicy.decide(marker(AndroidUpdateApplyPhase.SPAWN_FAILED), "1.2.3", 123L, 2_000L),
        )
    }

    @Test
    fun staleBinaryOrOldMarkerIsOrphanedInsteadOfReplayed() {
        assertEquals(
            AndroidUpdateRecoveryDecision.ORPHANED,
            AndroidUpdateRecoveryPolicy.decide(marker(AndroidUpdateApplyPhase.REQUESTED), "1.2.3", 122L, 2_000L),
        )
        val thirtyOneDays = 31L * 24L * 60L * 60L * 1_000L
        assertEquals(
            AndroidUpdateRecoveryDecision.ORPHANED,
            AndroidUpdateRecoveryPolicy.decide(
                marker(AndroidUpdateApplyPhase.REQUESTED, recordedAtMs = 1_000L),
                "1.2.3",
                123L,
                1_000L + thirtyOneDays,
            ),
        )
    }
}
