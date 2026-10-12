package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidAccountUsageSummary
import org.junit.Assert.assertEquals
import org.junit.Test

class AccountUsageReconciliationTest {
    private fun activeTrial(canCancel: Boolean = true) = AndroidAccountUsageSummary(
        sandUsagePercent = 42.5,
        sandUsageResetTimestampMs = 2_000_000_000_000L,
        hasAvailableUsage = true,
        hasNonZeroIncludedLimit = true,
        usesPooledEnterpriseAllowance = false,
        isSandTrial = true,
        hasEndedSandTrial = false,
        canCancelSandTrial = canCancel,
        onDemand = null,
    )

    @Test
    fun readBackCanConfirmCancellationEvenWhenMutationTransportFailed() {
        assertEquals(
            SandTrialCancellationOutcome.CONFIRMED,
            classifySandTrialCancellationOutcome(
                cancelSucceeded = false,
                reconciled = activeTrial(canCancel = false),
            ),
        )
    }

    @Test
    fun successfulMutationWithoutCanonicalConfirmationRemainsUnconfirmed() {
        assertEquals(
            SandTrialCancellationOutcome.SUBMITTED_UNCONFIRMED,
            classifySandTrialCancellationOutcome(
                cancelSucceeded = true,
                reconciled = activeTrial(),
            ),
        )
        assertEquals(
            SandTrialCancellationOutcome.SUBMITTED_UNCONFIRMED,
            classifySandTrialCancellationOutcome(
                cancelSucceeded = true,
                reconciled = null,
            ),
        )
    }

    @Test
    fun failedMutationWithoutCanonicalConfirmationIsOutcomeUnknown() {
        assertEquals(
            SandTrialCancellationOutcome.OUTCOME_UNKNOWN,
            classifySandTrialCancellationOutcome(
                cancelSucceeded = false,
                reconciled = activeTrial(),
            ),
        )
        assertEquals(
            SandTrialCancellationOutcome.OUTCOME_UNKNOWN,
            classifySandTrialCancellationOutcome(
                cancelSucceeded = false,
                reconciled = null,
            ),
        )
    }
}
