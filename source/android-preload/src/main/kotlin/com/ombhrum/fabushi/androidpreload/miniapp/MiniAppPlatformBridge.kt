package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.StandardCharsets
import java.util.UUID

/**
 * Android adapter for the canonical Fabushi Mini App control plane.
 *
 * Account credentials remain owned by the Rust Mahayana Host. This adapter only
 * receives a five-minute plugin-audience token when it must speak Streamable
 * HTTP MCP, and never forwards that token into a WebView or persistent storage.
 */
private object CanonicalInstalledProjectionCache {
    @Volatile
    private var projectionJson: String? = null

    fun store(manifests: JSONArray) {
        projectionJson = manifests.toString()
    }

    fun load(): JSONArray? = projectionJson?.let { JSONArray(it) }
}

internal class MiniAppPlatformBridge(
    private val coordinator: AndroidCoordinatorPort,
) {
    companion object {
        const val GLOBAL_DHARMA_ID = "global-dharma"
        const val PRAYER_WHEEL_CAPABILITY = "local.prayer-wheel.start"
        const val PRAYER_WHEEL_LIFETIME_SKU = "local-prayer-wheel.lifetime"
        const val PRAYER_WHEEL_LIFETIME_PRODUCT_ID = "prod.global-dharma.local-prayer-wheel.lifetime"
        const val PRAYER_WHEEL_LIFETIME_CNY_MINOR = 108_000L
        private const val API_BASE = "https://api.ombhrum.com"
        private const val MCP_PROTOCOL = "2025-06-18"
        private val PluginId = Regex("^[a-z0-9][a-z0-9-]{1,63}$")
        private val ToolName = Regex("^[A-Za-z0-9_.-]{1,120}$")
    }

    fun readInstalledMiniApps(): JSONArray {
        val response = platform("GET", "/v1/marketplace/added")
        return response.optJSONArray("apps")
            ?: error("Canonical installed projection did not include apps array")
    }

    fun rememberInstalledMiniApps(manifests: JSONArray) {
        CanonicalInstalledProjectionCache.store(manifests)
    }

    fun lastInstalledMiniApps(): JSONArray? = CanonicalInstalledProjectionCache.load()

    fun confirmInstalledMiniApp(pluginId: String): JSONObject {
        requirePluginId(pluginId)
        platform(
            method = "POST",
            path = "/v1/marketplace/plugins/$pluginId/add",
            body = JSONObject().put("platform", "android"),
        )
        val manifests = readInstalledMiniApps()
        for (index in 0 until manifests.length()) {
            val manifest = manifests.optJSONObject(index) ?: continue
            val projectedId = manifest.optString("id").ifBlank { manifest.optString("pluginId") }
            if (projectedId != pluginId) continue
            if (pluginId == GLOBAL_DHARMA_ID) {
                val bot = manifest.optJSONObject("bot")
                    ?: error("Canonical Global Dharma manifest did not include bot projection")
                check(bot.optString("id").isNotBlank()) {
                    "Canonical Global Dharma bot projection did not include id"
                }
            }
            rememberInstalledMiniApps(manifests)
            return manifest
        }
        error("Canonical installed projection did not include $pluginId after account install")
    }

    fun routeInput(pluginId: String, input: String): JSONObject {
        requirePluginId(pluginId)
        require(input.isNotBlank()) { "Mini App input must not be blank" }
        return platform(
            method = "POST",
            path = "/v1/marketplace/plugins/$pluginId/route",
            body = JSONObject().put("input", input),
        )
    }

    fun callOfficialMcpTool(
        pluginId: String,
        name: String,
        arguments: JSONObject,
        requestId: String = "official:" + UUID.randomUUID().toString(),
    ): JSONObject {
        requirePluginId(pluginId)
        require(ToolName.matches(name)) { "Invalid Mini App MCP tool name" }
        require(requestId.length in 8..256 && requestId.none(Char::isWhitespace)) {
            "Invalid stable Mini App MCP request id"
        }
        val operationId = "miniapp:" + pluginId + ":" + requestId
        publishOperation(pluginId, operationId, name, "started", "正在连接统一 WebMCP")
        return try {
            val token = delegatedPluginToken(pluginId)
            val endpoint = "$API_BASE/api/mcp/apps/$pluginId"
            val initialize = JSONObject()
                .put("jsonrpc", "2.0")
                .put("id", 1)
                .put("method", "initialize")
                .put(
                    "params",
                    JSONObject()
                        .put("protocolVersion", MCP_PROTOCOL)
                        .put("capabilities", JSONObject())
                        .put(
                            "clientInfo",
                            JSONObject()
                                .put("name", "fabushi-android-miniapp-host")
                                .put("version", BuildConfig.VERSION_NAME),
                        ),
                )
            val initialized = mcpPost(endpoint, token, null, initialize, expectJson = true)
            val sessionId = initialized.sessionId
                ?: error("Mini App MCP initialize did not return mcp-session-id")
            ensureNoMcpError(initialized.body, "initialize")
            publishOperation(pluginId, operationId, name, "running", "WebMCP 会话已建立")
            try {
                val notification = JSONObject()
                    .put("jsonrpc", "2.0")
                    .put("method", "notifications/initialized")
                    .put("params", JSONObject())
                mcpPost(endpoint, token, sessionId, notification, expectJson = false)

                val listRequest = JSONObject()
                    .put("jsonrpc", "2.0")
                    .put("id", 2)
                    .put("method", "tools/list")
                    .put("params", JSONObject())
                val listResponse = mcpPost(endpoint, token, sessionId, listRequest, expectJson = true).body
                    ?: error("Mini App MCP tools/list returned an empty response")
                ensureNoMcpError(listResponse, "tools/list")
                val tools = listResponse.optJSONObject("result")?.optJSONArray("tools")
                    ?: error("Mini App MCP tools/list did not return tools")
                check((0 until tools.length()).any { tools.optJSONObject(it)?.optString("name") == name }) {
                    "Mini App MCP tool $name is not advertised by tools/list"
                }
                publishOperation(pluginId, operationId, name, "running", "WebMCP tools/list 已验证")

                val call = JSONObject()
                    .put("jsonrpc", "2.0")
                    .put("id", 3)
                    .put("method", "tools/call")
                    .put(
                        "params",
                        JSONObject()
                            .put("name", name)
                            .put("arguments", arguments),
                    )
                val response = mcpPost(endpoint, token, sessionId, call, expectJson = true).body
                    ?: error("Mini App MCP tools/call returned an empty response")
                ensureNoMcpError(response, "tools/call")
                val result = response.optJSONObject("result")
                    ?: error("Mini App MCP tools/call did not return result")
                enforceProtectedHostRequest(pluginId, result)
                publishOperation(pluginId, operationId, name, "completed", "WebMCP Tool 已完成")
                result
            } finally {
                runCatching { mcpDelete(endpoint, token, sessionId) }
            }
        } catch (error: Throwable) {
            publishOperation(
                pluginId,
                operationId,
                name,
                "failed",
                error.message?.take(240) ?: "WebMCP Tool 调用失败",
            )
            throw error
        }
    }

    fun entitlement(pluginId: String, capability: String): JSONObject {
        requirePluginId(pluginId)
        require(capability.matches(Regex("^[a-z0-9][a-z0-9_.-]{1,127}$"))) { "Invalid entitlement capability" }
        return platform("GET", "/v1/plugins/$pluginId/entitlements/$capability")
    }

    fun purchase(pluginId: String, sku: String, idempotencyKey: String): JSONObject {
        requirePluginId(pluginId)
        require(sku.matches(Regex("^[A-Za-z0-9._-]{1,160}$"))) { "Invalid commerce SKU" }
        require(idempotencyKey.length in 12..160 && idempotencyKey.none(Char::isWhitespace)) {
            "Invalid purchase idempotency key"
        }
        return platform(
            method = "POST",
            path = "/v1/plugins/$pluginId/commerce/purchase",
            body = JSONObject()
                .put("sku", sku)
                .put("idempotencyKey", idempotencyKey),
        )
    }

    fun restorePurchases(): JSONObject = platform(
        method = "POST",
        path = "/v1/purchases/restore",
        body = JSONObject(),
    )

    private fun publishOperation(
        pluginId: String,
        operationId: String,
        tool: String,
        status: String,
        message: String,
    ) {
        coordinator.publishFeatureEvent(
            JSONObject()
                .put("type", "miniapp.operation")
                .put("miniAppId", pluginId)
                .put("pluginId", pluginId)
                .put("operationId", operationId)
                .put("tool", tool)
                .put("status", status)
                .put("message", message),
        )
    }

    private fun enforceProtectedHostRequest(pluginId: String, result: JSONObject) {
        val hostRequest = result.optJSONObject("structuredContent")?.optJSONObject("hostRequest") ?: return
        val capability = hostRequest.optString("capability")
        if (capability != PRAYER_WHEEL_CAPABILITY) return
        check(pluginId == GLOBAL_DHARMA_ID) { "Protected prayer-wheel capability is owned by Global Dharma" }
        val access = entitlement(pluginId, capability).optJSONObject("access")
            ?: error("Canonical entitlement response did not include access decision")
        check(access.optBoolean("protected", false)) {
            "Prayer-wheel capability is not marked protected by the canonical entitlement service"
        }
        check(access.optBoolean("allowed", false)) {
            "本地转经轮尚未获得有效权益：${access.optString("reason", "not_entitled")}"
        }
    }

    private fun delegatedPluginToken(pluginId: String): String {
        val response = platform(
            method = "POST",
            path = "/v1/auth/plugin-token",
            body = JSONObject()
                .put("pluginId", pluginId)
                .put("deviceId", "fabushi-android-miniapp-host")
                .put("scopes", JSONArray().put("miniapp:$pluginId")),
        )
        return response.optString("accessToken").trim().takeIf { it.length >= 24 }
            ?: error("Fabushi did not issue a delegated Mini App token")
    }

    private fun platform(method: String, path: String, body: JSONObject? = null): JSONObject {
        val params = JSONObject()
            .put("method", method)
            .put("path", path)
            .put("authenticated", true)
        if (body != null) params.put("body", body)
        val response = coordinator.platformRequest(params)
        if (!response.optBoolean("ok", false)) {
            val status = response.optInt("statusCode", 0)
            val data = response.opt("data")
            error("Fabushi platform request failed: $method $path -> HTTP $status ${data ?: response.optString("bodyText")}")
        }
        val data = response.opt("data")
        return when (data) {
            is JSONObject -> data
            is JSONArray -> JSONObject().put("items", data)
            is String -> runCatching { JSONObject(data) }.getOrElse { JSONObject().put("value", data) }
            else -> JSONObject()
        }
    }

    private data class McpResponse(
        val body: JSONObject?,
        val sessionId: String?,
    )

    private fun mcpPost(
        endpoint: String,
        token: String,
        sessionId: String?,
        payload: JSONObject,
        expectJson: Boolean,
    ): McpResponse {
        val connection = (URL(endpoint).openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 15_000
            readTimeout = 30_000
            doOutput = true
            setRequestProperty("Authorization", "Bearer $token")
            setRequestProperty("Content-Type", "application/json")
            setRequestProperty("Accept", "application/json, text/event-stream")
            setRequestProperty("MCP-Protocol-Version", MCP_PROTOCOL)
            if (!sessionId.isNullOrBlank()) setRequestProperty("Mcp-Session-Id", sessionId)
        }
        val bytes = payload.toString().toByteArray(StandardCharsets.UTF_8)
        connection.setFixedLengthStreamingMode(bytes.size)
        connection.outputStream.use { it.write(bytes) }
        val status = connection.responseCode
        val returnedSession = connection.getHeaderField("Mcp-Session-Id")?.trim()?.takeIf(String::isNotBlank)
        val stream = if (status in 200..299) connection.inputStream else connection.errorStream
        val raw = stream?.bufferedReader(StandardCharsets.UTF_8)?.use { it.readText() }.orEmpty()
        connection.disconnect()
        check(status in 200..299) { "Mini App MCP HTTP $status: ${raw.take(800)}" }
        if (!expectJson || raw.isBlank()) return McpResponse(null, returnedSession ?: sessionId)
        val body = runCatching { JSONObject(raw) }
            .getOrElse { error("Mini App MCP returned non-JSON response") }
        return McpResponse(body, returnedSession ?: sessionId)
    }

    private fun mcpDelete(endpoint: String, token: String, sessionId: String) {
        val connection = (URL(endpoint).openConnection() as HttpURLConnection).apply {
            requestMethod = "DELETE"
            connectTimeout = 5_000
            readTimeout = 5_000
            setRequestProperty("Authorization", "Bearer $token")
            setRequestProperty("Accept", "application/json, text/event-stream")
            setRequestProperty("MCP-Protocol-Version", MCP_PROTOCOL)
            setRequestProperty("Mcp-Session-Id", sessionId)
        }
        connection.responseCode
        connection.disconnect()
    }

    private fun ensureNoMcpError(body: JSONObject?, phase: String) {
        val mcpError = body?.optJSONObject("error") ?: return
        error("Mini App MCP $phase failed: ${mcpError.optString("message", mcpError.toString())}")
    }

    private fun requirePluginId(pluginId: String) {
        require(PluginId.matches(pluginId)) { "Invalid Mini App id" }
    }
}
