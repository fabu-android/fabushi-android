package com.ombhrum.fabushi.androidmain.coordinator
data class CoordinatorMainLegs(val controlReady: Boolean, val dataReady: Boolean, val mainDataReady: Boolean) {
    val fullyReady: Boolean get() = controlReady && dataReady && mainDataReady
}
