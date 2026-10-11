package com.ombhrum.fabushi.androidpreload.runtime

import androidx.activity.ComponentActivity
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink

/**
 * Process-scoped services that presentation is allowed to observe or signal.
 *
 * The concrete remote-device gateway and runtime lifecycle stay below this bridge. Renderer code
 * can publish its semantic surface and report account visibility without owning a transport.
 */
internal interface AndroidPresentationRuntimePort {
    val appAgentSurface: FabushiAppAgentSurface

    fun setLoggedIn(loggedIn: Boolean)

    fun setForeground(foreground: Boolean)

    fun launchExternalAuth(url: String): Boolean

    fun launchExternalUrl(url: String): Boolean

    fun handlePlatformDeepLink(link: AndroidDeepLink): Boolean

    fun attachInteractiveActivity(activity: ComponentActivity)

    fun detachInteractiveActivity(activity: ComponentActivity)
}
