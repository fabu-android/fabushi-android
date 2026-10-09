package com.ombhrum.fabushi

import android.app.Application
import android.content.Intent
import androidx.activity.ComponentActivity
import com.ombhrum.fabushi.androidmain.adapters.AndroidAccountOAuthAdapter
import com.ombhrum.fabushi.androidmain.adapters.AndroidMcpOAuthAdapter
import java.lang.ref.WeakReference
import com.ombhrum.fabushi.androidmain.coordinator.AndroidCoordinatorPorts
import com.ombhrum.fabushi.androidmain.notifications.AndroidNotificationRuntime
import com.ombhrum.fabushi.androidmain.webauthn.AndroidCredentialManagerWebAuthn
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort

/**
 * Process owner for Android runtime services that must survive Activity recreation.
 *
 * The Activity only supplies Android launch metadata. Coordinator/Host ownership remains below the
 * typed bridge and the remote-device transport is process-scoped rather than tied to a screen.
 */
class FabushiApplication : Application() {
    @Volatile
    private var processRuntime: FabushiProcessRuntime? = null

    internal fun ensurePresentationRuntime(intent: Intent?): AndroidPresentationRuntimePort =
        processRuntime ?: synchronized(this) {
            processRuntime ?: run {
                val ciBootstrapActive = FabushiCiBootstrap.prepare(this)
                FabushiProcessRuntime(
                    application = this,
                    intent = intent,
                    ciBootstrapActive = ciBootstrapActive,
                ).also { processRuntime = it }
            }
        }

    internal fun requirePresentationRuntime(): AndroidPresentationRuntimePort =
        checkNotNull(processRuntime) { "Fabushi process runtime has not been initialized" }

    override fun onTerminate() {
        processRuntime?.close()
        processRuntime = null
        super.onTerminate()
    }
}

internal class FabushiProcessRuntime(
    application: Application,
    intent: Intent?,
    ciBootstrapActive: Boolean,
) : AndroidPresentationRuntimePort, AutoCloseable {
    @Volatile
    private var appForeground = false
    @Volatile
    private var interactiveActivityRef: WeakReference<ComponentActivity>? = null
    private val coordinator = AndroidCoordinatorPorts.presentation(application).also {
        AndroidCoordinatorBridge.installTrustedRuntime(it)
    }
    private val accountOAuth = AndroidAccountOAuthAdapter()
    private val mcpOAuth = AndroidMcpOAuthAdapter(coordinator)
    override val appAgentSurface = FabushiAppAgentSurface()
    private val notificationRuntime = AndroidNotificationRuntime(
        context = application,
        isAppForeground = { appForeground },
    )
    private val webAuthnRuntime = AndroidCredentialManagerWebAuthn(
        application = application,
        coordinator = coordinator,
    )
    private val notificationEventSubscription =
        coordinator.addFeatureEventListener(notificationRuntime.feed::handleFeatureEvent)
    private val mcpOAuthEventSubscription =
        coordinator.addFeatureEventListener(::handleRuntimeFeatureEvent)
    private val remoteDeviceGateway = FabushiRemoteDeviceGateway(
        context = application,
        coordinator = coordinator,
        surface = appAgentSurface,
        metadata = FabushiCiBootstrap.gatewayMetadata(intent, ciBootstrapActive),
        configuredDeviceName = FabushiCiBootstrap.configuredDeviceName(intent, ciBootstrapActive),
    )

    override fun handlePlatformDeepLink(link: AndroidDeepLink): Boolean =
        when (link) {
            is AndroidDeepLink.McpOAuthCallback -> {
                runCatching { mcpOAuth.handleCallback(link) }
                true
            }
            else -> false
        }

    private fun handleRuntimeFeatureEvent(event: org.json.JSONObject) {
        if (event.optString("type") != "mcp.authorization.required") return
        val authorizationUrl = event.optString("authorizationUrl").trim()
        val provider = event.optString("provider").trim()
        if (authorizationUrl.isBlank() || provider.isBlank()) return
        val serverId = event.optString("serverId").trim().takeIf(String::isNotBlank)
        val accountKey = event.optString("accountKey").trim().takeIf(String::isNotBlank)
        val generation = event.optLong("generation", 0L).takeIf { it > 0L }
        mcpOAuth.beginAuthorization(
            activity = interactiveActivityOrNull(),
            authorizationUrl = authorizationUrl,
            provider = provider,
            serverId = serverId,
            accountKey = accountKey,
            generation = generation,
        )
    }

    override fun setLoggedIn(loggedIn: Boolean) {
        remoteDeviceGateway.setLoggedIn(loggedIn)
        if (!loggedIn) notificationRuntime.reset()
    }

    override fun setForeground(foreground: Boolean) {
        appForeground = foreground
    }

    override fun launchExternalAuth(url: String): Boolean =
        accountOAuth.openExternalAuth(interactiveActivityOrNull(), url)

    override fun attachInteractiveActivity(activity: ComponentActivity) {
        interactiveActivityRef = WeakReference(activity)
        webAuthnRuntime.attach(activity)
    }

    override fun detachInteractiveActivity(activity: ComponentActivity) {
        if (interactiveActivityRef?.get() === activity) {
            interactiveActivityRef = null
            webAuthnRuntime.detach(activity)
        }
    }

    internal fun interactiveActivityOrNull(): ComponentActivity? =
        interactiveActivityRef?.get()

    override fun close() {
        interactiveActivityRef = null
        webAuthnRuntime.close()
        mcpOAuthEventSubscription.close()
        notificationEventSubscription.close()
        notificationRuntime.reset()
        remoteDeviceGateway.close()
    }
}
