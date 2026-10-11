package com.ombhrum.fabushi.androidpreload.runtime

/**
 * Trusted preload-owned resolver for the process Coordinator.
 *
 * android-main installs the process runtime once. Presentation can only obtain the typed
 * [AndroidCoordinatorPort] contract and cannot construct, replace, or close the Host/Coordinator.
 */
object AndroidCoordinatorBridge {
    @Volatile
    private var installed: AndroidCoordinatorPort? = null

    internal fun installTrustedRuntime(port: AndroidCoordinatorPort) {
        val current = installed
        require(current == null || current === port) {
            "coordinator_runtime_already_installed"
        }
        installed = port
    }

    fun presentation(): AndroidCoordinatorPort =
        checkNotNull(installed) { "coordinator_runtime_not_installed" }
}
