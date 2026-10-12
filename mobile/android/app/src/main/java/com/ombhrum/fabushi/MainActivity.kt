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
                keyboardShortcutController = globalKeyboardShortcutController,
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

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action != KeyEvent.ACTION_DOWN) return super.dispatchKeyEvent(event)
        val childHandled = super.dispatchKeyEvent(event)
        val shortcutHandled = globalKeyboardShortcutController.dispatch(
            GlobalShortcutKeyEvent(
                key = shortcutKeyName(event),
                ctrl = event.isCtrlPressed,
                meta = event.isMetaPressed,
                alt = event.isAltPressed,
                shift = event.isShiftPressed,
                defaultPrevented = childHandled,
                repeat = event.repeatCount > 0,
            ),
        )
        return childHandled || shortcutHandled
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::enqueueDeepLink)
    }

    override fun onDestroy() {
        deepLinkController.markNotReady()
        globalKeyboardShortcutController.clearPresentationBinding()
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
