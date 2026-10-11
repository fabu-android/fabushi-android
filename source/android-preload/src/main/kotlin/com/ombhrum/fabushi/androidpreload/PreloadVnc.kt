package com.ombhrum.fabushi.androidpreload
data class VncEdgeState(val connected: Boolean, val viewerVisible: Boolean, val clipboardEnabled: Boolean, val lastFrameAtMs: Long?)
class PreloadVnc {
    private val gate = ViewerVisibilityGate()
    fun project(connected: Boolean, requested: Boolean, lastFrameAtMs: Long?): VncEdgeState {
        gate.setViewerRequested(requested)
        return VncEdgeState(connected, gate.isViewerVisible(), connected, lastFrameAtMs)
    }
    fun setAppVisible(visible: Boolean) = gate.setAppVisible(visible)
}
