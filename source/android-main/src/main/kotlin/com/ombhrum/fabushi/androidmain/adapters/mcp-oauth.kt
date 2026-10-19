package com.ombhrum.fabushi.androidmain.adapters

import androidx.activity.ComponentActivity
import com.ombhrum.fabushi.androidmain.mcp.AndroidMcpOAuthLoopbackProvider
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidMcpOAuthCompletion

/**
 * Android platform-owned MCP OAuth adapter.
 *
 * Coordinator owns state registration/single-use completion. This adapter owns only the external
 * browser and app-link transport, replacing Grok's localhost HTTP listener on Android.
 */
internal class AndroidMcpOAuthAdapter(
    coordinator: AndroidCoordinatorPort,
    private val browser: AndroidExternalUrlAdapter = AndroidExternalUrlAdapter(),
) {
    private val callbacks = AndroidMcpOAuthLoopbackProvider(coordinator)

    fun beginAuthorization(
        activity: ComponentActivity?,
        authorizationUrl: String,
        provider: String,
        serverId: String? = null,
        accountKey: String? = null,
        generation: Long? = null,
    ): Boolean {
        val pending = callbacks.registerAuthorization(
            authorizationUrl = authorizationUrl,
            provider = provider,
            serverId = serverId,
            accountKey = accountKey,
            generation = generation,
        ) ?: return false
        if (browser.openExternalAuth(activity, pending.authorizationUrl)) {
            return true
        }
        runCatching {
            callbacks.failRegisteredAuthorization(
                state = pending.state,
                reason = "android_browser_unavailable",
            )
        }
        return false
    }

    fun handleCallback(
        callback: AndroidDeepLink.McpOAuthCallback,
    ): AndroidMcpOAuthCompletion =
        callbacks.complete(callback)
}
