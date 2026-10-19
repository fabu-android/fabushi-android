package com.ombhrum.fabushi.androidpreload
object PreloadWebview {
    fun isNavigationAllowed(url: String, allowedHttpsHosts: Set<String>): Boolean = runCatching {
        val uri = java.net.URI(url)
        when (uri.scheme?.lowercase()) {
            "https" -> uri.host?.lowercase() in allowedHttpsHosts.map { it.lowercase() }.toSet()
            "http" -> PreloadBrowserBase.isLoopbackHost(uri.host.orEmpty())
            else -> false
        }
    }.getOrDefault(false)
}
