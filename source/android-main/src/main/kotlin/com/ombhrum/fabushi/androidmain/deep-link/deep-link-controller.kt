package com.ombhrum.fabushi.androidmain.deeplink

import com.ombhrum.fabushi.androidmain.auth.AndroidAuthCallbackRegistration
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AuthCompletionStatus
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLinkSource
import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

internal object AndroidDeepLinkRouter {
    private const val MAX_LENGTH = 2_048
    private val attemptIdPattern = Regex("^[A-Za-z0-9_-]{8,96}$")
    private val mcpOAuthStatePattern = Regex("^[A-Za-z0-9._~-]{16,512}$")
    private val appSections = setOf("settings", "feedback", "about", "widgets", "onboarding")

    fun parse(raw: String): AndroidDeepLink? {
        if (raw.isEmpty() || raw.length > MAX_LENGTH) return null
        if (raw.any { it.code < 33 || it.code > 126 }) return null
        if ('#' in raw || '\\' in raw || !hasValidPercentEncoding(raw)) return null

        val uri = runCatching { URI(raw) }.getOrNull() ?: return null
        if (!uri.scheme.equals(AndroidAuthCallbackRegistration.PROTOCOL_SCHEME, ignoreCase = true)) return null
        if (uri.userInfo != null || uri.port != -1 || uri.fragment != null) return null
        val host = uri.host?.lowercase().orEmpty()
        if (host.isBlank()) return null
        if (!hasCanonicalPath(uri.rawPath.orEmpty())) return null
        val query = parseQuery(uri.rawQuery) ?: return null

        return when (host) {
            "auth" -> parseAuth(uri.path.orEmpty(), query)
            "mcp-oauth" -> parseMcpOAuth(uri.path.orEmpty(), query)
            "app" -> parseInfo(uri.path.orEmpty(), query)
            "agent" -> parseAgent(uri.path.orEmpty(), query)
            in appSections -> parseSection(host, uri.path.orEmpty(), query)
            else -> null
        }
    }

    private fun parseAuth(
        path: String,
        query: Map<String, List<String>>,
    ): AndroidDeepLink? {
        if (path != "/complete") return null
        if (query.keys.any { it !in setOf("attemptId", "status") }) return null
        val attemptIds = query["attemptId"].orEmpty()
        val statuses = query["status"].orEmpty()
        if (attemptIds.size != 1 || statuses.size > 1) return null
        val attemptId = attemptIds.single()
        if (!attemptIdPattern.matches(attemptId)) return null
        val status = when (statuses.singleOrNull()?.lowercase() ?: "completed") {
            "completed" -> AuthCompletionStatus.COMPLETED
            "cancelled" -> AuthCompletionStatus.CANCELLED
            "failed" -> AuthCompletionStatus.FAILED
            else -> return null
        }
        return AndroidDeepLink.AuthCompletion(attemptId, status)
    }

    private fun parseMcpOAuth(
        path: String,
        query: Map<String, List<String>>,
    ): AndroidDeepLink? {
        if (path != "/callback") return null
        if (query.keys.any { it !in setOf("state", "code", "error") }) return null
        val states = query["state"].orEmpty()
        val codes = query["code"].orEmpty()
        val errors = query["error"].orEmpty()
        if (states.size != 1 || codes.size > 1 || errors.size > 1) return null
        if ((codes.isEmpty()) == (errors.isEmpty())) return null

        val state = states.single()
        if (!mcpOAuthStatePattern.matches(state)) return null
        val code = codes.singleOrNull()?.takeIf { it.isNotBlank() && it.length <= 4_096 }
        val error = errors.singleOrNull()?.takeIf { it.isNotBlank() && it.length <= 1_024 }
        if ((code == null) == (error == null)) return null
        return AndroidDeepLink.McpOAuthCallback(
            state = state,
            code = code,
            error = error,
        )
    }

    private fun parseInfo(
        path: String,
        query: Map<String, List<String>>,
    ): AndroidDeepLink? {
        if (path != "/v1/info") return null
        if (query.keys != setOf("topic")) return null
        val topics = query["topic"].orEmpty()
        if (topics.size != 1 || topics.single() != "deep-links") return null
        return AndroidDeepLink.Info(
            source = AndroidDeepLinkSource.PROTOCOL,
            topic = "deep-links",
        )
    }

    private fun parseAgent(
        path: String,
        query: Map<String, List<String>>,
    ): AndroidDeepLink? {
        if (query.isNotEmpty()) return null
        val segments = path.split('/').filter(String::isNotBlank)
        if (segments.size != 1) return null
        val agentId = segments.single()
        if (agentId.length > 200) return null
        if (agentId.any { it.code < 33 || it.code > 126 }) return null
        return AndroidDeepLink.Agent(agentId)
    }

    private fun parseSection(
        section: String,
        path: String,
        query: Map<String, List<String>>,
    ): AndroidDeepLink? {
        if (path.isNotEmpty() && path != "/") return null
        if (query.isNotEmpty()) return null
        return AndroidDeepLink.AppSection(section)
    }

    private fun parseQuery(raw: String?): Map<String, List<String>>? {
        if (raw.isNullOrEmpty()) return emptyMap()
        val out = linkedMapOf<String, MutableList<String>>()
        for (part in raw.split('&')) {
            if (part.isEmpty()) return null
            val equals = part.indexOf('=')
            val rawKey = if (equals >= 0) part.substring(0, equals) else part
            val rawValue = if (equals >= 0) part.substring(equals + 1) else ""
            val key = decode(rawKey) ?: return null
            val value = decode(rawValue) ?: return null
            out.getOrPut(key) { mutableListOf() }.add(value)
        }
        return out
    }

    private fun decode(value: String): String? =
        runCatching { URLDecoder.decode(value, StandardCharsets.UTF_8.name()) }.getOrNull()

    private fun hasValidPercentEncoding(value: String): Boolean {
        var index = 0
        while (index < value.length) {
            if (value[index] == '%') {
                if (index + 2 >= value.length) return false
                if (!value[index + 1].isHexDigit() || !value[index + 2].isHexDigit()) return false
                index += 3
            } else {
                index += 1
            }
        }
        return true
    }

    private fun hasCanonicalPath(path: String): Boolean {
        if ('%' in path) return false
        val segments = path.split('/')
        return segments.none { it == "." || it == ".." }
    }

    private fun Char.isHexDigit(): Boolean =
        this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'
}


internal const val DEEP_LINK_PENDING_MAX = 16
internal const val DEEP_LINK_DEDUPE_WINDOW_MS = 2_000L

internal class AndroidDeepLinkController(
    private val dispatch: (AndroidDeepLink) -> Unit,
    private val focusWindow: () -> Unit = {},
    private val log: (String) -> Unit = {},
    private val nowMs: () -> Long = System::currentTimeMillis,
) {
    private var rendererReady = false
    private val pending = ArrayDeque<Pair<String, AndroidDeepLink>>()
    private val recentlyAccepted = linkedMapOf<String, Long>()

    fun handleCandidate(raw: String, origin: String): Boolean {
        val parsed = AndroidDeepLinkRouter.parse(raw)
        if (parsed == null) {
            log("deep-link: ignored invalid candidate from $origin")
            return false
        }
        val canonical = canonicalKey(parsed)
        if (isDuplicate(canonical)) {
            log("deep-link: deduped $canonical from $origin")
            return false
        }
        if (!rendererReady && pending.size >= DEEP_LINK_PENDING_MAX) {
            log("deep-link: dropped $canonical from $origin (pending queue full)")
            return false
        }

        recentlyAccepted[canonical] = nowMs()
        focusWindow()
        if (rendererReady) {
            dispatch(parsed)
        } else {
            pending.addLast(canonical to parsed)
        }
        return true
    }

    fun hasPendingActivation(): Boolean = pending.isNotEmpty()

    fun markReady() {
        rendererReady = true
        while (pending.isNotEmpty()) {
            dispatch(pending.removeFirst().second)
        }
    }

    fun markNotReady() {
        rendererReady = false
        val pendingKeys = pending.mapTo(linkedSetOf()) { it.first }
        recentlyAccepted.keys.retainAll(pendingKeys)
    }

    private fun isDuplicate(canonical: String): Boolean {
        val now = nowMs()
        recentlyAccepted.entries.removeAll { now - it.value > DEEP_LINK_DEDUPE_WINDOW_MS }
        return pending.any { it.first == canonical } || recentlyAccepted.containsKey(canonical)
    }

    private fun canonicalKey(link: AndroidDeepLink): String = when (link) {
        is AndroidDeepLink.AuthCompletion ->
            "auth:${link.attemptId}:${link.status.name.lowercase()}"
        is AndroidDeepLink.McpOAuthCallback ->
            "mcp-oauth:${link.state}:${link.code ?: "error:" + link.error}"
        is AndroidDeepLink.Info -> "info:${link.source.name.lowercase()}:${link.topic}"
        is AndroidDeepLink.Agent -> "agent:${link.agentId}"
        is AndroidDeepLink.AppSection -> "section:${link.section}"
    }
}
