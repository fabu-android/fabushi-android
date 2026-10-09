package com.ombhrum.fabushi

import android.content.Context
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
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
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

/**
 * Account-scoped remote-device transport owned by the installed Android App.
 * Only the existing fabushi.app.* semantic surface is published; no arbitrary
 * shell, Kotlin/Java reflection, JavaScript execution, or credential writes are
 * exposed through this transport.
 */
internal class FabushiRemoteDeviceGateway(
    context: Context,
    private val coordinator: AndroidCoordinatorPort,
    private val surface: FabushiAppAgentSurface,
    private val metadata: Map<String, String>,
    private val configuredDeviceName: String? = null,
) : AutoCloseable {
    companion object {
        private const val OfficialGatewayUrl = "wss://fabushi-mcp.ombhrum.com/agent"
        private const val LeaseSeconds = 14_400
        private const val MaxMessageBytes = 32 * 1024 * 1024
    }

    private data class AgentSession(
        val accessToken: String,
        val deviceId: String,
        val sessionId: String,
        val username: String,
        val accessTokenExpiresAt: Long,
    )

    private val traceFile = File(context.getExternalFilesDir(null) ?: context.filesDir, "device-gateway-trace.jsonl")
    private val commandJournal = RemoteCommandJournal(File(context.filesDir, "remote-command-journal.properties"))
    private val commandJobs = ConcurrentHashMap<String, Job>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(25, TimeUnit.SECONDS)
        .build()
    private val stateLock = Any()
    private var desiredLoggedIn = false
    private var monitorJob: Job? = null
    private var socket: WebSocket? = null
    private var activeSession: AgentSession? = null
    private var registered = false

    fun setLoggedIn(loggedIn: Boolean) {
        synchronized(stateLock) { desiredLoggedIn = loggedIn }
        if (!loggedIn) {
            monitorJob?.cancel()
            monitorJob = null
            stopConnection("logged-out")
            return
        }
        if (monitorJob != null) return
        monitorJob = scope.launch {
            while (isActive) {
                refreshConnection()
                delay(10_000)
            }
        }
    }

    private suspend fun refreshConnection() {
        if (!synchronized(stateLock) { desiredLoggedIn }) return
        runCatching {
            parseAgentSession(coordinator.authDeviceAgentSession())
        }.onSuccess { candidate ->
            val current = synchronized(stateLock) { activeSession }
            val stillUsable = synchronized(stateLock) {
                current?.deviceId == candidate.deviceId &&
                    current.sessionId == candidate.sessionId &&
                    current.accessToken == candidate.accessToken &&
                    registered && socket != null
            }
            if (!stillUsable) connect(candidate)
        }.onFailure { error ->
            appendTrace("connection-refresh-failed", mapOf("error" to safeErrorCode(error)))
            stopConnection("refresh-failed")
        }
    }

    private fun connect(session: AgentSession) {
        stopConnection("reconnect")
        val request = Request.Builder()
            .url(OfficialGatewayUrl)
            .header("Authorization", "Bearer ${session.accessToken}")
            .build()
        synchronized(stateLock) {
            activeSession = session
            registered = false
        }
        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                synchronized(stateLock) { socket = webSocket }
                val registration = JSONObject()
                    .put("type", "register")
                    .put("deviceId", session.deviceId)
                    .put("name", configuredDeviceName ?: "Fabushi Android ${session.deviceId}".take(200))
                    .put("platform", "android")
                    .put("capabilities", JSONArray(FabushiAppAgentSurface.ToolNames))
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
                when (message.optString("type")) {
                    "registered" -> {
                        synchronized(stateLock) { registered = true }
                        appendTrace("registered", mapOf("deviceId" to message.optString("deviceId").take(128)))
                    }
                    "call" -> {
                        val requestId = message.optString("requestId").take(128)
                        if (requestId.isBlank()) return
                        lateinit var job: Job
                        job = scope.launch(start = CoroutineStart.LAZY) {
                            try {
                                handleCall(webSocket, message)
                            } finally {
                                commandJobs.remove(requestId, job)
                            }
                        }
                        if (commandJobs.putIfAbsent(requestId, job) == null) {
                            job.start()
                        } else {
                            job.cancel()
                        }
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
        synchronized(stateLock) { socket = client.newWebSocket(request, listener) }
    }

    private suspend fun handleCall(webSocket: WebSocket, message: JSONObject) {
        val requestId = message.optString("requestId").take(128)
        val toolName = message.optString("toolName").take(128)
        if (requestId.isBlank() || toolName.isBlank()) return
        val arguments = message.optJSONObject("arguments") ?: JSONObject()
        val session = synchronized(stateLock) { activeSession }
            ?: return sendRemoteError(webSocket, requestId, "device_agent_session_unavailable")

        val admission = runCatching {
            commandJournal.begin(requestId, session.deviceId, session.sessionId, toolName)
        }.getOrElse { error ->
            val safe = safeErrorCode(error)
            sendRemoteError(webSocket, requestId, safe)
            appendTrace("call-rejected", mapOf("requestId" to requestId, "toolName" to toolName, "error" to safe))
            return
        }
        if (admission is RemoteCommandAdmission.Replay) {
            val record = admission.record
            if (record.state == RemoteCommandState.COMPLETED && record.responseJson != null) {
                webSocket.send(record.responseJson)
            } else {
                sendRemoteError(webSocket, requestId, record.error ?: "remote_command_failed")
            }
            appendTrace("call-replayed", mapOf("requestId" to requestId, "toolName" to toolName, "state" to record.state.name))
            return
        }

        appendTrace("call-started", mapOf("requestId" to requestId, "toolName" to toolName, "sessionId" to session.sessionId.take(96)))
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
                val wire = JSONObject()
                    .put("type", "result")
                    .put("requestId", requestId)
                    .put("ok", true)
                    .put("result", result)
                    .toString()
                commandJournal.complete(requestId, wire)
                webSocket.send(wire)
                appendTrace("call-completed", mapOf("requestId" to requestId, "toolName" to toolName, "ok" to true))
            }
            .onFailure { error ->
                val safe = safeErrorCode(error)
                runCatching { commandJournal.fail(requestId, safe) }
                sendRemoteError(webSocket, requestId, safe)
                appendTrace(
                    "call-completed",
                    mapOf("requestId" to requestId, "toolName" to toolName, "ok" to false, "error" to safe),
                )
            }
    }

    private fun sendRemoteError(webSocket: WebSocket, requestId: String, error: String) {
        webSocket.send(
            JSONObject()
                .put("type", "result")
                .put("requestId", requestId)
                .put("ok", false)
                .put("error", error)
                .toString(),
        )
    }

    private suspend fun invokeTool(toolName: String, arguments: JSONObject): JSONObject =
        withContext(Dispatchers.Main.immediate) {
            require(toolName in FabushiAppAgentSurface.ToolNames) { "unknown_fabushi_app_tool" }
            when (toolName) {
                FabushiAppAgentSurface.StatusTool -> statusJson(surface.status())
                FabushiAppAgentSurface.SnapshotTool -> {
                    val maximum = arguments.optInt("maxElements", 500).coerceIn(1, 500)
                    snapshotJson(surface.snapshot(), maximum)
                }
                FabushiAppAgentSurface.FindTool -> {
                    val snapshot = surface.snapshot()
                    val ref = arguments.optString("ref").takeIf(String::isNotBlank)
                    val agentId = ref?.let { parseRef(it, snapshot.generation) }
                        ?: arguments.optString("agentId").takeIf(String::isNotBlank)
                    val name = arguments.optString("name").takeIf(String::isNotBlank)
                        ?: arguments.optString("text").takeIf(String::isNotBlank)
                    val matches = surface.find(
                        agentId = agentId,
                        role = arguments.optString("role").takeIf(String::isNotBlank),
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
                    val agentId = arguments.optString("ref").takeIf(String::isNotBlank)
                        ?.let { parseRef(it, generation) }
                        ?: arguments.optString("agentId").takeIf(String::isNotBlank)
                        ?: error("app_surface_action_target_missing")
                    val action = arguments.optString("action").takeIf(String::isNotBlank)
                        ?: error("invalid_device_gateway_call")
                    snapshotJson(
                        surface.action(
                            expectedGeneration = generation,
                            agentId = agentId,
                            action = action,
                            value = arguments.optString("value").takeIf { arguments.has("value") },
                        ),
                        500,
                    )
                }
                FabushiAppAgentSurface.WaitTool -> assertionJson(
                    surface.waitFor(
                        expectedScreen = arguments.optString("screen").takeIf(String::isNotBlank)
                            ?: arguments.optString("route").takeIf(String::isNotBlank),
                        agentId = arguments.optString("agentId").takeIf(String::isNotBlank),
                        role = arguments.optString("role").takeIf(String::isNotBlank),
                        name = arguments.optString("name").takeIf(String::isNotBlank)
                            ?: arguments.optString("text").takeIf(String::isNotBlank),
                        state = arguments.optString("state", "present"),
                        timeoutMilliseconds = arguments.optLong("timeoutMs", 10_000).coerceIn(100, 30_000),
                    ),
                )
                FabushiAppAgentSurface.AssertTool -> assertionJson(
                    surface.assertState(
                        expectedScreen = arguments.optString("screen").takeIf(String::isNotBlank)
                            ?: arguments.optString("route").takeIf(String::isNotBlank),
                        agentId = arguments.optString("agentId").takeIf(String::isNotBlank),
                        role = arguments.optString("role").takeIf(String::isNotBlank),
                        name = arguments.optString("name").takeIf(String::isNotBlank)
                            ?: arguments.optString("text").takeIf(String::isNotBlank),
                        state = arguments.optString("state", "present"),
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
        commandJobs.values.forEach(Job::cancel)
        commandJobs.clear()
        commandJournal.markPendingOutcomeUnknown("remote_connection_stopped_before_terminal_result")
        previousSocket?.close(1001, reason.take(80))
        if (previousRegistered) appendTrace("disconnected", mapOf("reason" to reason.take(80)))
    }

    override fun close() {
        monitorJob?.cancel()
        monitorJob = null
        stopConnection("stopped")
        scope.cancel()
        client.dispatcher.executorService.shutdown()
        client.connectionPool.evictAll()
    }

    private fun parseAgentSession(value: JSONObject): AgentSession {
        val accessToken = value.optString("accessToken").trim()
        val deviceId = value.optString("deviceId").trim()
        val sessionId = value.optString("sessionId").trim()
        val username = value.optString("username").ifBlank {
            value.optJSONObject("user")?.optString("username").orEmpty()
        }.take(200)
        val expiresAt = value.optLong("accessTokenExpiresAt", 0)
        require(accessToken.length in 24..(16 * 1024) && accessToken.none(Char::isWhitespace)) { "invalid_device_agent_session" }
        require(deviceId.matches(Regex("^[A-Za-z0-9._:-]{1,128}$"))) { "invalid_device_agent_session" }
        require(sessionId.isNotBlank() && sessionId.length <= 200) { "invalid_device_agent_session" }
        return AgentSession(accessToken, deviceId, sessionId, username, expiresAt)
    }

    private fun toolDescriptors(): JSONArray {
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
        return JSONArray()
            .put(descriptor(FabushiAppAgentSurface.StatusTool, "Fabushi app agent-surface status", "Report whether the active Fabushi Android application exposes its structured semantic surface.", schema(), true))
            .put(descriptor(FabushiAppAgentSurface.SnapshotTool, "Read the Fabushi semantic UI", "Return a structured redacted semantic snapshot of the active Fabushi Android UI.", schema("maxElements" to integerSchema(), "includeText" to booleanSchema()), true))
            .put(descriptor(FabushiAppAgentSurface.FindTool, "Find Fabushi UI elements", "Find semantic elements by stable id, generation-bound ref, role, accessible name, or visible text.", schema("agentId" to stringSchema(), "ref" to stringSchema(), "role" to stringSchema(), "name" to stringSchema(), "text" to stringSchema(), "limit" to integerSchema()), true))
            .put(descriptor(FabushiAppAgentSurface.ActionTool, "Operate the Fabushi semantic UI", "Perform one allowlisted action against the exact current semantic generation.", schema("generation" to integerSchema(), "ref" to stringSchema(), "agentId" to stringSchema(), "action" to stringSchema("invoke", "focus", "setValue", "pressKey", "scroll", "selectOption", "toggle"), "value" to stringSchema(), required = listOf("generation", "action")), false))
            .put(descriptor(FabushiAppAgentSurface.WaitTool, "Wait for Fabushi UI state", "Wait for a bounded semantic UI condition.", schema(*(stateProperties + ("timeoutMs" to integerSchema()))), true))
            .put(descriptor(FabushiAppAgentSurface.AssertTool, "Assert Fabushi UI state", "Evaluate a deterministic semantic UI assertion for CI evidence.", schema(*stateProperties), true))
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
        "remote_command_identity_session_mismatch", "remote_command_identity_tool_mismatch",
        "remote_command_already_pending", "remote_command_outcome_unknown_reconcile_required",
        "device_agent_session_unavailable", "remote_command_failed",
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
