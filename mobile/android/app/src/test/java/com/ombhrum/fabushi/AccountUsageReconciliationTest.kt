package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidAccountUsageSummary
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAccountUpgradeCta
import org.junit.Assert.assertEquals
import org.junit.Test

class AccountUsageReconciliationTest {
    private fun upgradeCta(
        disabled: Boolean = false,
        kind: String = "dashboard-action",
        action: String? = "requestLimitIncrease",
    ) = AndroidAccountUpgradeCta(
        label = "Upgrade",
        disabled = disabled,
        kind = kind,
        dashboardAction = action,
    )

    @Test
    fun upgradeCtaPolicyAllowsOnlySettledAccountExactDashboardAction() {
        assertEquals(
            true,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "account-a",
                accountUsageBusy = false,
                cta = upgradeCta(),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = false,
                accountAuthId = "account-a",
                accountUsageBusy = false,
                cta = upgradeCta(),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "",
                accountUsageBusy = false,
                cta = upgradeCta(),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "account-a",
                accountUsageBusy = true,
                cta = upgradeCta(),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "account-a",
                accountUsageBusy = false,
                cta = upgradeCta(disabled = true),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "account-a",
                accountUsageBusy = false,
                cta = upgradeCta(kind = "open-url", action = null),
            ),
        )
        assertEquals(
            false,
            shouldPerformAccountUpgradeCta(
                loggedIn = true,
                accountAuthId = "account-a",
                accountUsageBusy = false,
                cta = upgradeCta(action = "deleteEverything"),
            ),
        )
    }

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
    fun currentAccountGenerationAcceptsUsageProjection() {
        assertEquals(
            true,
            shouldApplyAccountUsageResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-a",
            ),
        )
    }

    @Test
    fun staleGenerationOrAccountSwitchRejectsUsageProjection() {
        assertEquals(
            false,
            shouldApplyAccountUsageResult(
                requestGeneration = 6,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-a",
            ),
        )
        assertEquals(
            false,
            shouldApplyAccountUsageResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-b",
            ),
        )
        assertEquals(
            false,
            shouldApplyAccountUsageResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = false,
                currentAuthId = "account-a",
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
