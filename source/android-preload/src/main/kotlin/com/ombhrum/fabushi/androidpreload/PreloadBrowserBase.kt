package com.ombhrum.fabushi.androidpreload

object PreloadBrowserBase {
    private val exactIdentityHosts = setOf(
        "accounts.google.com",
        "appleid.apple.com",
        "github.com",
    )

    private val identityHostSuffixes = listOf(
        ".okta.com",
        ".okta-emea.com",
        ".oktapreview.com",
        ".duosecurity.com",
        ".login.microsoftonline.com",
        ".onelogin.com",
        ".auth0.com",
        ".pingidentity.com",
        ".rippling.com",
    )

    fun isAllowlistedIdentityHost(host: String): Boolean {
        val normalized = host.lowercase().trim().trimEnd('.')
        if (normalized.isEmpty()) return false
        if (normalized in exactIdentityHosts) return true
        return identityHostSuffixes.any { suffix ->
            normalized.endsWith(suffix) &&
                normalized.length > suffix.length
        }
    }

    fun isLoopbackHost(host: String): Boolean =
        host.equals("localhost", true) || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}
