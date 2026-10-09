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
        internal fun validateExternalAuthUrl(rawUrl: String): URI? {
            if (rawUrl.length !in 1..MAX_URL_LENGTH) return null
            val uri = runCatching { URI(rawUrl) }.getOrNull() ?: return null
            if (!uri.scheme.equals("https", ignoreCase = true)) return null
            if (uri.host.isNullOrBlank()) return null
            if (uri.userInfo != null) return null
            if (uri.fragment != null) return null
            if (!uri.host.equals(FABUSHI_AUTH_HOST, ignoreCase = true)) return null
            return uri
        }

        private const val MAX_URL_LENGTH = 8_192
        private const val FABUSHI_AUTH_HOST = "api.ombhrum.com"
    }
}
