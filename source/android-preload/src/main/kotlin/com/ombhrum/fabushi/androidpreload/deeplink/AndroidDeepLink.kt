package com.ombhrum.fabushi.androidpreload.deeplink

sealed interface AndroidDeepLink {
    data class AuthCompletion(
        val attemptId: String,
        val status: AuthCompletionStatus,
    ) : AndroidPresentationDeepLink

    data class McpOAuthCallback(
        val state: String,
        val code: String?,
        val error: String?,
    ) : AndroidDeepLink

    data class Info(
        val source: AndroidDeepLinkSource,
        val topic: String = "deep-links",
    ) : AndroidPresentationDeepLink

    data class Agent(val agentId: String) : AndroidPresentationDeepLink

    data class AppSection(val section: String) : AndroidPresentationDeepLink
}

sealed interface AndroidPresentationDeepLink : AndroidDeepLink

enum class AuthCompletionStatus {
    COMPLETED,
    CANCELLED,
    FAILED,
}

enum class AndroidDeepLinkSource {
    PROTOCOL,
    HTTPS,
}
