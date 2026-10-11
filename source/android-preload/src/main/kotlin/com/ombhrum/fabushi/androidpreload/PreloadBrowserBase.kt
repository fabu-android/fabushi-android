package com.ombhrum.fabushi.androidpreload
object PreloadBrowserBase {
    private val identityHosts = setOf("accounts.google.com", "appleid.apple.com", "github.com")
    fun isAllowlistedIdentityHost(host: String): Boolean = host.lowercase().trimEnd('.') in identityHosts
    fun isLoopbackHost(host: String): Boolean =
        host.equals("localhost", true) || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}
