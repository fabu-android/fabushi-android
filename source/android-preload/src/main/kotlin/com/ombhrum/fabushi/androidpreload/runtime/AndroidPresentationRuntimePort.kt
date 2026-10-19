package com.ombhrum.fabushi.androidpreload.runtime

import androidx.activity.ComponentActivity
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.media.AndroidHumanCallMediaPermissionProjection
import org.json.JSONArray
import org.json.JSONObject

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

    fun rootNotificationTrays(): JSONArray

    fun dismissRootNotificationTray(id: String): Boolean

    fun clearRootNotificationTrays(): Boolean

    fun invokeRootNotificationDashboardAction(action: String, args: JSONObject): JSONObject

    fun addRootNotificationEventListener(listener: (JSONObject) -> Unit): AutoCloseable

    fun handlePlatformDeepLink(link: AndroidDeepLink): Boolean

    fun requestHumanCallMediaPermissions(
        audio: Boolean,
        video: Boolean,
        listener: (AndroidHumanCallMediaPermissionProjection) -> Unit,
    ): Boolean

    fun handleHumanCallMediaPermissionResult(
        activity: ComponentActivity,
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ): Boolean

    fun attachInteractiveActivity(activity: ComponentActivity)

    fun detachInteractiveActivity(activity: ComponentActivity)
}
