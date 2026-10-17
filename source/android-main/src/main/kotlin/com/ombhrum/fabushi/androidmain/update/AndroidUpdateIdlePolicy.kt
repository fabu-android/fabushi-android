package com.ombhrum.fabushi

data class AndroidUpdateIdleInputs(
    val optInEnabled: Boolean,
    val gateObserved: Boolean,
    val gateEnabled: Boolean,
    val signedTransportAvailable: Boolean,
    val userAwayForThreshold: Boolean,
    val hostConfirmedIdle: Boolean,
    val updateOperationBusy: Boolean,
)

object AndroidUpdateIdlePolicy {
    fun canAutoStage(input: AndroidUpdateIdleInputs): Boolean =
        input.optInEnabled &&
            input.gateObserved &&
            input.gateEnabled &&
            input.signedTransportAvailable &&
            input.userAwayForThreshold &&
            input.hostConfirmedIdle &&
            !input.updateOperationBusy
}
