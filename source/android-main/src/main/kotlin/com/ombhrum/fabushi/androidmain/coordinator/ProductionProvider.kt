package com.ombhrum.fabushi.androidmain.coordinator
interface ProductionCoordinatorProvider {
    val account: CoordinatorAccountRuntime
    val control: CoordinatorControlServer
    val executors: CoordinatorExecutors
    val resync: CoordinatorResyncTracker
    val process: CoordinatorProcessRuntime
}
data class ProductionCoordinatorPorts(
    override val account: CoordinatorAccountRuntime,
    override val control: CoordinatorControlServer,
    override val executors: CoordinatorExecutors,
    override val resync: CoordinatorResyncTracker,
    override val process: CoordinatorProcessRuntime,
) : ProductionCoordinatorProvider
