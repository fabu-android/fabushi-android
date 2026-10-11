package com.ombhrum.fabushi.androidpreload
class ViewerVisibilityGate {
    private var appVisible = true
    private var viewerRequested = false
    fun setAppVisible(value: Boolean) { appVisible = value }
    fun setViewerRequested(value: Boolean) { viewerRequested = value }
    fun isViewerVisible(): Boolean = appVisible && viewerRequested
}
