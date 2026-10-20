package com.ombhrum.fabushi.androidmain.coordinator
data class ProductionCoordinatorRoot(
    val primary: ProductionCoordinatorPorts,
    val auxiliary: ProductionCoordinatorAuxiliaryPorts,
) {
    fun isReady(): Boolean = primary.process.state is CoordinatorProcessState.Running
}
