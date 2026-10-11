package com.ombhrum.fabushi

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import com.ombhrum.fabushi.androidmain.deeplink.AndroidDeepLinkController
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidPresentationDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import kotlinx.coroutines.flow.MutableSharedFlow

/**
 * Android lifecycle shell.
 *
 * Product rendering lives under the frontend source tree. Runtime/domain orchestration is intentionally kept
 * out of the Activity so process recreation and renderer replacement can be tested independently.
 */
class MainActivity : ComponentActivity() {
    private val deepLinks = MutableSharedFlow<AndroidPresentationDeepLink>(replay = 1, extraBufferCapacity = 31)
    private lateinit var runtimePort: AndroidPresentationRuntimePort
    private val deepLinkController = AndroidDeepLinkController(
        dispatch = ::dispatchDeepLink,
    )
    private val updateModel: AndroidUpdateViewModel by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runtimePort = (application as FabushiApplication).ensurePresentationRuntime(intent)
        enableEdgeToEdge()
        setContent {
            FabushiApplicationRoot(
                activity = this,
                deepLinks = deepLinks,
                updateModel = updateModel,
                runtimePort = runtimePort,
            )
        }
        deepLinkController.markReady()
        intent?.data?.let(::enqueueDeepLink)
    }

    override fun onStart() {
        super.onStart()
        (application as FabushiApplication).requirePresentationRuntime().apply {
            attachInteractiveActivity(this@MainActivity)
            setForeground(true)
        }
        updateModel.setForeground(true)
        ensureNotificationPermission()
    }

    override fun onStop() {
        (application as FabushiApplication).requirePresentationRuntime().apply {
            setForeground(false)
            detachInteractiveActivity(this@MainActivity)
        }
        updateModel.setForeground(false)
        super.onStop()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::enqueueDeepLink)
    }

    override fun onDestroy() {
        deepLinkController.markNotReady()
        super.onDestroy()
    }

    internal fun appAgentSurfaceForTesting(): FabushiAppAgentSurface =
        (application as FabushiApplication).requirePresentationRuntime().appAgentSurface

    private fun dispatchDeepLink(link: AndroidDeepLink) {
        if (runtimePort.handlePlatformDeepLink(link)) return
        if (link is AndroidPresentationDeepLink) {
            deepLinks.tryEmit(link)
        }
    }

    private fun ensureNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val preferences = getSharedPreferences("fabushi.mobile", MODE_PRIVATE)
        if (preferences.getBoolean("notification-permission-requested", false)) return
        preferences.edit().putBoolean("notification-permission-requested", true).apply()
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4107)
    }

    private fun enqueueDeepLink(uri: android.net.Uri) {
        deepLinkController.handleCandidate(uri.toString(), "android-intent")
    }
}
