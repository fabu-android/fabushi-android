package com.ombhrum.fabushi

import android.content.Context
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.optNonNullString
import com.ombhrum.fabushi.telegram.optNullableString
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.time.Instant
import java.util.concurrent.TimeUnit

/**
 * Account-scoped remote-device transport owned by the installed Android App.
 * Only the existing fabushi.app.* semantic surface is published; no arbitrary
 * shell, Kotlin/Java reflection, JavaScript execution, or credential writes are
 * exposed through this transport.
 */
internal class FabushiRemoteDeviceGateway(
    context: Context,
    private val surface: FabushiAppAgentSurface,
    private val metadata: Map<String, String>,
    private val configuredDeviceName: String? = null,
    private val phoneBridge: com.ombhrum.fabushi.telegram.MikaBridgeCallbacks? = null,
    private val gatewayUrl: String = OfficialGatewayUrl,
    private val retryDelayMs: Long = 10_000L,
) : AutoCloseable {
    companion object {
        internal const val OfficialGatewayUrl = "wss://fabushi-mcp.ombhrum.com/agent"
        private const val LeaseSeconds = 14_400
        private const val MaxMessageBytes = 32 * 1024 * 1024

        const val PhoneStatusTool = "fabushi.phone.status"
        const val PhoneDispatchTool = "fabushi.phone.dispatch"
        const val PhoneStopTool = "fabushi.phone.stop"
        val PhoneToolNames: List<String> = listOf(PhoneStatusTool, PhoneDispatchTool, PhoneStopTool)
    }

    private data class AgentSession(
        val accessToken: String,
        val deviceId: String,
        val sessionId: String,
        val username: String,
        val accessTokenExpiresAt: Long,
    )

    private val host = MahayanaHost(context.applicationContext)
    private val traceFile = File(context.getExternalFilesDir(null) ?: context.filesDir, "device-gateway-trace.jsonl")
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(25, TimeUnit.SECONDS)
        .build()
    private val stateLock = Any()
    private var desiredLoggedIn = false
    private var closed = false
    private var monitorJob: Job? = null
    private var socket: WebSocket? = null
    private var activeSession: AgentSession? = null
    private var registered = false

    internal fun isRegistered(): Boolean = synchronized(stateLock) { registered }
    internal fun isClosed(): Boolean = synchronized(stateLock) { closed }

    fun setLoggedIn(loggedIn: Boolean) {
        val jobToCancel: Job?
        synchronized(stateLock) {
            if (closed) return
            desiredLoggedIn = loggedIn
            if (!loggedIn) {
                jobToCancel = monitorJob
                monitorJob = null
            } else {
                if (monitorJob == null) {
                    monitorJob = scope.launch {
                        while (isActive) {
                            refreshConnection()
                            delay(retryDelayMs.coerceAtLeast(50L))
                        }
                    }
                }
                return
            }
        }
        jobToCancel?.cancel()
        stopConnection("logged-out")
    }

    private suspend fun refreshConnection() {
        if (!synchronized(stateLock) { !closed && desiredLoggedIn }) return
        runCatching {
            parseAgentSession(host.request("feature.auth.deviceAgentSession"))
        }.onSuccess { candidate ->
            val shouldConnect = synchronized(stateLock) {
                if (closed || !desiredLoggedIn) {
                    false
                } else {
                    val current = activeSession
                    val stillUsable = current?.deviceId == candidate.deviceId &&
                        current.sessionId == candidate.sessionId &&
                        current.accessToken == candidate.accessToken &&
                        registered && socket != null
                    !stillUsable
                }
            }
            if (shouldConnect) connect(candidate)
        }.onFailure { error ->
            appendTrace("connection-refresh-failed", mapOf("error" to safeErrorCode(error)))
            stopConnection("refresh-failed")
        }
    }

    private fun connect(session: AgentSession) {
        if (!synchronized(stateLock) { !closed && desiredLoggedIn }) return
        stopConnection("reconnect")
        if (!synchronized(stateLock) { !closed && desiredLoggedIn }) return
        val request = Request.Builder()
            .url(gatewayUrl)
            .header("Authorization", "Bearer ${session.accessToken}")
            .build()
        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val active = synchronized(stateLock) {
                    !closed && desiredLoggedIn && socket === webSocket
                }
                if (!active) {
                    webSocket.close(1000, "stale")
                    return
                }
                val advertisedCapabilities = if (phoneBridge != null) {
                    FabushiAppAgentSurface.ToolNames + PhoneToolNames
                } else {
                    FabushiAppAgentSurface.ToolNames
                }
                val registration = JSONObject()
                    .put("type", "register")
                    .put("deviceId", session.deviceId)
                    .put("name", configuredDeviceName ?: "Fabushi Android ${session.deviceId}".take(200))
                    .put("platform", "android")
                    .put("capabilities", JSONArray(advertisedCapabilities))
                    .put("tools", toolDescriptors())
                    .put("leaseSeconds", LeaseSeconds)
                    .put("metadata", JSONObject(metadata))
                if (!webSocket.send(registration.toString())) {
                    appendTrace("register-send-failed", mapOf("deviceId" to session.deviceId.take(128)))
                    webSocket.close(1011, "registration failed")
                    return
                }
                appendTrace(
                    "register-sent",
                    mapOf("deviceId" to session.deviceId.take(128), "sessionId" to session.sessionId.take(96)),
                )
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (text.toByteArray(Charsets.UTF_8).size > MaxMessageBytes) return
                val message = runCatching { JSONObject(text) }.getOrNull() ?: return
                when (message.optNonNullString("type")) {
                    "registered" -> {
                        val accepted = synchronized(stateLock) {
                            if (closed || !desiredLoggedIn || socket !== webSocket) {
                                false
                            } else {
                                registered = true
                                true
                            }
                        }
                        if (!accepted) {
                            webSocket.close(1000, "stale")
                            return
                        }
                        appendTrace("registered", mapOf("deviceId" to message.optNonNullString("deviceId").take(128)))
                    }
                    "call" -> {
                        val active = synchronized(stateLock) { !closed && desiredLoggedIn && socket === webSocket }
                        if (!active) {
                            webSocket.close(1000, "stale")
                            return
                        }
                        scope.launch { handleCall(webSocket, message) }
                    }
                }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                synchronized(stateLock) {
                    if (socket === webSocket) {
                        socket = null
                        registered = false
                    }
                }
                appendTrace("socket-closed", mapOf("code" to code, "reason" to reason.take(80)))
            }

            override fun onFailure(webSocket: WebSocket, error: Throwable, response: Response?) {
                synchronized(stateLock) {
                    if (socket === webSocket) {
                        socket = null
                        registered = false
                    }
                }
                appendTrace("receive-failed", mapOf("error" to safeErrorCode(error)))
            }
        }
        synchronized(stateLock) {
            if (closed || !desiredLoggedIn) return
            activeSession = session
            registered = false
            socket = client.newWebSocket(request, listener)
        }
    }

    private suspend fun handleCall(webSocket: WebSocket, message: JSONObject) {
        val requestId = message.optNonNullString("requestId").take(128)
        val toolName = message.optNonNullString("toolName").take(128)
        if (requestId.isBlank() || toolName.isBlank()) return
        val arguments = message.optJSONObject("arguments") ?: JSONObject()
        appendTrace("call-started", mapOf("requestId" to requestId, "toolName" to toolName))
        runCatching { invokeTool(toolName, arguments) }
            .onSuccess { structured ->
                val result = JSONObject()
                    .put("structuredContent", structured)
                    .put(
                        "content",
                        JSONArray().put(
                            JSONObject()
                                .put("type", "text")
                                .put("text", "Fabushi Android semantic tool completed."),
                        ),
                    )
                webSocket.send(
                    JSONObject()
                        .put("type", "result")
                        .put("requestId", requestId)
                        .put("ok", true)
                        .put("result", result)
                        .toString(),
                )
                appendTrace("call-completed", mapOf("requestId" to requestId, "toolName" to toolName, "ok" to true))
            }
            .onFailure { error ->
                val safe = safeErrorCode(error)
                webSocket.send(
                    JSONObject()
                        .put("type", "result")
                        .put("requestId", requestId)
                        .put("ok", false)
                        .put("error", safe)
                        .toString(),
                )
                appendTrace(
                    "call-completed",
                    mapOf("requestId" to requestId, "toolName" to toolName, "ok" to false, "error" to safe),
                )
            }
    }

    internal suspend fun invokeTool(toolName: String, arguments: JSONObject): JSONObject =
        withContext(Dispatchers.Main.immediate) {
            val allowedTools = if (phoneBridge != null) {
                FabushiAppAgentSurface.ToolNames + PhoneToolNames
            } else {
                FabushiAppAgentSurface.ToolNames
            }
            require(toolName in allowedTools) { "unknown_fabushi_app_tool" }
            when (toolName) {
                PhoneStatusTool -> {
                    val snap = checkNotNull(phoneBridge).currentSnapshot()
                    JSONObject()
                        .put("available", snap.available)
                        .put("accessibilityEnabled", snap.accessibilityEnabled)
                        .put("wirelessAdbPaired", snap.wirelessAdbPaired)
                        .put("helperBinderAlive", snap.helperBinderAlive)
                        .put("virtualDisplayActive", snap.virtualDisplayActive)
                        .put("capsuleVisible", snap.capsuleVisible)
                        .put("sandboxReady", snap.sandboxReady)
                        .put("activeSpeedProfile", snap.activeSpeedProfile)
                        .put("hasActiveTask", snap.activeTask != null)
                        .put("hasPendingConfirmation", snap.pendingConfirmationPrompt != null)
                }
                PhoneDispatchTool -> {
                    val prompt = arguments.optNonNullString("prompt").trim()
                    require(prompt.isNotEmpty() && prompt.length <= 2000) { "invalid_device_gateway_call" }
                    val sourceChat = arguments.optNullableString("conversationId")
                    checkNotNull(phoneBridge).submitPhoneTask(prompt, sourceChat)
                    JSONObject()
                        .put("accepted", true)
                        .put("promptLength", prompt.length)
                }
                PhoneStopTool -> {
                    checkNotNull(phoneBridge).stopActiveTask()
                    JSONObject().put("stopped", true)
                }
                FabushiAppAgentSurface.StatusTool -> statusJson(surface.status())
                FabushiAppAgentSurface.SnapshotTool -> {
                    val maximum = arguments.optInt("maxElements", 500).coerceIn(1, 500)
                    snapshotJson(surface.snapshot(), maximum)
                }
                FabushiAppAgentSurface.FindTool -> {
                    val snapshot = surface.snapshot()
                    val ref = arguments.optNullableString("ref")
                    val agentId = ref?.let { parseRef(it, snapshot.generation) }
                        ?: arguments.optNullableString("agentId")
                    val name = arguments.optNullableString("name")
                        ?: arguments.optNullableString("text")
                    val matches = surface.find(
                        agentId = agentId,
                        role = arguments.optNullableString("role"),
                        name = name,
                        limit = arguments.optInt("limit", 25).coerceIn(1, 100),
                    )
                    JSONObject()
                        .put("version", FabushiAppAgentSurface.Version)
                        .put("appId", snapshot.appId)
                        .put("platform", snapshot.platform)
                        .put("screen", snapshot.screen)
                        .put("generation", snapshot.generation)
                        .put("matches", elementsJson(matches, snapshot.generation))
                }
                FabushiAppAgentSurface.ActionTool -> {
                    require(arguments.has("generation")) { "invalid_app_surface_generation" }
                    val generation = arguments.optLong("generation", -1)
                    require(generation >= 0) { "invalid_app_surface_generation" }
                    val agentId = arguments.optNullableString("ref")
                        ?.let { parseRef(it, generation) }
                        ?: arguments.optNullableString("agentId")
                        ?: error("app_surface_action_target_missing")
                    val action = arguments.optNullableString("action")
                        ?: error("invalid_device_gateway_call")
                    val actionValue = if (!arguments.has("value") || arguments.isNull("value")) {
                        null
                    } else {
                        arguments.optNonNullString("value")
                    }
                    snapshotJson(
                        surface.action(
                            expectedGeneration = generation,
                            agentId = agentId,
                            action = action,
                            value = actionValue,
                        ),
                        500,
                    )
                }
                FabushiAppAgentSurface.WaitTool -> assertionJson(
                    surface.waitFor(
                        expectedScreen = arguments.optNullableString("screen")
                            ?: arguments.optNullableString("route"),
                        agentId = arguments.optNullableString("agentId"),
                        role = arguments.optNullableString("role"),
                        name = arguments.optNullableString("name")
                            ?: arguments.optNullableString("text"),
                        state = arguments.optNonNullString("state", "present").ifBlank { "present" },
                        timeoutMilliseconds = arguments.optLong("timeoutMs", 10_000).coerceIn(100, 30_000),
                    ),
                )
                FabushiAppAgentSurface.AssertTool -> assertionJson(
                    surface.assertState(
                        expectedScreen = arguments.optNullableString("screen")
                            ?: arguments.optNullableString("route"),
                        agentId = arguments.optNullableString("agentId"),
                        role = arguments.optNullableString("role"),
                        name = arguments.optNullableString("name")
                            ?: arguments.optNullableString("text"),
                        state = arguments.optNonNullString("state", "present").ifBlank { "present" },
                    ),
                )
                else -> error("unknown_fabushi_app_tool")
            }
        }

    private fun stopConnection(reason: String) {
        val previousRegistered: Boolean
        val previousSocket: WebSocket?
        synchronized(stateLock) {
            previousRegistered = registered
            registered = false
            activeSession = null
            previousSocket = socket
            socket = null
        }
        previousSocket?.close(1001, reason.take(80))
        if (previousRegistered) appendTrace("disconnected", mapOf("reason" to reason.take(80)))
    }

    override fun close() {
        val jobToCancel: Job?
        synchronized(stateLock) {
            if (closed) return
            closed = true
            desiredLoggedIn = false
            jobToCancel = monitorJob
            monitorJob = null
        }
        jobToCancel?.cancel()
        stopConnection("stopped")
        scope.cancel()
        client.dispatcher.executorService.shutdown()
        client.connectionPool.evictAll()
        host.close()
    }

    private fun parseAgentSession(value: JSONObject): AgentSession {
        val accessToken = value.optNonNullString("accessToken").trim()
        val deviceId = value.optNonNullString("deviceId").trim()
        val sessionId = value.optNonNullString("sessionId").trim()
        val username = value.optNonNullString("username").ifBlank {
            value.optJSONObject("user")?.optNonNullString("username").orEmpty()
        }.take(200)
        val expiresAt = value.optLong("accessTokenExpiresAt", 0)
        require(accessToken.length in 24..(16 * 1024) && accessToken.none(Char::isWhitespace)) { "invalid_device_agent_session" }
        require(deviceId.matches(Regex("^[A-Za-z0-9._:-]{1,128}$"))) { "invalid_device_agent_session" }
        require(sessionId.isNotBlank() && sessionId.length <= 200) { "invalid_device_agent_session" }
        return AgentSession(accessToken, deviceId, sessionId, username, expiresAt)
    }

    internal fun toolDescriptors(): JSONArray {
        fun schema(vararg properties: Pair<String, Any>, required: List<String> = emptyList()): JSONObject {
            val objectValue = JSONObject().put("type", "object").put("properties", JSONObject())
            properties.forEach { (name, value) -> objectValue.getJSONObject("properties").put(name, value) }
            if (required.isNotEmpty()) objectValue.put("required", JSONArray(required))
            return objectValue
        }
        fun stringSchema(vararg values: String) = JSONObject().put("type", "string").also {
            if (values.isNotEmpty()) it.put("enum", JSONArray(values.toList()))
        }
        fun integerSchema() = JSONObject().put("type", "integer")
        fun booleanSchema() = JSONObject().put("type", "boolean")
        fun descriptor(name: String, title: String, description: String, input: JSONObject, readOnly: Boolean) =
            JSONObject()
                .put("name", name)
                .put("title", title)
                .put("description", description)
                .put("inputSchema", input)
                .put("annotations", JSONObject().put("readOnlyHint", readOnly))
        val stateProperties = arrayOf<Pair<String, Any>>(
            "route" to stringSchema(), "screen" to stringSchema(), "agentId" to stringSchema(),
            "role" to stringSchema(), "name" to stringSchema(), "text" to stringSchema(),
            "state" to stringSchema("present", "absent", "enabled", "disabled", "visible", "hidden"),
        )
        val arr = JSONArray()
            .put(descriptor(FabushiAppAgentSurface.StatusTool, "Fabushi app agent-surface status", "Report whether the active Fabushi Android application exposes its structured semantic surface.", schema(), true))
            .put(descriptor(FabushiAppAgentSurface.SnapshotTool, "Read the Fabushi semantic UI", "Return a structured redacted semantic snapshot of the active Fabushi Android UI.", schema("maxElements" to integerSchema(), "includeText" to booleanSchema()), true))
            .put(descriptor(FabushiAppAgentSurface.FindTool, "Find Fabushi UI elements", "Find semantic elements by stable id, generation-bound ref, role, accessible name, or visible text.", schema("agentId" to stringSchema(), "ref" to stringSchema(), "role" to stringSchema(), "name" to stringSchema(), "text" to stringSchema(), "limit" to integerSchema()), true))
            .put(descriptor(FabushiAppAgentSurface.ActionTool, "Operate the Fabushi semantic UI", "Perform one allowlisted action against the exact current semantic generation.", schema("generation" to integerSchema(), "ref" to stringSchema(), "agentId" to stringSchema(), "action" to stringSchema("invoke", "focus", "setValue", "pressKey", "scroll", "selectOption", "toggle"), "value" to stringSchema(), required = listOf("generation", "action")), false))
            .put(descriptor(FabushiAppAgentSurface.WaitTool, "Wait for Fabushi UI state", "Wait for a bounded semantic UI condition.", schema(*(stateProperties + ("timeoutMs" to integerSchema()))), true))
            .put(descriptor(FabushiAppAgentSurface.AssertTool, "Assert Fabushi UI state", "Evaluate a deterministic semantic UI assertion for CI evidence.", schema(*stateProperties), true))
        if (phoneBridge != null) {
            arr
                .put(descriptor(PhoneStatusTool, "Mika phone-control status", "Report Accessibility, Wireless ADB helper, VirtualDisplay, Capsule, and Sandbox readiness.", schema(), true))
                .put(descriptor(PhoneDispatchTool, "Dispatch Mika phone-control task", "Submit a ConfirmationGate-protected phone task to Mika TaskService.", schema("prompt" to stringSchema(), "conversationId" to stringSchema(), required = listOf("prompt")), false))
                .put(descriptor(PhoneStopTool, "Stop Mika phone-control task", "Immediately cancel the active Mika phone task.", schema(), false))
        }
        return arr
    }

    private fun statusJson(status: FabushiAppAgentSurface.Status) = JSONObject()
        .put("version", status.version).put("appId", status.appId).put("platform", status.platform)
        .put("available", status.available).put("screen", status.screen).put("generation", status.generation)

    private fun snapshotJson(snapshot: FabushiAppAgentSurface.Snapshot, maximum: Int) = JSONObject()
        .put("version", snapshot.version).put("appId", snapshot.appId).put("platform", snapshot.platform)
        .put("screen", snapshot.screen).put("generation", snapshot.generation)
        .put("elements", elementsJson(snapshot.elements.take(maximum), snapshot.generation))

    private fun assertionJson(assertion: FabushiAppAgentSurface.Assertion) = JSONObject()
        .put("passed", assertion.passed).put("screen", assertion.screen).put("generation", assertion.generation)
        .put("matches", elementsJson(assertion.matches, assertion.generation)).put("failures", JSONArray(assertion.failures))

    private fun elementsJson(elements: List<FabushiAppAgentSurface.Element>, generation: Long) = JSONArray().also { array ->
        elements.forEach { element ->
            val value = JSONObject()
                .put("agentId", element.agentId)
                .put("ref", "g$generation:${element.agentId}")
                .put("role", element.role)
                .put("name", element.name)
                .put("visible", element.visible)
                .put("enabled", element.enabled)
                .put("sensitive", element.sensitive)
            element.valuePresent?.let { value.put("valuePresent", it) }
            element.valueLength?.let { value.put("valueLength", it) }
            array.put(value)
        }
    }

    private fun parseRef(ref: String, expectedGeneration: Long): String {
        require(ref.startsWith("g")) { "invalid_app_surface_ref" }
        val separator = ref.indexOf(':')
        require(separator > 1) { "invalid_app_surface_ref" }
        require(ref.substring(1, separator).toLongOrNull() == expectedGeneration) { "stale_app_surface_generation" }
        return ref.substring(separator + 1).also { require(it.isNotBlank()) { "invalid_app_surface_ref" } }
    }

    private fun safeErrorCode(error: Throwable): String = when (val message = error.message.orEmpty()) {
        "unknown_fabushi_app_tool", "invalid_device_agent_session", "invalid_device_gateway_call",
        "invalid_app_surface_generation", "app_surface_action_target_missing", "invalid_app_surface_ref",
        "stale_app_surface_generation", "app_surface_element_not_found",
        "app_surface_target_hidden", "app_surface_target_disabled", "sensitive_app_surface_input_requires_secure_input",
        "app_surface_value_too_large", "app_surface_action_unavailable", "unsupported_app_surface_action" -> message
        else -> "transport_error:${error::class.java.simpleName.take(80)}"
    }

    private fun appendTrace(phase: String, values: Map<String, Any?> = emptyMap()) {
        runCatching {
            traceFile.parentFile?.mkdirs()
            val record = JSONObject().put("at", Instant.now().toString()).put("platform", "android").put("phase", phase)
            values.forEach { (key, value) -> record.put(key, value) }
            traceFile.appendText(record.toString() + "\n", Charsets.UTF_8)
        }
    }
}
