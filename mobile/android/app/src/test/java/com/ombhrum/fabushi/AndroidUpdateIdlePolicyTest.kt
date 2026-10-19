package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateIdlePolicyTest {
    private fun eligible() = AndroidUpdateIdleInputs(
        optInEnabled = true,
        gateObserved = true,
        gateEnabled = true,
        signedTransportAvailable = true,
        userAwayForThreshold = true,
        hostConfirmedIdle = true,
        updateOperationBusy = false,
    )

    @Test fun allSafetyInputsAreRequired() {
        assertTrue(AndroidUpdateIdlePolicy.canAutoStage(eligible()))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(optInEnabled = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(gateObserved = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(gateEnabled = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(signedTransportAvailable = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(userAwayForThreshold = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(hostConfirmedIdle = false)))
        assertFalse(AndroidUpdateIdlePolicy.canAutoStage(eligible().copy(updateOperationBusy = true)))
    }
}
