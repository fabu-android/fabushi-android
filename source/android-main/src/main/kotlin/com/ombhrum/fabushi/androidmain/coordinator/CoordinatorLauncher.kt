package com.ombhrum.fabushi.androidmain.coordinator
data class CoordinatorLaunchHandle(val generation: Long, val startedAtMs: Long, val ready: Boolean)
class CoordinatorLauncher {
    fun launch(generation: Long, nowMs: Long): CoordinatorLaunchHandle {
        require(generation > 0)
        return CoordinatorLaunchHandle(generation, nowMs, true)
    }
}
