package com.ombhrum.fabushi

import android.app.Application
import android.content.Intent
import androidx.activity.ComponentActivity
import com.ombhrum.fabushi.androidmain.adapters.AndroidAccountOAuthAdapter
import com.ombhrum.fabushi.androidmain.adapters.AndroidExternalUrlAdapter
import com.ombhrum.fabushi.androidmain.adapters.AndroidMcpOAuthAdapter
import java.lang.ref.WeakReference
import java.util.concurrent.Executors
import com.ombhrum.fabushi.androidmain.coordinator.AndroidCoordinatorPorts
import com.ombhrum.fabushi.androidmain.coordinator.RootShellFocusRefreshOwner
import com.ombhrum.fabushi.androidmain.coordinator.RootShellFocusRefreshToken
import com.ombhrum.fabushi.androidmain.diagnostics.AndroidProcessMetricsRuntime
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
    private val rootShellFocusRefreshOwner = RootShellFocusRefreshOwner()
    private val rootShellFocusRefreshExecutor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-root-focus-refresh").apply { isDaemon = true }
    }
    private val rootShellFocusRefreshSubscription =
        coordinator.addRootConnectionResilienceListener { projection ->
            rootShellFocusRefreshOwner.observeConnection(projection)
                ?.let(::dispatchRootShellFocusRefresh)
        }
    private val accountOAuth = AndroidAccountOAuthAdapter()
    private val externalUrl = AndroidExternalUrlAdapter()
    private val mcpOAuth = AndroidMcpOAuthAdapter(coordinator)
    override val appAgentSurface = FabushiAppAgentSurface()
    private val notificationRuntime = AndroidNotificationRuntime(
        context = application,
        isAppForeground = { appForeground },
    ).also { runtime ->
        // Desktop seeds notification/badge state from the canonical roster before applying
        // incremental agent-upserted events. Do the same at process startup so a cold-start delta
        // cannot remain buffered indefinitely while waiting for a later full-roster event.
        runCatching { coordinator.agentList() }
            .onSuccess { agents ->
                runtime.feed.handleFeatureEvent(
                    org.json.JSONObject()
                        .put("kind", "agents-roster-seed")
                        .put("agents", org.json.JSONArray(agents.toString())),
                )
            }
    }
    private val webAuthnRuntime = AndroidCredentialManagerWebAuthn(
        application = application,
        coordinator = coordinator,
    )
    private val notificationEventSubscription =
        coordinator.addFeatureEventListener(::handleNotificationFeatureEvent)
    private val mcpOAuthEventSubscription =
        coordinator.addFeatureEventListener(::handleRuntimeFeatureEvent)
    private val rootNotificationListeners =
        java.util.concurrent.CopyOnWriteArrayList<(org.json.JSONObject) -> Unit>()
    private val rootNotificationEventSubscription =
        coordinator.addFeatureEventListener { event ->
            if (event.optString("type").startsWith("tray.")) {
                rootNotificationListeners.forEach { listener ->
                    runCatching { listener(org.json.JSONObject(event.toString())) }
                }
            }
        }
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

    private fun handleNotificationFeatureEvent(event: org.json.JSONObject) {
        val eventPayload = event.optJSONObject("event") ?: event
        val kind = event.optString("kind").ifBlank {
            eventPayload.optString("kind").ifBlank { eventPayload.optString("type") }
        }
        if (kind == "agents" && eventPayload.optJSONArray("agents") == null) {
            runCatching { coordinator.agentList() }
                .onSuccess { agents ->
                    notificationRuntime.feed.handleFeatureEvent(
                        org.json.JSONObject()
                            .put("kind", "agents")
                            .put("agents", org.json.JSONArray(agents.toString())),
                    )
                }
            return
        }
        notificationRuntime.feed.handleFeatureEvent(event)
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
        rootShellFocusRefreshOwner.observeVisibility(foreground)
            ?.let(::dispatchRootShellFocusRefresh)
    }

    private fun dispatchRootShellFocusRefresh(token: RootShellFocusRefreshToken) {
        rootShellFocusRefreshExecutor.execute {
            runCatching { coordinator.accountAccessProjection() }
            rootShellFocusRefreshOwner.complete(token)
        }
    }

    override fun launchExternalAuth(url: String): Boolean =
        accountOAuth.openExternalAuth(interactiveActivityOrNull(), url)

    override fun launchExternalUrl(url: String): Boolean =
        externalUrl.openExternalAuth(interactiveActivityOrNull(), url)

    override fun rootNotificationTrays(): org.json.JSONArray =
        coordinator.rootNotificationTrays()

    override fun dismissRootNotificationTray(id: String): Boolean =
        coordinator.dismissRootNotificationTray(id)

    override fun clearRootNotificationTrays(): Boolean =
        coordinator.clearRootNotificationTrays()

    override fun invokeRootNotificationDashboardAction(
        action: String,
        args: org.json.JSONObject,
    ): org.json.JSONObject =
        coordinator.invokeRootNotificationDashboardAction(action, args)

    override fun addRootNotificationEventListener(
        listener: (org.json.JSONObject) -> Unit,
    ): AutoCloseable {
        rootNotificationListeners += listener
        return AutoCloseable { rootNotificationListeners.remove(listener) }
    }

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
        rootShellFocusRefreshSubscription.close()
        rootShellFocusRefreshOwner.dispose()
        rootShellFocusRefreshExecutor.shutdownNow()
        webAuthnRuntime.close()
        rootNotificationEventSubscription.close()
        rootNotificationListeners.clear()
        mcpOAuthEventSubscription.close()
        notificationEventSubscription.close()
        notificationRuntime.reset()
        remoteDeviceGateway.close()
        AndroidCoordinatorBridge.clearTrustedRuntime(coordinator)
        AndroidCoordinatorPorts.closeProcessRuntime()
    }
}
