package com.ombhrum.fabushi.androidmain.adapters

import android.net.Uri
import java.net.URI
import androidx.activity.ComponentActivity
import androidx.browser.customtabs.CustomTabsIntent

internal class AndroidAccountOAuthAdapter {
    fun openExternalAuth(
        activity: ComponentActivity?,
        rawUrl: String,
    ): Boolean {
        val target = validateExternalAuthUrl(rawUrl) ?: return false
        val interactiveActivity = activity ?: return false
        CustomTabsIntent.Builder()
            .setShowTitle(true)
            .build()
            .launchUrl(interactiveActivity, Uri.parse(target.toASCIIString()))
        return true
    }

    companion object {
        internal fun validateExternalAuthUrl(rawUrl: String): URI? =
            AndroidExternalUrlPolicy.parseServerAcceptedAuthExternalUrl(
                rawUrl = rawUrl,
                expectedOrigin = FABUSHI_AUTH_ORIGIN,
            )

        private const val FABUSHI_AUTH_ORIGIN = "https://api.ombhrum.com"
    }
}
