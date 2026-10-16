package com.ombhrum.fabushi

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
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
    internal val globalKeyboardShortcutController = GlobalKeyboardShortcutController()
    private val deepLinks = MutableSharedFlow<AndroidPresentationDeepLink>(replay = 1, extraBufferCapacity = 31)
    private lateinit var runtimePort: AndroidPresentationRuntimePort
    private var runtimeReady = false
    private var activityStarted = false
    private var startupFailure by mutableStateOf<FabushiRootFailure?>(null)
    private val deepLinkController = AndroidDeepLinkController(
        dispatch = ::dispatchDeepLink,
    )
    private val updateModel: AndroidUpdateViewModel by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        initializePresentationRuntime()
        enableEdgeToEdge()
        setContent {
            val failure = startupFailure
            when {
                failure != null -> FabushiRootErrorSurface(failure = failure, onRetry = ::retryPresentationRuntime)
                runtimeReady -> FabushiApplicationRoot(
                    activity = this,
                    keyboardShortcutController = globalKeyboardShortcutController,
                    deepLinks = deepLinks,
                    updateModel = updateModel,
                    runtimePort = runtimePort,
                )
            }
        }
        if (runtimeReady) markPresentationRuntimeReady()
    }

    override fun onStart() {
        super.onStart()
        activityStarted = true
        if (runtimeReady) {
            runtimePort.attachInteractiveActivity(this@MainActivity)
            runtimePort.setForeground(true)
            updateModel.setForeground(true)
            ensureNotificationPermission()
        }
    }

    override fun onStop() {
        activityStarted = false
        if (runtimeReady) {
            runtimePort.setForeground(false)
            runtimePort.detachInteractiveActivity(this)
            updateModel.setForeground(false)
        }
        super.onStop()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // Activity.onKeyDown is reached only after the Window/focused child had the first
        // opportunity to consume this key event. Keep that framework child-first ordering
        // instead of calling ComponentActivity.dispatchKeyEvent(), which is a restricted API.
        val shortcutHandled = globalKeyboardShortcutController.dispatch(
            GlobalShortcutKeyEvent(
                key = shortcutKeyName(event),
                ctrl = event.isCtrlPressed,
                meta = event.isMetaPressed,
                alt = event.isAltPressed,
                shift = event.isShiftPressed,
                defaultPrevented = false,
                repeat = event.repeatCount > 0,
            ),
        )
        return shortcutHandled || super.onKeyDown(keyCode, event)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::enqueueDeepLink)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (runtimeReady) {
            runtimePort.handleHumanCallMediaPermissionResult(
                activity = this,
                requestCode = requestCode,
                permissions = permissions,
                grantResults = grantResults,
            )
        }
    }

    override fun onDestroy() {
        deepLinkController.markNotReady()
        globalKeyboardShortcutController.clearPresentationBinding()
        super.onDestroy()
    }

    internal fun appAgentSurfaceForTesting(): FabushiAppAgentSurface =
        (application as FabushiApplication).requirePresentationRuntime().appAgentSurface

    private fun dispatchDeepLink(link: AndroidDeepLink) {
        if (runtimeReady && runtimePort.handlePlatformDeepLink(link)) return
        if (link is AndroidPresentationDeepLink) {
            deepLinks.tryEmit(link)
        }
    }


    private fun initializePresentationRuntime(): Boolean =
        runCatching { (application as FabushiApplication).ensurePresentationRuntime(intent) }
            .fold(
                onSuccess = { runtime ->
                    runtimePort = runtime
                    runtimeReady = true
                    startupFailure = null
                    true
                },
                onFailure = { error ->
                    runtimeReady = false
                    deepLinkController.markNotReady()
                    startupFailure = projectFabushiRootFailure(error)
                    false
                },
            )

    private fun retryPresentationRuntime() {
        if (!initializePresentationRuntime()) return
        markPresentationRuntimeReady()
        if (activityStarted) {
            runtimePort.attachInteractiveActivity(this)
            runtimePort.setForeground(true)
            updateModel.setForeground(true)
            ensureNotificationPermission()
        }
    }

    private fun markPresentationRuntimeReady() {
        deepLinkController.markReady()
        intent?.data?.let(::enqueueDeepLink)
    }

    private fun ensureNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val preferences = getSharedPreferences("fabushi.mobile", MODE_PRIVATE)
        if (preferences.getBoolean("notification-permission-requested", false)) return
        preferences.edit().putBoolean("notification-permission-requested", true).apply()
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4107)
    }

    private fun shortcutKeyName(event: KeyEvent): String = when (event.keyCode) {
        KeyEvent.KEYCODE_ESCAPE -> "escape"
        KeyEvent.KEYCODE_DPAD_UP -> "up"
        KeyEvent.KEYCODE_DPAD_DOWN -> "down"
        KeyEvent.KEYCODE_LEFT_BRACKET -> "bracketleft"
        KeyEvent.KEYCODE_RIGHT_BRACKET -> "bracketright"
        KeyEvent.KEYCODE_COMMA -> "comma"
        KeyEvent.KEYCODE_N -> "n"
        KeyEvent.KEYCODE_K -> "k"
        KeyEvent.KEYCODE_M -> "m"
        KeyEvent.KEYCODE_I -> "i"
        KeyEvent.KEYCODE_L -> "l"
        KeyEvent.KEYCODE_F -> "f"
        KeyEvent.KEYCODE_B -> "b"
        in KeyEvent.KEYCODE_1..KeyEvent.KEYCODE_9 ->
            (event.keyCode - KeyEvent.KEYCODE_0).toString()
        else -> KeyEvent.keyCodeToString(event.keyCode).removePrefix("KEYCODE_").lowercase()
    }

    private fun enqueueDeepLink(uri: android.net.Uri) {
        deepLinkController.handleCandidate(uri.toString(), "android-intent")
    }
}
