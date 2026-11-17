package com.ombhrum.fabushi

import android.content.Context
import android.util.Log
import com.ombhrum.fabushi.mika.TelegramMikaChannelBridge
import com.wangzan101.mika.MikaApp

/**
 * Unified Application container for Fabushi Telegram + Mika Phone Control Agent (`com.ombhrum.fabushi`).
 *
 * Subclasses [MikaApp] so every Mika phone-control capability (`MikaAccessibilityService`,
 * `UID 2000` Wireless ADB `IMikaShell` helper, `VirtualDisplay`, Floating Capsule, `TeachRuntime`,
 * `PageKeeper` headless CDP browser, Alpine `proot` Linux sandbox, `MikaInCallService`,
 * `NoticeListenerService`, and `TriggerRuntime` schedules) shares the exact process-wide
 * singletons alongside `MahayanaHost` and `TelegramAndroidShell`.
 */
class FabushiApplication : MikaApp() {
    val telegramMikaBridge: TelegramMikaChannelBridge by lazy {
        TelegramMikaChannelBridge(this)
    }

    override fun onCreate() {
        super.onCreate()
        runCatching {
            telegramMikaBridge.startObserving()
        }.onFailure { err ->
            Log.w("FabushiApplication", "Failed to start Telegram-Mika bridge observer", err)
        }
    }

    companion object {
        fun bridgeOrNull(context: Context): TelegramMikaChannelBridge? =
            (context.applicationContext as? FabushiApplication)?.telegramMikaBridge
    }
}
