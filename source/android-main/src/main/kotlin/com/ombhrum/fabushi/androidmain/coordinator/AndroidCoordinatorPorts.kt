package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import com.ombhrum.fabushi.androidpreload.CoordinatorPortBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort

/**
 * Presentation-facing resolver for the process-scoped Coordinator port.
 *
 * Presentation code receives only the trusted bridge contract. Concrete Host/runtime ownership
 * stays inside android-main so Activity/ViewModel code cannot construct or close the Host.
 */
object AndroidCoordinatorPorts {
    private const val PRESENTATION_OWNER = "android-presentation-boundary"

    @Volatile
    private var presentationBridge: CoordinatorPortBridge? = null

    /**
     * One process-scoped presentation owner mirrors Desktop's single renderer-owned Coordinator
     * port without introducing another runtime or state owner. Screens/ViewModels may share this
     * typed capability, but no second presentation boundary can claim the same port.
     */
    fun presentation(application: Application): AndroidCoordinatorPort {
        val existing = presentationBridge
        if (existing != null) return existing.claim(PRESENTATION_OWNER)
        return synchronized(this) {
            val bridge = presentationBridge ?: CoordinatorPortBridge(
                AndroidCoordinatorRuntime.get(application),
            ).also { presentationBridge = it }
            bridge.claim(PRESENTATION_OWNER)
        }
    }

    /**
     * Process-owner shutdown only. Screens/ViewModels intentionally never receive this capability.
     */
    fun closeProcessRuntime() {
        synchronized(this) {
            presentationBridge?.release(PRESENTATION_OWNER)
            presentationBridge = null
            AndroidCoordinatorRuntime.closeProcessRuntime()
        }
    }
}
