package com.ombhrum.fabushi.androidpreload
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
data class PrimaryPreloadInitialState(val appVersion: String, val packaged: Boolean, val processGeneration: Long)
class DesktopPreloadBridge(val coordinator: AndroidCoordinatorPort, val initialState: PrimaryPreloadInitialState)
