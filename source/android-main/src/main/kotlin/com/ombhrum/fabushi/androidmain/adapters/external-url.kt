package com.ombhrum.fabushi.androidmain.adapters

import android.net.Uri
import androidx.activity.ComponentActivity
import androidx.browser.customtabs.CustomTabsIntent
import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

/**
 * Browser boundary for untrusted external HTTPS URLs.
 *
 * Unlike [AndroidAccountOAuthAdapter], this adapter does not grant first-party account-auth
 * privilege. It accepts arbitrary HTTPS origins only after removing foreign web-login token
 * parameters equivalent to Desktop's external-url policy.
 */
internal class AndroidExternalUrlAdapter {
    fun openExternalAuth(
        activity: ComponentActivity?,
        rawUrl: String,
    ): Boolean {
        val target = sanitizeExternalHttpsUrl(rawUrl) ?: return false
        val interactiveActivity = activity ?: return false
        CustomTabsIntent.Builder()
            .setShowTitle(true)
            .build()
            .launchUrl(interactiveActivity, Uri.parse(target.toASCIIString()))
        return true
    }

    companion object {
        private const val MAX_URL_LENGTH = 16_384

        internal fun sanitizeExternalHttpsUrl(rawUrl: String): URI? {
            if (rawUrl.length !in 1..MAX_URL_LENGTH) return null
            val uri = runCatching { URI(rawUrl) }.getOrNull() ?: return null
            if (!uri.scheme.equals("https", ignoreCase = true)) return null
            if (uri.host.isNullOrBlank() || uri.userInfo != null) return null

            val query = sanitizeParameterList(uri.rawQuery.orEmpty())
            val fragment = sanitizeFragment(uri.rawFragment.orEmpty())
            if (!query.changed && !fragment.changed) return uri

            val ascii = uri.toASCIIString()
            val fragmentIndex = ascii.indexOf('#')
            val beforeFragment = if (fragmentIndex >= 0) ascii.substring(0, fragmentIndex) else ascii
            val queryIndex = beforeFragment.indexOf('?')
            val base = if (queryIndex >= 0) beforeFragment.substring(0, queryIndex) else beforeFragment
            val rebuilt = buildString {
                append(base)
                if (query.value.isNotEmpty()) {
                    append('?')
                    append(query.value)
                }
                if (fragment.value.isNotEmpty()) {
                    append('#')
                    append(fragment.value)
                }
            }
            return runCatching { URI(rebuilt) }.getOrNull()
        }

        private data class SanitizedPart(
            val value: String,
            val changed: Boolean,
        )

        private fun sanitizeFragment(raw: String): SanitizedPart {
            if (raw.isEmpty()) return SanitizedPart(raw, false)
            val question = raw.indexOf('?')
            if (question < 0) return sanitizeParameterList(raw)
            val route = sanitizeParameterList(raw.substring(0, question))
            val params = sanitizeParameterList(raw.substring(question + 1))
            if (!route.changed && !params.changed) return SanitizedPart(raw, false)
            val next = if (params.value.isEmpty()) {
                route.value
            } else {
                route.value + "?" + params.value
            }
            return SanitizedPart(next, true)
        }

        private fun sanitizeParameterList(raw: String): SanitizedPart {
            if (raw.isEmpty()) return SanitizedPart(raw, false)
            var changed = false
            val kept = raw.split('&').filter { part ->
                val remove = isForeignWebAuthTokenPart(part)
                if (remove) changed = true
                !remove
            }
            return SanitizedPart(if (changed) kept.joinToString("&") else raw, changed)
        }

        private fun isForeignWebAuthTokenPart(part: String): Boolean {
            val separator = part.indexOf('=')
            val encodedName = if (separator < 0) part else part.substring(0, separator)
            val name = decodedParameterName(encodedName).lowercase()
            return name.startsWith("tgwebauth") || name == "autologin_token"
        }

        private fun decodedParameterName(encoded: String): String {
            var result = encoded
            repeat(4) {
                val decoded = runCatching {
                    URLDecoder.decode(
                        result.replace("+", "%2B"),
                        StandardCharsets.UTF_8.name(),
                    )
                }.getOrNull() ?: return@repeat
                if (decoded == result) return@repeat
                result = decoded
            }
            while (result.startsWith("?")) result = result.drop(1)
            return result
        }
    }
}
