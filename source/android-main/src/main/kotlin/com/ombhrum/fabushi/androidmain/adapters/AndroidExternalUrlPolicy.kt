package com.ombhrum.fabushi.androidmain.adapters

import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

internal object AndroidExternalUrlPolicy {
    private val allowedProtocols = setOf("http", "https", "mailto", "obsidian", "tel")

    fun parseAllowedExternalUrl(rawUrl: String): String? {
        if (rawUrl.isEmpty() || rawUrl.length > MAX_URL_LENGTH) return null
        val uri = runCatching { URI(rawUrl) }.getOrNull() ?: return null
        val scheme = uri.scheme?.lowercase() ?: return null
        if (scheme !in allowedProtocols) return null
        if (scheme != "http" && scheme != "https") return uri.toASCIIString()
        if (uri.host.isNullOrBlank()) return null
        return stripForeignWebAuthTokens(uri.toASCIIString())
    }

    fun parseServerAcceptedAuthExternalUrl(
        rawUrl: String,
        expectedOrigin: String,
    ): URI? {
        if (rawUrl.isEmpty() || rawUrl.length > MAX_URL_LENGTH) return null
        val url = runCatching { URI(rawUrl) }.getOrNull() ?: return null
        val expected = runCatching { URI(expectedOrigin) }.getOrNull() ?: return null
        if (!url.isHttpFamily() || !expected.isHttpFamily()) return null
        if (url.userInfo != null || expected.userInfo != null) return null
        if (url.host.isNullOrBlank() || expected.host.isNullOrBlank()) return null
        if (originKey(url) != originKey(expected)) return null
        return url
    }

    private fun stripForeignWebAuthTokens(rawUrl: String): String {
        val hashIndex = rawUrl.indexOf('#')
        val beforeFragment = if (hashIndex >= 0) rawUrl.substring(0, hashIndex) else rawUrl
        val rawFragment = if (hashIndex >= 0) rawUrl.substring(hashIndex + 1) else null

        val queryIndex = beforeFragment.indexOf('?')
        val base = if (queryIndex >= 0) beforeFragment.substring(0, queryIndex) else beforeFragment
        val rawQuery = if (queryIndex >= 0) beforeFragment.substring(queryIndex + 1) else null

        val query = rawQuery?.let(::withoutForeignWebAuthTokenParams)
        val fragment = rawFragment?.let(::withoutForeignWebAuthTokenFragment)

        return buildString {
            append(base)
            if (rawQuery != null && !query.isNullOrEmpty()) append('?').append(query)
            if (rawFragment != null && !fragment.isNullOrEmpty()) append('#').append(fragment)
        }
    }

    private fun withoutForeignWebAuthTokenFragment(encoded: String): String {
        val question = encoded.indexOf('?')
        if (question < 0) return withoutForeignWebAuthTokenParams(encoded)
        val routePart = encoded.substring(0, question)
        val paramsPart = encoded.substring(question + 1)
        val route = withoutForeignWebAuthTokenParams(routePart)
        val params = withoutForeignWebAuthTokenParams(paramsPart)
        return if (params.isEmpty()) route else "$route?$params"
    }

    private fun withoutForeignWebAuthTokenParams(encoded: String): String =
        encoded.split('&').filterNot(::isForeignWebAuthTokenPart).joinToString("&")

    private fun isForeignWebAuthTokenPart(part: String): Boolean {
        val encodedName = part.substringBefore('=')
        val name = decodedParameterName(encodedName).lowercase()
        return name.startsWith("tgwebauth") || name == "autologin_token"
    }

    private fun decodedParameterName(value: String): String {
        var result = value
        repeat(4) {
            val next = runCatching {
                URLDecoder.decode(result.replace("+", "%2B"), StandardCharsets.UTF_8.name())
            }.getOrElse { return@repeat }
            if (next == result) return@repeat
            result = next
        }
        return result.dropWhile { it == '?' }
    }

    private fun URI.isHttpFamily(): Boolean =
        scheme.equals("http", ignoreCase = true) || scheme.equals("https", ignoreCase = true)

    private fun originKey(uri: URI): Triple<String, String, Int> {
        val scheme = uri.scheme.lowercase()
        val port = when {
            uri.port >= 0 -> uri.port
            scheme == "https" -> 443
            else -> 80
        }
        return Triple(scheme, uri.host.lowercase(), port)
    }

    private const val MAX_URL_LENGTH = 8_192
}
