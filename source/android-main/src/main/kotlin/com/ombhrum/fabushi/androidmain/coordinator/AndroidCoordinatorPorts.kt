package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort

/**
 * Presentation-facing resolver for the process-scoped Coordinator port.
 *
 * Presentation code receives only the trusted bridge contract. Concrete Host/runtime ownership
 * stays inside android-main so Activity/ViewModel code cannot construct or close the Host.
 */
object AndroidCoordinatorPorts {
    fun presentation(application: Application): AndroidCoordinatorPort =
        AndroidCoordinatorRuntime.get(application)
}
