package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort

/**
 * Production renderer client for Coordinator requests.
 *
 * The renderer knows only the preload contract. Concrete android-main/Host types are intentionally
 * unreachable from this layer.
 */
internal object CoordinatorClient {
    fun presentation(): AndroidCoordinatorPort = AndroidCoordinatorBridge.presentation()
}
