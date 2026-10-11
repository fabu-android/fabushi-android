package com.ombhrum.fabushi.androidmain.coordinator
data class ProductionCoordinatorAuxiliaryPorts(
    val connectivity: AndroidConnectivity,
    val telemetry: CoordinatorHandoffTelemetry,
    val launcher: CoordinatorLauncher,
)
