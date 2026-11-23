package com.ombhrum.fabushi.e2e.harness

import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.EOFException
import java.io.InputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.net.URLDecoder
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import org.json.JSONArray
import org.json.JSONObject

data class RecordedRequest(
    val method: String,
    val path: String,
    val rawQuery: String,
    val headers: Map<String, String>,
    val body: String,
) {
    val authorization: String? get() = headers["authorization"]
    fun bodyJsonOrNull(): JSONObject? = runCatching { JSONObject(body) }.getOrNull()
    fun queryParam(name: String): String? =
        rawQuery.split('&')
            .filter { it.isNotEmpty() }
            .firstOrNull { it.substringBefore('=') == name }
            ?.substringAfter('=', "")
            ?.let { runCatching { URLDecoder.decode(it, "UTF-8") }.getOrDefault(it) }
}

data class FaultRule(
    val method: String? = null,
    val pathPrefix: String,
    val status: Int = 500,
    val errorBody: String = """{"error":"injected_fault","message":"Injected fault"}""",
    val delayMs: Long = 0L,
    val remainingHits: AtomicInteger = AtomicInteger(Int.MAX_VALUE),
    val dropConnection: Boolean = false,
)

/**
 * Pure-JDK stateful in-process `gpui_telegram_server` implementing HTTP/1.1 `/api/v1/...` AND
 * RFC 6455 WebSockets (`/api/v1/ws?token=...` and `/agent` for [com.ombhrum.fabushi.FabushiRemoteDeviceGateway])
 * on a single ephemeral loopback port.
 *
 * Enforces all 5 mandatory wire safeguards (`TEST_INFRA.md §3.2`):
 * 1. UTF-8 byte `Content-Length` (`bytes.size`, never `String.length`).
 * 2. Case-insensitive header parsing & exact bounded `Content-Length` reads (`0` when absent; chunked supported).
 * 3. Omits `Sec-WebSocket-Extensions` (`permessage-deflate`) in `101 Switching Protocols`.
 * 4. Full 7-bit (`<126`), 16-bit (`126`), and 64-bit (`127`) RFC 6455 payload lengths on read and write.
 * 5. Synchronized WS `OutputStream` writes per client socket, `tcpNoDelay = true`, full socket tracking & teardown.
 */
class MockGpuiTelegramServer : AutoCloseable {
    companion object {
        private const val WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

        fun closedLoopbackUrl(): String {
            val port = ServerSocket(0).use { it.localPort }
            return "http://127.0.0.1:$port"
        }

        fun isValidAssistCode(code: String): Boolean {
            if (code.length !in 8..16) return false
            var hasLetter = false
            var hasDigit = false
            for (ch in code) {
                when {
                    ch in 'a'..'z' || ch in 'A'..'Z' -> hasLetter = true
                    ch in '0'..'9' -> hasDigit = true
                    else -> return false
                }
            }
            return hasLetter && hasDigit
        }

        fun isValidBotUsername(username: String): Boolean {
            val clean = username.trim().trimStart('@')
            if (clean.length !in 4..32) return false
            if (clean[0] !in 'a'..'z' && clean[0] !in 'A'..'Z') return false
            if (!clean.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '_' }) return false
            return clean.lowercase().endsWith("bot")
        }
    }

    // ---- Stateful Data Models ----
    data class MockUser(
        val userId: Long,
        var username: String,
        var firstName: String,
        var lastName: String = "",
        var bio: String = "",
        var password: String = "password123",
    )

    data class MockSession(
        val sessionId: String,
        val userId: Long,
        val token: String,
        val deviceName: String = "Fabushi Android",
        val platform: String = "android",
        val ip: String = "127.0.0.1",
        val isCurrent: Boolean = true,
    )

    data class MockChat(
        val id: Long,
        val ownerUserId: Long,
        var title: String,
        var kind: String = "direct",
        var isPinned: Boolean = false,
        var isMuted: Boolean = false,
        var muteUntilMs: Long? = null,
        var isArchived: Boolean = false,
        var unreadCount: Int = 0,
        var ttlSeconds: Int = 0,
        var lastMessageDateMs: Long = 1_000L,
        var isBot: Boolean = false,
        var peerUserId: Long? = null,
        var readOutboxMaxId: Long = 0L,
        var draftText: String? = null,
        var draftReplyToId: Long? = null,
        val pinnedMessageIds: MutableList<Long> = mutableListOf(),
    )

    data class MockMessage(
        val id: Long,
        val chatId: Long,
        val senderId: Long,
        val senderName: String,
        var text: String,
        var contentType: String = "text",
        var dateMs: Long,
        var isEdited: Boolean = false,
        var replyTo: Long? = null,
        var clientNonce: String? = null,
        var fileId: String? = null,
        var filename: String? = null,
        var mimeType: String? = null,
        var sizeBytes: Int = 0,
        val reactions: MutableMap<String, Int> = LinkedHashMap(),
        val myReactions: MutableSet<String> = LinkedHashSet(),
        var inlineKeyboard: JSONArray? = null,
        var poll: JSONObject? = null,
    )

    data class MockFolder(
        val id: Long,
        val ownerUserId: Long,
        var title: String,
        var includeContacts: Boolean = false,
        var includeBots: Boolean = false,
        var includeGroups: Boolean = false,
        var includeChannels: Boolean = false,
        var excludeMuted: Boolean = false,
    )

    data class MockContact(
        val userId: Long,
        val ownerUserId: Long,
        val username: String,
        val displayName: String,
        val phone: String = "",
        val bio: String = "",
        val chatId: Long? = null,
    )

    data class MockBot(
        val botUserId: Long,
        val ownerUserId: Long,
        var agentId: String,
        var username: String,
        var displayName: String,
        var description: String = "",
        var aiPersona: String = "Private system persona prompt",
        var botToken: String,
        var executionState: String = "idle",
        var avatarShape: String = "circle",
        var avatarColor: String = "#007AFF",
        var preferredModel: String = "",
        var approvalMode: String = "ask_every_time",
        var companionChatId: Long,
        var isFabushi: Boolean = true,
        var boundNodeId: String? = null,
    )

    data class MockComputer(
        val nodeId: String,
        val ownerUserId: Long,
        var kind: String = "cloud",
        var displayName: String,
        var host: String = "",
        var sshPort: Int = 22,
        var osName: String = "Ubuntu 24.04",
        var resolution: String = "1920x1080",
        var online: Boolean = false,
        var runPolicy: String = "ask_every_time",
        var hasSavedCredential: Boolean = true,
        var testShouldSucceed: Boolean = true,
        var testFailureStderr: String = "Connection refused",
    )

    data class MockRemoteSession(
        val sessionId: String,
        val targetNodeId: String,
        val controllerKind: String = "human",
        val controllerName: String = "Fabushi Android",
        val relayMode: String = "desktop",
    )

    data class MockMcpGrant(
        val familyId: String,
        val clientName: String,
        val scopes: List<String> = listOf("remote.read", "remote.exec"),
        val allowedNodeIds: List<String> = emptyList(),
    )

    data class MockMedia(
        val fileId: String,
        val filename: String,
        val mimeType: String,
        val bytes: ByteArray,
    )

    data class MockAlipayAttempt(
        val attemptId: String,
        val loginUrl: String,
        var status: String = "pending",
        var httpStatusOverride: Int? = null,
        var targetUsername: String = "alice",
    )

    data class HttpResult(
        val status: Int = 200,
        val bodyBytes: ByteArray = "{}".toByteArray(Charsets.UTF_8),
        val contentType: String = "application/json; charset=utf-8",
    ) {
        companion object {
            fun json(body: String, status: Int = 200): HttpResult =
                HttpResult(status = status, bodyBytes = body.toByteArray(Charsets.UTF_8))

            fun json(obj: JSONObject, status: Int = 200): HttpResult = json(obj.toString(), status)

            fun error(status: Int, code: String, message: String): HttpResult =
                json(JSONObject().put("error", code).put("message", message), status)
        }
    }

    private class WsClientHandle(val path: String, val socket: Socket, val out: OutputStream) {
        private val writeLock = Any()

        fun sendText(text: String) {
            val payload = text.toByteArray(Charsets.UTF_8)
            synchronized(writeLock) {
                writeWsFrame(out, opcode = 0x1, payload = payload)
            }
        }

        fun sendPong(payload: ByteArray) {
            synchronized(writeLock) {
                writeWsFrame(out, opcode = 0xA, payload = payload)
            }
        }

        fun sendClose() {
            synchronized(writeLock) {
                runCatching { writeWsFrame(out, opcode = 0x8, payload = ByteArray(0)) }
            }
        }

        private fun writeWsFrame(stream: OutputStream, opcode: Int, payload: ByteArray) {
            val header = ByteArrayOutputStream()
            header.write(0x80 or (opcode and 0x0F))
            val len = payload.size.toLong()
            when {
                len < 126L -> header.write(len.toInt())
                len <= 65535L -> {
                    header.write(126)
                    header.write(((len shr 8) and 0xFF).toInt())
                    header.write((len and 0xFF).toInt())
                }
                else -> {
                    header.write(127)
                    for (shift in 56 downTo 0 step 8) {
                        header.write(((len shr shift) and 0xFF).toInt())
                    }
                }
            }
            stream.write(header.toByteArray())
            if (payload.isNotEmpty()) stream.write(payload)
            stream.flush()
        }
    }

    // ---- Networking & Lifecycle ----
    private val serverSocket = ServerSocket(0)
    private val running = AtomicBoolean(true)
    private val executor = Executors.newCachedThreadPool { r ->
        Thread(r, "fabushi-e2e-mock-srv").apply { isDaemon = true }
    }
    private val trackedSockets = CopyOnWriteArrayList<Socket>()
    private val wsClients = CopyOnWriteArrayList<WsClientHandle>()

    val port: Int get() = serverSocket.localPort
    val baseUrl: String get() = "http://127.0.0.1:$port"
    val wsBaseUrl: String get() = "ws://127.0.0.1:$port"

    // ---- Observability & Fault Injection ----
    val recordedRequests = CopyOnWriteArrayList<RecordedRequest>()
    val wsRequests = CopyOnWriteArrayList<RecordedRequest>()
    val agentRegisterFrames = CopyOnWriteArrayList<JSONObject>()
    val wsConnectCount = AtomicInteger(0)
    val agentWsConnectCount = AtomicInteger(0)
    private val faults = CopyOnWriteArrayList<FaultRule>()

    // ---- Stateful Store (Guarded by stateLock, NEVER held across delayMs sleeps!) ----
    private val stateLock = Any()
    private val idGen = AtomicLong(1000L)
    private val users = LinkedHashMap<Long, MockUser>()
    private val sessionsByToken = LinkedHashMap<String, MockSession>()
    private val chats = LinkedHashMap<Long, MockChat>()
    private val messagesByChat = LinkedHashMap<Long, MutableList<MockMessage>>()
    private val folders = LinkedHashMap<Long, MockFolder>()
    private val contacts = LinkedHashMap<Long, MockContact>()
    private val bots = LinkedHashMap<Long, MockBot>()
    private val computers = LinkedHashMap<String, MockComputer>()
    private val remoteSessions = LinkedHashMap<String, MockRemoteSession>()
    private val mcpGrants = LinkedHashMap<String, MockMcpGrant>()
    private val mediaStore = LinkedHashMap<String, MockMedia>()
    private val pollsById = LinkedHashMap<String, Pair<Long, Long>>() // pollId -> (chatId, msgId)
    private val alipayAttempts = LinkedHashMap<String, MockAlipayAttempt>()
    private val knownDirectoryUsers = LinkedHashMap<String, MockUser>()
    private val localCommandsQueue = mutableListOf<JSONObject>()
    private val userSettingsByUser = LinkedHashMap<Long, JSONObject>()

    @Volatile var overrideNextLoginToken: String? = null
    @Volatile var codingAgentProbes: JSONArray = JSONArray()
        .put(JSONObject().put("provider_kind", "claude_code").put("installed", false))
        .put(JSONObject().put("provider_kind", "codex").put("installed", false))
        .put(JSONObject().put("provider_kind", "open_code").put("installed", false))
        .put(JSONObject().put("provider_kind", "cursor_cli").put("installed", false))
        .put(JSONObject().put("provider_kind", "antigravity").put("installed", false))
        .put(JSONObject().put("provider_kind", "acp_custom").put("installed", false))

    @Volatile var assistId: String = "987654321"
    @Volatile var assistAllow: Boolean = false
    @Volatile var assistCodeMode: String = "temporary"
    @Volatile var assistCode: String = "Init1234"
    @Volatile var currentPts: Long = 10L
    @Volatile var callbackActionToast: String = "Action applied"
    @Volatile var callbackUpdatedText: String? = "Updated via callback"
    @Volatile var callbackUpdatedKeyboard: JSONArray? = null

    init {
        synchronized(stateLock) {
            users[1L] = MockUser(1L, "alice", "Alice", "", "Alice bio", "password123")
            users[2L] = MockUser(2L, "bob", "Bob", "", "Bob bio", "password123")
            knownDirectoryUsers["bob"] = users[2L]!!
            knownDirectoryUsers["charlie"] = MockUser(3L, "charlie", "Charlie", "", "Charlie bio")
        }
        executor.execute { acceptLoop() }
    }

    // ---- Test Seeding & Control Helpers ----

    fun addFault(rule: FaultRule) {
        faults.add(rule)
    }

    fun clearFaults() {
        faults.clear()
    }

    fun clearRecordedRequests() {
        recordedRequests.clear()
    }

    fun seedChat(chat: MockChat) = synchronized(stateLock) {
        chats[chat.id] = chat
        messagesByChat.putIfAbsent(chat.id, mutableListOf())
    }

    fun seedMessage(msg: MockMessage) = synchronized(stateLock) {
        messagesByChat.computeIfAbsent(msg.chatId) { mutableListOf() }.add(msg)
        chats[msg.chatId]?.let { c ->
            if (msg.dateMs > c.lastMessageDateMs) c.lastMessageDateMs = msg.dateMs
        }
    }

    fun seedBot(bot: MockBot) = synchronized(stateLock) {
        bots[bot.botUserId] = bot
    }

    fun seedComputer(comp: MockComputer) = synchronized(stateLock) {
        computers[comp.nodeId] = comp
    }

    fun seedRemoteSession(session: MockRemoteSession) = synchronized(stateLock) {
        remoteSessions[session.sessionId] = session
    }

    fun seedMcpGrant(grant: MockMcpGrant) = synchronized(stateLock) {
        mcpGrants[grant.familyId] = grant
    }

    fun seedSession(session: MockSession) = synchronized(stateLock) {
        sessionsByToken[session.token] = session
    }

    fun seedContact(contact: MockContact) = synchronized(stateLock) {
        contacts[contact.userId] = contact
    }

    fun seedFolder(folder: MockFolder) = synchronized(stateLock) {
        folders[folder.id] = folder
    }

    fun seedMedia(media: MockMedia) = synchronized(stateLock) {
        mediaStore[media.fileId] = media
    }

    fun queueLocalCommand(cmd: JSONObject) = synchronized(stateLock) {
        localCommandsQueue.add(cmd)
    }

    fun setAlipayAttemptStatus(attemptId: String, status: String, httpStatusOverride: Int? = null) =
        synchronized(stateLock) {
            alipayAttempts[attemptId]?.let {
                it.status = status
                it.httpStatusOverride = httpStatusOverride
            }
        }

    fun revokeAllTokensForUser(userId: Long) = synchronized(stateLock) {
        val toRemove = sessionsByToken.entries.filter { it.value.userId == userId }.map { it.key }
        toRemove.forEach { sessionsByToken.remove(it) }
    }

    fun revokeToken(token: String) = synchronized(stateLock) {
        sessionsByToken.remove(token)
    }

    fun getChatSnapshot(chatId: Long): MockChat? = synchronized(stateLock) {
        chats[chatId]?.let { c -> c.copy(pinnedMessageIds = c.pinnedMessageIds.toMutableList()) }
    }

    fun getMessageSnapshot(chatId: Long, messageId: Long): MockMessage? = synchronized(stateLock) {
        messagesByChat[chatId]?.firstOrNull { it.id == messageId }?.let { m ->
            m.copy(
                reactions = LinkedHashMap(m.reactions),
                myReactions = LinkedHashSet(m.myReactions),
            )
        }
    }

    fun pushWsEvent(json: String) {
        wsClients.filter { it.path == "/api/v1/ws" }.forEach { client ->
            runCatching { client.sendText(json) }
        }
    }

    fun pushAgentWsMessage(json: String) {
        wsClients.filter { it.path == "/agent" }.forEach { client ->
            runCatching { client.sendText(json) }
        }
    }

    fun dropAllWebSockets() {
        val snapshot = wsClients.toList()
        wsClients.clear()
        snapshot.forEach { client ->
            runCatching { client.socket.close() }
        }
    }

    // ---- Socket Accept & HTTP/WS Framing ----

    private fun acceptLoop() {
        while (running.get()) {
            val socket = try {
                serverSocket.accept()
            } catch (_: Exception) {
                break
            }
            socket.tcpNoDelay = true
            trackedSockets.add(socket)
            executor.execute { handleConnection(socket) }
        }
    }

    private fun handleConnection(socket: Socket) {
        try {
            val input = BufferedInputStream(socket.getInputStream())
            val output = socket.getOutputStream()
            val req = readHttpRequest(input) ?: return

            val upgrade = req.headers["upgrade"]
            if (upgrade != null && upgrade.equals("websocket", ignoreCase = true)) {
                handleWebSocketUpgrade(socket, req, input, output)
                return
            }

            recordedRequests.add(req)

            // Evaluate matching fault rule BEFORE acquiring stateLock so delayMs NEVER blocks concurrent requests!
            val matchedFault = matchFault(req)
            if (matchedFault != null) {
                if (matchedFault.delayMs > 0L) {
                    Thread.sleep(matchedFault.delayMs)
                }
                if (matchedFault.dropConnection) {
                    socket.close()
                    return
                }
                if (matchedFault.status > 0) {
                    writeHttpResponse(
                        output,
                        HttpResult.json(matchedFault.errorBody, matchedFault.status),
                    )
                    return
                }
            }

            val resp = dispatchHttp(req)
            writeHttpResponse(output, resp)
        } catch (_: Exception) {
            // Connection closed by peer or fault injection
        } finally {
            trackedSockets.remove(socket)
            runCatching { socket.close() }
        }
    }

    private fun matchFault(req: RecordedRequest): FaultRule? {
        for (rule in faults) {
            val methodOk = rule.method == null || rule.method.equals(req.method, ignoreCase = true)
            if (methodOk && req.path.startsWith(rule.pathPrefix)) {
                while (true) {
                    val rem = rule.remainingHits.get()
                    if (rem <= 0) break
                    if (rule.remainingHits.compareAndSet(rem, rem - 1)) {
                        return rule
                    }
                }
            }
        }
        return null
    }

    private fun readHttpRequest(input: BufferedInputStream): RecordedRequest? {
        val requestLine = readAsciiLine(input) ?: return null
        if (requestLine.isEmpty()) return null
        val parts = requestLine.split(' ')
        if (parts.size < 2) return null
        val method = parts[0].uppercase()
        val rawTarget = parts[1]
        val qIdx = rawTarget.indexOf('?')
        val path = if (qIdx >= 0) rawTarget.substring(0, qIdx) else rawTarget
        val rawQuery = if (qIdx >= 0) rawTarget.substring(qIdx + 1) else ""

        // Safeguard #2: case-insensitive header keys + exact bounded body read
        val headers = LinkedHashMap<String, String>()
        while (true) {
            val line = readAsciiLine(input) ?: break
            if (line.isEmpty()) break
            val colon = line.indexOf(':')
            if (colon > 0) {
                val k = line.substring(0, colon).trim().lowercase()
                val v = line.substring(colon + 1).trim()
                headers[k] = v
            }
        }

        val transferEncoding = headers["transfer-encoding"]?.lowercase().orEmpty()
        val bodyBytes = when {
            transferEncoding.contains("chunked") -> readChunkedBody(input)
            else -> {
                val contentLength = headers["content-length"]?.toIntOrNull() ?: 0
                if (contentLength > 0) readExact(input, contentLength) else ByteArray(0)
            }
        }
        return RecordedRequest(
            method = method,
            path = path,
            rawQuery = rawQuery,
            headers = headers,
            body = String(bodyBytes, Charsets.UTF_8),
        )
    }

    private fun readChunkedBody(input: BufferedInputStream): ByteArray {
        val out = ByteArrayOutputStream()
        while (true) {
            val sizeLine = readAsciiLine(input)?.trim() ?: break
            val size = sizeLine.substringBefore(';').trim().toIntOrNull(16) ?: break
            if (size <= 0) {
                readAsciiLine(input)
                break
            }
            out.write(readExact(input, size))
            readAsciiLine(input) // trailing CRLF
        }
        return out.toByteArray()
    }

    private fun readAsciiLine(input: InputStream): String? {
        val buf = ByteArrayOutputStream()
        while (true) {
            val b = input.read()
            if (b == -1) return if (buf.size() == 0) null else buf.toString("ISO-8859-1")
            if (b == '\n'.code) break
            if (b != '\r'.code) buf.write(b)
        }
        return buf.toString("ISO-8859-1")
    }

    private fun readExact(input: InputStream, count: Int): ByteArray {
        val buf = ByteArray(count)
        var off = 0
        while (off < count) {
            val n = input.read(buf, off, count - off)
            if (n <= 0) throw EOFException("Unexpected EOF after $off/$count bytes")
            off += n
        }
        return buf
    }

    // Safeguard #1: UTF-8 byte Content-Length (`bytes.size`)
    private fun writeHttpResponse(output: OutputStream, resp: HttpResult) {
        val reason = when (resp.status) {
            200 -> "OK"
            201 -> "Created"
            204 -> "No Content"
            400 -> "Bad Request"
            401 -> "Unauthorized"
            403 -> "Forbidden"
            404 -> "Not Found"
            409 -> "Conflict"
            410 -> "Gone"
            500 -> "Internal Server Error"
            else -> "Status"
        }
        val header = buildString {
            append("HTTP/1.1 ").append(resp.status).append(' ').append(reason).append("\r\n")
            append("Content-Type: ").append(resp.contentType).append("\r\n")
            append("Content-Length: ").append(resp.bodyBytes.size).append("\r\n")
            append("Connection: close\r\n\r\n")
        }
        output.write(header.toByteArray(Charsets.ISO_8859_1))
        if (resp.bodyBytes.isNotEmpty()) output.write(resp.bodyBytes)
        output.flush()
    }

    // Safeguards #3, #4, #5: RFC 6455 handshake (no Sec-WebSocket-Extensions), 7/16/64-bit frames, sync writes
    private fun handleWebSocketUpgrade(
        socket: Socket,
        req: RecordedRequest,
        input: BufferedInputStream,
        output: OutputStream,
    ) {
        wsRequests.add(req)
        if (req.path == "/api/v1/ws") {
            val token = req.queryParam("token")?.trim()
            val validSession = synchronized(stateLock) {
                !token.isNullOrEmpty() && sessionsByToken.containsKey(token)
            }
            if (!validSession) {
                writeHttpResponse(output, HttpResult.error(401, "unauthorized", "Invalid or revoked WebSocket token"))
                return
            }
        }
        val key = req.headers["sec-websocket-key"] ?: run {
            writeHttpResponse(output, HttpResult.error(400, "bad_ws", "Missing Sec-WebSocket-Key"))
            return
        }
        val accept = Base64.getEncoder().encodeToString(
            MessageDigest.getInstance("SHA-1").digest((key + WS_GUID).toByteArray(Charsets.ISO_8859_1)),
        )
        // Safeguard #3: deliberately OMIT Sec-WebSocket-Extensions so OkHttp 5.4.0 uses uncompressed frames!
        val handshake = buildString {
            append("HTTP/1.1 101 Switching Protocols\r\n")
            append("Upgrade: websocket\r\n")
            append("Connection: Upgrade\r\n")
            append("Sec-WebSocket-Accept: ").append(accept).append("\r\n\r\n")
        }
        output.write(handshake.toByteArray(Charsets.ISO_8859_1))
        output.flush()

        val handle = WsClientHandle(req.path, socket, output)
        wsClients.add(handle)
        if (req.path == "/agent") {
            agentWsConnectCount.incrementAndGet()
        } else {
            wsConnectCount.incrementAndGet()
        }

        try {
            while (running.get() && !socket.isClosed) {
                val b0 = input.read()
                if (b0 == -1) break
                val b1 = input.read()
                if (b1 == -1) break
                val opcode = b0 and 0x0F
                val masked = (b1 and 0x80) != 0
                val len7 = b1 and 0x7F
                val payloadLen: Long = when (len7) {
                    126 -> {
                        val ext = readExact(input, 2)
                        (((ext[0].toInt() and 0xFF) shl 8) or (ext[1].toInt() and 0xFF)).toLong()
                    }
                    127 -> {
                        val ext = readExact(input, 8)
                        var v = 0L
                        for (i in 0 until 8) {
                            v = (v shl 8) or (ext[i].toLong() and 0xFFL)
                        }
                        v
                    }
                    else -> len7.toLong()
                }
                val maskKey = if (masked) readExact(input, 4) else null
                val payload = if (payloadLen > 0L) readExact(input, payloadLen.toInt()) else ByteArray(0)
                if (maskKey != null) {
                    for (i in payload.indices) {
                        payload[i] = (payload[i].toInt() xor maskKey[i % 4].toInt()).toByte()
                    }
                }
                when (opcode) {
                    0x1 -> {
                        val text = String(payload, Charsets.UTF_8)
                        handleWsClientText(req.path, text, handle)
                    }
                    0x8 -> {
                        handle.sendClose()
                        break
                    }
                    0x9 -> handle.sendPong(payload)
                    else -> Unit
                }
            }
        } catch (_: Exception) {
            // Peer closed or dropAllWebSockets() closed socket
        } finally {
            wsClients.remove(handle)
        }
    }

    private fun handleWsClientText(path: String, text: String, handle: WsClientHandle) {
        val obj = runCatching { JSONObject(text) }.getOrNull() ?: return
        val type = obj.optString("type")
        if (path == "/agent") {
            if (type == "register") {
                agentRegisterFrames.add(obj)
                handle.sendText(
                    JSONObject()
                        .put("type", "registered")
                        .put("deviceId", obj.optString("deviceId"))
                        .toString(),
                )
            }
        } else {
            if (type == "Ping") {
                handle.sendText("""{"type":"Pong","payload":{}}""")
            }
        }
    }

    // ---- Stateful HTTP `/api/v1/*` Router ----

    private fun authenticate(req: RecordedRequest): MockSession? {
        val auth = req.authorization ?: return null
        if (!auth.startsWith("Bearer ", ignoreCase = true)) return null
        val token = auth.substring(7).trim()
        return sessionsByToken[token]
    }

    private fun mintToken(userId: Long): String {
        val custom = overrideNextLoginToken
        if (custom != null) {
            overrideNextLoginToken = null
            return custom
        }
        val seq = idGen.incrementAndGet()
        // Always >= 24 ASCII non-whitespace chars so FabushiRemoteDeviceGateway bridges cleanly!
        return "tok_e2e_session_u${userId}_${seq}_0123456789abcdef"
    }

    private fun createSessionLocked(user: MockUser, deviceName: String): MockSession {
        val sid = "sess-${user.userId}-${idGen.incrementAndGet()}"
        val tok = mintToken(user.userId)
        val sess = MockSession(sessionId = sid, userId = user.userId, token = tok, deviceName = deviceName, isCurrent = true)
        sessionsByToken[tok] = sess
        return sess
    }

    private fun dispatchHttp(req: RecordedRequest): HttpResult = synchronized(stateLock) {
        val path = req.path
        val method = req.method
        val body = req.bodyJsonOrNull() ?: JSONObject()

        // ---- Unauthenticated Auth & Health Routes ----
        if (method == "GET" && path == "/api/v1/status") {
            return@synchronized HttpResult.json("""{"status":"ok","version":"1.6.6"}""")
        }
        if (method == "POST" && path == "/api/v1/auth/login") {
            val username = body.optString("username").trim().ifBlank { return@synchronized HttpResult.error(400, "invalid", "Username required") }
            val user = users.values.firstOrNull { it.username.equals(username, ignoreCase = true) }
                ?: MockUser(idGen.incrementAndGet(), username, username.replaceFirstChar { it.uppercase() }).also { users[it.userId] = it }
            val sess = createSessionLocked(user, body.optString("device_name", "Fabushi Android"))
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", user.userId)
                    .put("username", user.username)
                    .put("first_name", user.firstName)
                    .put("session_id", sess.sessionId)
                    .put("token", sess.token),
            )
        }
        if (method == "POST" && (path == "/api/v1/auth/register" || path == "/api/v1/auth/email/register")) {
            val username = body.optString("username").trim().ifBlank { return@synchronized HttpResult.error(400, "invalid", "Username required") }
            val firstName = body.optString("first_name").ifBlank { username }
            val uid = idGen.incrementAndGet()
            val user = MockUser(uid, username, firstName, password = body.optString("password", "pw"))
            users[uid] = user
            knownDirectoryUsers[username.lowercase()] = user
            val sess = createSessionLocked(user, body.optString("device_name", "Fabushi Android"))
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", user.userId)
                    .put("username", user.username)
                    .put("first_name", user.firstName)
                    .put("session_id", sess.sessionId)
                    .put("token", sess.token),
            )
        }
        if (method == "POST" && path == "/api/v1/auth/email/send-code") {
            val email = body.optString("email").trim()
            if (email.isBlank() || !email.contains('@')) {
                return@synchronized HttpResult.error(400, "invalid_email", "Valid email required")
            }
            return@synchronized HttpResult.json(JSONObject().put("sent", true).put("email", email).put("expires_in_seconds", 300))
        }
        if (method == "POST" && path == "/api/v1/auth/alipay/start") {
            val aid = "alipay-${idGen.incrementAndGet()}"
            val url = "https://auth.alipay.com/login/index.htm?attempt_id=$aid"
            alipayAttempts[aid] = MockAlipayAttempt(attemptId = aid, loginUrl = url, status = "pending")
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("attempt_id", aid)
                    .put("login_url", url)
                    .put("status", "pending"),
            )
        }
        if (method == "GET" && path.startsWith("/api/v1/auth/alipay/poll/")) {
            val aid = path.removePrefix("/api/v1/auth/alipay/poll/")
            val attempt = alipayAttempts[aid] ?: return@synchronized HttpResult.error(404, "not_found", "Alipay attempt not found")
            attempt.httpStatusOverride?.let { code ->
                return@synchronized HttpResult.error(code, "alipay_error", "Alipay attempt status $code")
            }
            if (attempt.status == "completed" || attempt.status == "complete") {
                val user = users.values.firstOrNull { it.username == attempt.targetUsername } ?: users[1L]!!
                val sess = createSessionLocked(user, "Alipay OAuth")
                return@synchronized HttpResult.json(
                    JSONObject()
                        .put("attempt_id", aid)
                        .put("status", "completed")
                        .put("token", sess.token)
                        .put("user_id", user.userId)
                        .put("username", user.username)
                        .put("first_name", user.firstName)
                        .put("session_id", sess.sessionId),
                )
            }
            return@synchronized HttpResult.json(JSONObject().put("attempt_id", aid).put("status", attempt.status))
        }

        // ---- Protected Routes (Require Bearer Token -> 401 otherwise) ----
        val session = authenticate(req)
            ?: return@synchronized HttpResult.error(401, "unauthorized", "Invalid or expired bearer token")
        val me = users[session.userId]
            ?: return@synchronized HttpResult.error(401, "unauthorized", "Session user missing")

        // Account, Sessions, Profile & Settings
        if (method == "GET" && path == "/api/v1/auth/me") {
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", me.userId)
                    .put("username", me.username)
                    .put("first_name", me.firstName)
                    .put("last_name", me.lastName)
                    .put("bio", me.bio)
                    .put("session_id", session.sessionId),
            )
        }
        if (method == "PUT" && (path == "/api/v1/auth/profile" || path == "/api/v1/users/me")) {
            body.optString("first_name").takeIf { it.isNotBlank() }?.let { me.firstName = it }
            body.optString("last_name").takeIf { it.isNotBlank() }?.let { me.lastName = it }
            body.optString("username").takeIf { it.isNotBlank() }?.let { me.username = it }
            if (body.has("bio")) me.bio = body.optString("bio")
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", me.userId)
                    .put("username", me.username)
                    .put("first_name", me.firstName)
                    .put("last_name", me.lastName)
                    .put("bio", me.bio),
            )
        }
        if (path == "/api/v1/users/me/settings") {
            if (method == "GET") {
                return@synchronized HttpResult.json(userSettingsByUser[me.userId] ?: JSONObject())
            }
            if (method == "PUT" || method == "PATCH") {
                val merged = if (method == "PATCH") JSONObject((userSettingsByUser[me.userId] ?: JSONObject()).toString()) else JSONObject()
                body.keys().forEach { k -> merged.put(k, body.get(k)) }
                userSettingsByUser[me.userId] = merged
                return@synchronized HttpResult.json(merged)
            }
        }
        if (method == "GET" && path == "/api/v1/auth/sessions") {
            val arr = JSONArray()
            sessionsByToken.values.filter { it.userId == me.userId }.forEach { s ->
                arr.put(
                    JSONObject()
                        .put("session_id", s.sessionId)
                        .put("device_name", s.deviceName)
                        .put("platform", s.platform)
                        .put("ip", s.ip)
                        .put("is_current", s.sessionId == session.sessionId)
                        .put("last_active_ms", System.currentTimeMillis()),
                )
            }
            return@synchronized HttpResult.json(JSONObject().put("sessions", arr))
        }
        if (method == "DELETE" && path == "/api/v1/auth/sessions/others") {
            val toDrop = sessionsByToken.entries.filter { it.value.userId == me.userId && it.value.sessionId != session.sessionId }.map { it.key }
            toDrop.forEach { sessionsByToken.remove(it) }
            return@synchronized HttpResult.json(JSONObject().put("revoked_count", toDrop.size))
        }
        if (method == "DELETE" && path.startsWith("/api/v1/auth/sessions/")) {
            val sid = path.removePrefix("/api/v1/auth/sessions/")
            val key = sessionsByToken.entries.firstOrNull { it.value.sessionId == sid }?.key
                ?: return@synchronized HttpResult.error(404, "not_found", "Session $sid not found")
            sessionsByToken.remove(key)
            return@synchronized HttpResult.json(JSONObject().put("revoked", true).put("session_id", sid))
        }

        // Chats & Pinning
        if (method == "GET" && path == "/api/v1/chats") {
            val arr = JSONArray()
            chats.values.filter { it.ownerUserId == me.userId }.forEach { c ->
                arr.put(chatToJson(c))
            }
            return@synchronized HttpResult.json(JSONObject().put("chats", arr).put("total", arr.length()))
        }
        if (method == "POST" && (path == "/api/v1/chats/create" || path == "/api/v1/chats")) {
            val title = body.optString("title").trim().ifBlank { return@synchronized HttpResult.error(400, "invalid", "Title required") }
            val cid = idGen.incrementAndGet()
            val kind = body.optString("kind").ifBlank { body.optString("chat_type", "group") }
            val chat = MockChat(
                id = cid,
                ownerUserId = me.userId,
                title = title,
                kind = kind,
                lastMessageDateMs = System.currentTimeMillis(),
            )
            chats[cid] = chat
            messagesByChat.putIfAbsent(cid, mutableListOf())
            return@synchronized HttpResult.json(chatToJson(chat))
        }

        // Chat-scoped routes `/api/v1/chats/{id}/...`
        if (path.startsWith("/api/v1/chats/")) {
            val rest = path.removePrefix("/api/v1/chats/")
            val segs = rest.split('/')
            val cid = segs[0].toLongOrNull()
            if (cid != null) {
                val chat = chats[cid]
                val sub = segs.getOrNull(1)
                when {
                    sub == "pin" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.isPinned = body.optBoolean("pinned", true)
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("is_pinned", chat.isPinned)
                                .put("is_muted", chat.isMuted)
                                .put("is_archived", chat.isArchived),
                        )
                    }
                    sub == "mute" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.isMuted = body.optBoolean("muted", true)
                        chat.muteUntilMs = if (chat.isMuted && body.has("mute_until_ms")) body.optLong("mute_until_ms") else null
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("is_muted", chat.isMuted)
                                .put("mute_until_ms", chat.muteUntilMs ?: JSONObject.NULL),
                        )
                    }
                    sub == "archive" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.isArchived = body.optBoolean("archived", true)
                        return@synchronized HttpResult.json(JSONObject().put("chat_id", cid).put("is_archived", chat.isArchived))
                    }
                    sub == "read" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.unreadCount = 0
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("chat_id", cid).put("unread_count", 0))
                    }
                    sub == "clear" && method == "POST" -> {
                        messagesByChat[cid]?.clear()
                        chat?.pinnedMessageIds?.clear()
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("chat_id", cid))
                    }
                    sub == "leave" && method == "POST" -> {
                        chats.remove(cid)
                        messagesByChat.remove(cid)
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("chat_id", cid))
                    }
                    sub == "ttl" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.ttlSeconds = body.optInt("ttl_seconds", 0)
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("chat_id", cid).put("ttl_seconds", chat.ttlSeconds))
                    }
                    sub == "statistics" && method == "GET" -> {
                        val msgs = messagesByChat[cid].orEmpty()
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("subscribers", 42)
                                .put("totalMessages", msgs.size)
                                .put("pinnedMessages", chat?.pinnedMessageIds?.size ?: 0)
                                .put("avgViewsPerPost", msgs.size)
                                .put("daily_views", JSONArray().put(10).put(15).put(22)),
                        )
                    }
                    sub == "pins" && method == "GET" -> {
                        val ids = chat?.pinnedMessageIds.orEmpty()
                        val pinsArr = JSONArray()
                        ids.forEach { mid -> pinsArr.put(JSONObject().put("message_id", mid).put("chat_id", cid)) }
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("pinned_message_ids", JSONArray(ids))
                                .put("pins", pinsArr),
                        )
                    }
                    sub == "pins" && method == "POST" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        val mid = body.optLong("message_id", 0L)
                        if (mid <= 0L) return@synchronized HttpResult.error(400, "invalid", "message_id required")
                        if (mid !in chat.pinnedMessageIds) chat.pinnedMessageIds.add(mid)
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("pinned_message_ids", JSONArray(chat.pinnedMessageIds)),
                        )
                    }
                    sub == "pins" && method == "DELETE" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        val pathMid = segs.getOrNull(2)?.toLongOrNull()
                        val queryMid = req.queryParam("message_id")?.toLongOrNull()
                        val targetMid = pathMid ?: queryMid
                        if (targetMid != null) {
                            chat.pinnedMessageIds.remove(targetMid)
                        } else {
                            chat.pinnedMessageIds.clear()
                        }
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("pinned_message_ids", JSONArray(chat.pinnedMessageIds)),
                        )
                    }
                    sub == "messages" && segs.size == 2 && method == "GET" -> {
                        val all = messagesByChat[cid].orEmpty().sortedBy { it.id }
                        val beforeId = req.queryParam("before_id")?.toLongOrNull()
                        val limit = req.queryParam("limit")?.toIntOrNull()?.coerceAtLeast(1) ?: 50
                        val eligible = if (beforeId != null) all.filter { it.id < beforeId } else all
                        val page = if (eligible.size > limit) eligible.takeLast(limit) else eligible
                        val hasMore = eligible.size > page.size
                        val arr = JSONArray()
                        page.forEach { m -> arr.put(messageToJson(m, chat?.pinnedMessageIds.orEmpty())) }
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("has_more", hasMore)
                                .put("messages", arr),
                        )
                    }
                    sub == "messages" && segs.size == 3 && method == "PUT" -> {
                        val mid = segs[2].toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad message_id")
                        val msg = messagesByChat[cid]?.firstOrNull { it.id == mid }
                            ?: return@synchronized HttpResult.error(404, "not_found", "Message $mid not found")
                        msg.text = body.optString("text", msg.text)
                        msg.isEdited = true
                        return@synchronized HttpResult.json(messageToJson(msg, chat?.pinnedMessageIds.orEmpty()).put("ok", true))
                    }
                    sub == "messages" && segs.size == 3 && method == "DELETE" -> {
                        val mid = segs[2].toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad message_id")
                        messagesByChat[cid]?.removeAll { it.id == mid }
                        chat?.pinnedMessageIds?.remove(mid)
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("deleted_id", mid))
                    }
                    sub == "messages" && segs.size == 4 && segs[3] == "reactions" && method == "POST" -> {
                        val mid = segs[2].toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad message_id")
                        val msg = messagesByChat[cid]?.firstOrNull { it.id == mid }
                            ?: return@synchronized HttpResult.error(404, "not_found", "Message $mid not found")
                        val emoji = body.optString("emoji").ifBlank { return@synchronized HttpResult.error(400, "invalid", "Emoji required") }
                        if (emoji in msg.myReactions) {
                            msg.myReactions.remove(emoji)
                            val next = (msg.reactions[emoji] ?: 1) - 1
                            if (next <= 0) msg.reactions.remove(emoji) else msg.reactions[emoji] = next
                        } else {
                            msg.myReactions.add(emoji)
                            msg.reactions[emoji] = (msg.reactions[emoji] ?: 0) + 1
                        }
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("reactions", reactionsToJson(msg)))
                    }
                    sub == "draft" && method == "GET" -> {
                        val text = chat?.draftText
                        val draftObj: Any = if (text != null) {
                            JSONObject().put("text", text).apply {
                                chat.draftReplyToId?.let { put("reply_to_msg_id", it) }
                            }
                        } else JSONObject.NULL
                        return@synchronized HttpResult.json(JSONObject().put("chat_id", cid).put("draft", draftObj))
                    }
                    sub == "draft" && method == "PUT" -> {
                        if (chat == null) return@synchronized HttpResult.error(404, "not_found", "Chat not found")
                        chat.draftText = body.optString("text", "")
                        chat.draftReplyToId = body.optLong("reply_to_msg_id").takeIf { it > 0L }
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("chat_id", cid)
                                .put("draft", JSONObject().put("text", chat.draftText)),
                        )
                    }
                    sub == "draft" && method == "DELETE" -> {
                        chat?.draftText = null
                        chat?.draftReplyToId = null
                        return@synchronized HttpResult.json(JSONObject().put("ok", true).put("chat_id", cid))
                    }
                    sub == "polls" && method == "POST" -> {
                        val question = body.optString("question").trim()
                        val opts = body.optJSONArray("options") ?: JSONArray()
                        val pollId = "poll-${idGen.incrementAndGet()}"
                        val mid = idGen.incrementAndGet()
                        val optObjs = JSONArray()
                        for (i in 0 until opts.length()) {
                            optObjs.put(
                                JSONObject()
                                    .put("id", "opt-${i + 1}")
                                    .put("text", opts.optString(i))
                                    .put("voter_count", 0)
                                    .put("chosen", false),
                            )
                        }
                        val pollObj = JSONObject()
                            .put("id", pollId)
                            .put("question", question)
                            .put("options", optObjs)
                            .put("is_anonymous", body.optBoolean("is_anonymous", true))
                            .put("multiple_choice", body.optBoolean("multiple_choice", false))
                            .put("is_quiz", body.optBoolean("is_quiz", false))
                        if (body.has("correct_option_id")) pollObj.put("correct_option_id", body.optInt("correct_option_id"))
                        if (body.has("solution")) pollObj.put("solution", body.optString("solution"))
                        val msg = MockMessage(
                            id = mid,
                            chatId = cid,
                            senderId = me.userId,
                            senderName = me.firstName,
                            text = "📊 $question",
                            contentType = "poll",
                            dateMs = System.currentTimeMillis(),
                            poll = pollObj,
                        )
                        messagesByChat.computeIfAbsent(cid) { mutableListOf() }.add(msg)
                        pollsById[pollId] = cid to mid
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("message_id", mid)
                                .put("poll_id", pollId)
                                .put("poll", pollObj),
                        )
                    }
                    sub == "callback" && method == "POST" -> {
                        val mid = body.optLong("message_id", 0L)
                        val updatedText = callbackUpdatedText
                        if (mid > 0L && updatedText != null) {
                            messagesByChat[cid]?.firstOrNull { it.id == mid }?.let { m ->
                                m.text = updatedText
                                callbackUpdatedKeyboard?.let { m.inlineKeyboard = it }
                            }
                        }
                        val resp = JSONObject().put("toast", callbackActionToast)
                        updatedText?.let { resp.put("updated_text", it) }
                        callbackUpdatedKeyboard?.let { resp.put("inline_keyboard", it) }
                        return@synchronized HttpResult.json(resp)
                    }
                }
            }
        }

        // Send Message (idempotent by client_nonce!)
        if (method == "POST" && path == "/api/v1/messages/send") {
            val cid = body.optLong("chat_id", 0L)
            if (cid <= 0L) return@synchronized HttpResult.error(400, "invalid", "chat_id required")
            val nonce = body.optString("client_nonce").ifBlank { null }
            val list = messagesByChat.computeIfAbsent(cid) { mutableListOf() }
            if (nonce != null) {
                val existing = list.firstOrNull { it.clientNonce == nonce }
                if (existing != null) {
                    return@synchronized HttpResult.json(
                        JSONObject()
                            .put("message_id", existing.id)
                            .put("id", existing.id)
                            .put("pts", currentPts)
                            .put("date_ms", existing.dateMs)
                            .put("status", "sent"),
                    )
                }
            }
            val mid = idGen.incrementAndGet()
            val nowMs = System.currentTimeMillis()
            val mediaId = body.optString("media_id").ifBlank { null }
            val media = mediaId?.let { mediaStore[it] }
            val replyTo = body.optLong("reply_to").takeIf { it > 0L }
                ?: body.optLong("reply_to_id").takeIf { it > 0L }
            val msg = MockMessage(
                id = mid,
                chatId = cid,
                senderId = me.userId,
                senderName = me.firstName,
                text = body.optString("text", ""),
                contentType = when {
                    media != null && media.mimeType.startsWith("image/") -> "photo"
                    media != null && media.mimeType.startsWith("audio/") -> "voice"
                    media != null -> "document"
                    else -> "text"
                },
                dateMs = nowMs,
                replyTo = replyTo,
                clientNonce = nonce,
                fileId = mediaId,
                filename = media?.filename,
                mimeType = media?.mimeType,
                sizeBytes = media?.bytes?.size ?: 0,
            )
            list.add(msg)
            chats[cid]?.let { it.lastMessageDateMs = nowMs }
            currentPts += 1L
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("message_id", mid)
                    .put("id", mid)
                    .put("pts", currentPts)
                    .put("date_ms", nowMs)
                    .put("status", "sent"),
            )
        }

        // Polls vote/close
        if (method == "POST" && path.startsWith("/api/v1/polls/") && path.endsWith("/vote")) {
            val pollId = path.removePrefix("/api/v1/polls/").removeSuffix("/vote")
            val (cid, mid) = pollsById[pollId] ?: return@synchronized HttpResult.error(404, "not_found", "Poll not found")
            val msg = messagesByChat[cid]?.firstOrNull { it.id == mid }
            val pollObj = msg?.poll ?: return@synchronized HttpResult.error(404, "not_found", "Poll payload missing")
            val chosen = body.optJSONArray("option_ids") ?: JSONArray()
            val chosenSet = (0 until chosen.length()).map { chosen.optInt(it) }.toSet()
            val opts = pollObj.getJSONArray("options")
            for (i in 0 until opts.length()) {
                val o = opts.getJSONObject(i)
                val isChosen = i in chosenSet
                o.put("chosen", isChosen)
                o.put("voter_count", if (isChosen) 1 else 0)
            }
            return@synchronized HttpResult.json(JSONObject().put("poll", pollObj).put("ok", true))
        }
        if (method == "POST" && path.startsWith("/api/v1/polls/") && path.endsWith("/close")) {
            val pollId = path.removePrefix("/api/v1/polls/").removeSuffix("/close")
            val (cid, mid) = pollsById[pollId] ?: return@synchronized HttpResult.error(404, "not_found", "Poll not found")
            val pollObj = messagesByChat[cid]?.firstOrNull { it.id == mid }?.poll
                ?: return@synchronized HttpResult.error(404, "not_found", "Poll missing")
            pollObj.put("closed", true)
            return@synchronized HttpResult.json(JSONObject().put("poll", pollObj).put("ok", true))
        }

        // Folders
        if (method == "GET" && path == "/api/v1/folders") {
            val arr = JSONArray()
            folders.values.filter { it.ownerUserId == me.userId }.forEach { arr.put(folderToJson(it)) }
            return@synchronized HttpResult.json(JSONObject().put("folders", arr))
        }
        if (method == "POST" && path == "/api/v1/folders") {
            val fid = idGen.incrementAndGet()
            val folder = MockFolder(
                id = fid,
                ownerUserId = me.userId,
                title = body.optString("title", "Folder"),
                includeContacts = body.optBoolean("include_contacts", false),
                includeBots = body.optBoolean("include_bots", false),
                includeGroups = body.optBoolean("include_groups", false),
                includeChannels = body.optBoolean("include_channels", false),
                excludeMuted = body.optBoolean("exclude_muted", false),
            )
            folders[fid] = folder
            return@synchronized HttpResult.json(folderToJson(folder))
        }
        if (method == "PUT" && path.startsWith("/api/v1/folders/")) {
            val fid = path.removePrefix("/api/v1/folders/").toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad folder id")
            val folder = folders[fid] ?: return@synchronized HttpResult.error(404, "not_found", "Folder not found")
            folder.title = body.optString("title", folder.title)
            return@synchronized HttpResult.json(folderToJson(folder))
        }
        if (method == "DELETE" && path.startsWith("/api/v1/folders/")) {
            val fid = path.removePrefix("/api/v1/folders/").toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad folder id")
            folders.remove(fid)
            return@synchronized HttpResult.json(JSONObject().put("ok", true).put("id", fid))
        }

        // Contacts & Peer Resolve
        if (method == "GET" && path == "/api/v1/contacts") {
            val arr = JSONArray()
            contacts.values.filter { it.ownerUserId == me.userId }.forEach { c ->
                arr.put(
                    JSONObject()
                        .put("user_id", c.userId)
                        .put("username", c.username)
                        .put("display_name", c.displayName)
                        .put("phone", c.phone)
                        .put("bio", c.bio)
                        .apply { c.chatId?.let { put("chat_id", it) } },
                )
            }
            return@synchronized HttpResult.json(JSONObject().put("contacts", arr))
        }
        if (method == "POST" && (path == "/api/v1/contacts/add" || path == "/api/v1/contacts")) {
            val uname = body.optString("username").trim().trimStart('@').lowercase()
            val target = knownDirectoryUsers[uname]
                ?: return@synchronized HttpResult.error(404, "user_not_found", "User @$uname not found")
            val contact = MockContact(
                userId = target.userId,
                ownerUserId = me.userId,
                username = target.username,
                displayName = target.firstName,
                bio = target.bio,
            )
            contacts[target.userId] = contact
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", target.userId)
                    .put("username", target.username)
                    .put("display_name", target.firstName),
            )
        }
        if (method == "DELETE" && path.startsWith("/api/v1/contacts/")) {
            val uid = path.removePrefix("/api/v1/contacts/").toLongOrNull() ?: return@synchronized HttpResult.error(400, "invalid", "Bad user id")
            contacts.remove(uid)
            return@synchronized HttpResult.json(JSONObject().put("ok", true).put("user_id", uid))
        }
        if (method == "GET" && path == "/api/v1/peers/resolve") {
            val uname = req.queryParam("username").orEmpty().trim().trimStart('@').lowercase()
            val target = knownDirectoryUsers[uname]
                ?: return@synchronized HttpResult.error(404, "not_found", "Peer @$uname not found")
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("user_id", target.userId)
                    .put("username", target.username)
                    .put("first_name", target.firstName),
            )
        }

        // Media Upload & Download
        if (method == "POST" && path == "/api/v1/media/upload") {
            val b64 = body.optString("data_base64")
            if (b64.isBlank()) return@synchronized HttpResult.error(400, "invalid", "data_base64 required")
            val rawBytes = Base64.getDecoder().decode(b64)
            val fid = "med_${idGen.incrementAndGet()}"
            val filename = body.optString("filename").ifBlank { req.queryParam("filename") ?: "file.bin" }
            val mimeType = body.optString("mime_type").ifBlank { req.queryParam("mime_type") ?: "application/octet-stream" }
            mediaStore[fid] = MockMedia(fid, filename, mimeType, rawBytes)
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("file_id", fid)
                    .put("media_id", fid)
                    .put("filename", filename)
                    .put("mime_type", mimeType)
                    .put("size_bytes", rawBytes.size),
            )
        }
        if (method == "GET" && path.startsWith("/api/v1/media/")) {
            val fid = path.removePrefix("/api/v1/media/")
            val media = mediaStore[fid] ?: return@synchronized HttpResult.error(404, "not_found", "Media $fid not found")
            return@synchronized HttpResult(status = 200, bodyBytes = media.bytes, contentType = media.mimeType)
        }

        // Search & PTS Sync
        if (method == "GET" && path == "/api/v1/search/messages") {
            val q = req.queryParam("q").orEmpty().lowercase()
            val filterChat = req.queryParam("chat_id")?.toLongOrNull()
            val arr = JSONArray()
            messagesByChat.forEach { (cid, msgs) ->
                if (filterChat != null && cid != filterChat) return@forEach
                val chat = chats[cid]
                if (chat != null && chat.ownerUserId != me.userId) return@forEach
                msgs.filter { q.isEmpty() || it.text.lowercase().contains(q) }.forEach { m ->
                    arr.put(messageToJson(m, chat?.pinnedMessageIds.orEmpty()))
                }
            }
            return@synchronized HttpResult.json(JSONObject().put("messages", arr).put("total", arr.length()))
        }
        if ((method == "POST" || method == "GET") && path == "/api/v1/sync/difference") {
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("current_pts", currentPts)
                    .put("new_pts", currentPts)
                    .put("has_more", false)
                    .put("snapshot_required", false)
                    .put("messages", JSONArray())
                    .put("updates", JSONArray()),
            )
        }

        // ---- Real Bots & Coding Agents ----
        if (method == "GET" && path == "/api/v1/bots/my") {
            val arr = JSONArray()
            bots.values.filter { it.ownerUserId == me.userId && !it.isFabushi }.forEach { arr.put(botToJson(it)) }
            return@synchronized HttpResult.json(JSONObject().put("bots", arr).put("total", arr.length()))
        }
        if (method == "POST" && path == "/api/v1/bots") {
            val displayName = body.optString("display_name").trim()
            val username = body.optString("username").trim().trimStart('@')
            if (displayName.isBlank() || !isValidBotUsername(username)) {
                return@synchronized HttpResult.error(400, "invalid_bot_username", "Bot username '$username' must be 4..32 ASCII letters/digits/underscore starting with a letter and ending in 'bot'")
            }
            val botId = idGen.incrementAndGet()
            val chatId = idGen.incrementAndGet()
            val token = "srv_bot_${botId}_${idGen.incrementAndGet()}"
            val bot = MockBot(
                botUserId = botId,
                ownerUserId = me.userId,
                agentId = "bot-$botId",
                username = username,
                displayName = displayName,
                description = body.optString("description"),
                botToken = token,
                avatarShape = body.optString("avatar_shape", "circle"),
                avatarColor = body.optString("avatar_color", "#007AFF"),
                companionChatId = chatId,
                isFabushi = false,
            )
            bots[botId] = bot
            chats[chatId] = MockChat(chatId, me.userId, displayName, "direct", lastMessageDateMs = System.currentTimeMillis(), isBot = true, peerUserId = botId)
            return@synchronized HttpResult.json(botToJson(bot).put("bot_token", token))
        }
        if (method == "GET" && path == "/api/v1/fabushi-bots") {
            val arr = JSONArray()
            bots.values.filter { it.ownerUserId == me.userId && it.isFabushi }.forEach { arr.put(botToJson(it)) }
            return@synchronized HttpResult.json(JSONObject().put("bots", arr).put("total", arr.length()))
        }
        if (method == "POST" && path == "/api/v1/fabushi-bots") {
            val displayName = body.optString("display_name").trim()
            val rawUsername = body.optString("username").trim().trimStart('@')
            val username = rawUsername.ifBlank { "fabushi_${idGen.incrementAndGet()}_bot" }
            if (displayName.isBlank() || !isValidBotUsername(username)) {
                return@synchronized HttpResult.error(400, "invalid_bot_username", "Invalid Fabushi bot username '$username'")
            }
            val botId = idGen.incrementAndGet()
            val chatId = idGen.incrementAndGet()
            val token = "srv_fbot_${botId}_${idGen.incrementAndGet()}"
            val bot = MockBot(
                botUserId = botId,
                ownerUserId = me.userId,
                agentId = body.optString("agent_id").ifBlank { "ag-$botId" },
                username = username,
                displayName = displayName,
                description = body.optString("description"),
                botToken = token,
                avatarShape = body.optString("avatar_shape", "hexagon"),
                avatarColor = body.optString("avatar_color", "#34C759"),
                preferredModel = body.optString("preferred_model", ""),
                approvalMode = body.optString("approval_mode", "ask_every_time"),
                companionChatId = chatId,
                isFabushi = true,
            )
            bots[botId] = bot
            chats[chatId] = MockChat(chatId, me.userId, displayName, "direct", lastMessageDateMs = System.currentTimeMillis(), isBot = true, peerUserId = botId)
            return@synchronized HttpResult.json(botToJson(bot).put("bot_token", token))
        }
        if (path.startsWith("/api/v1/fabushi-bots/")) {
            val bidStr = path.removePrefix("/api/v1/fabushi-bots/")
            val bot = bots.values.firstOrNull { it.botUserId.toString() == bidStr || it.agentId == bidStr }
                ?: return@synchronized HttpResult.error(404, "not_found", "Fabushi bot $bidStr not found")
            if (method == "GET") return@synchronized HttpResult.json(botToJson(bot))
            if (method == "PATCH") {
                if (bot.ownerUserId != me.userId) {
                    return@synchronized HttpResult.error(403, "forbidden", "Only the bot owner can patch bot profile")
                }
                body.optString("display_name").takeIf { it.isNotBlank() }?.let { bot.displayName = it }
                body.optString("description").takeIf { it.isNotBlank() }?.let { bot.description = it }
                body.optString("avatar_shape").takeIf { it.isNotBlank() }?.let { bot.avatarShape = it }
                body.optString("avatar_color").takeIf { it.isNotBlank() }?.let { bot.avatarColor = it }
                body.optString("preferred_model").takeIf { it.isNotBlank() }?.let { bot.preferredModel = it }
                body.optString("approval_mode").takeIf { it.isNotBlank() }?.let { bot.approvalMode = it }
                return@synchronized HttpResult.json(botToJson(bot))
            }
        }
        if (method == "GET" && path.startsWith("/api/v1/bots/profile/")) {
            val target = path.removePrefix("/api/v1/bots/profile/").trimStart('@')
            val bot = bots.values.firstOrNull { it.username.equals(target, ignoreCase = true) || it.botUserId.toString() == target }
                ?: return@synchronized HttpResult.error(404, "not_found", "Bot profile '$target' not found")
            val json = botToJson(bot)
            if (bot.ownerUserId == me.userId) {
                json.put("ai_persona", bot.aiPersona)
            }
            // Non-owners strictly NEVER receive ai_persona!
            return@synchronized HttpResult.json(json)
        }
        if (method == "POST" && path == "/api/v1/botfather/nl-create") {
            val prompt = body.optString("prompt").trim()
            if (prompt.isBlank()) return@synchronized HttpResult.error(400, "invalid", "Prompt required")
            val botId = idGen.incrementAndGet()
            val chatId = idGen.incrementAndGet()
            val token = "srv_nlbot_${botId}_${idGen.incrementAndGet()}"
            val bot = MockBot(
                botUserId = botId,
                ownerUserId = me.userId,
                agentId = "nl-$botId",
                username = "nl_${botId}_bot",
                displayName = "NL Assistant $botId",
                description = prompt,
                botToken = token,
                avatarShape = body.optString("avatar_shape", "star"),
                avatarColor = body.optString("avatar_color", "#AF52DE"),
                companionChatId = chatId,
                isFabushi = true,
            )
            bots[botId] = bot
            chats[chatId] = MockChat(chatId, me.userId, bot.displayName, "direct", lastMessageDateMs = System.currentTimeMillis(), isBot = true, peerUserId = botId)
            return@synchronized HttpResult.json(botToJson(bot).put("bot_token", token))
        }
        if (method == "POST" && path == "/api/v1/coding-agents/probe") {
            return@synchronized HttpResult.json(JSONObject().put("probes", codingAgentProbes))
        }
        if (path.startsWith("/api/v1/bots/") && path.endsWith("/computer")) {
            val bidStr = path.removePrefix("/api/v1/bots/").removeSuffix("/computer")
            val bot = bots.values.firstOrNull { it.botUserId.toString() == bidStr || it.agentId == bidStr }
                ?: return@synchronized HttpResult.error(404, "not_found", "Bot $bidStr not found")
            if (method == "GET") {
                return@synchronized HttpResult.json(
                    JSONObject()
                        .put("bot_id", bot.botUserId.toString())
                        .put("bound_node_id", bot.boundNodeId ?: JSONObject.NULL),
                )
            }
            if (method == "PUT") {
                val clear = body.optBoolean("clear_bound_node", false)
                bot.boundNodeId = if (clear) null else body.optString("bound_node_id").ifBlank { null }
                return@synchronized HttpResult.json(
                    JSONObject()
                        .put("bot_id", bot.botUserId.toString())
                        .put("bound_node_id", bot.boundNodeId ?: JSONObject.NULL),
                )
            }
        }

        // ---- Real Remote Control, Computers, Assist, Sessions & MCP Grants ----
        if (method == "GET" && path == "/api/v1/computers") {
            val arr = JSONArray()
            computers.values.filter { it.ownerUserId == me.userId }.forEach { arr.put(computerToJson(it)) }
            return@synchronized HttpResult.json(JSONObject().put("computers", arr))
        }
        if (method == "POST" && path == "/api/v1/computers/cloud") {
            val host = body.optString("host").trim()
            if (host.isBlank()) {
                return@synchronized HttpResult.error(400, "invalid_host", "Cloud computer host must not be empty")
            }
            val displayName = body.optString("display_name").trim().ifBlank { host }
            val nodeId = body.optString("node_id").ifBlank { "cloud:${me.userId}:node-${idGen.incrementAndGet()}" }
            val comp = MockComputer(
                nodeId = nodeId,
                ownerUserId = me.userId,
                kind = "cloud",
                displayName = displayName,
                host = host,
                sshPort = body.optInt("port", 22),
                osName = "Ubuntu 24.04",
                resolution = "1920x1080",
                online = body.optBoolean("verify_connection", false),
                runPolicy = "ask_every_time",
                hasSavedCredential = body.optString("secret").isNotBlank(),
            )
            computers[nodeId] = comp
            // NEVER echo raw SSH secret!
            return@synchronized HttpResult.json(computerToJson(comp))
        }
        if (method == "POST" && path == "/api/v1/computers/local/register") {
            val deviceId = body.optString("device_id").ifBlank { req.headers["x-device-id"] ?: "local-device" }
            val nodeId = "local:$deviceId"
            val comp = MockComputer(
                nodeId = nodeId,
                ownerUserId = me.userId,
                kind = "local",
                displayName = body.optString("display_name", "Fabushi Android"),
                host = body.optString("ip_address", "127.0.0.1"),
                osName = body.optString("os_name", "Android"),
                resolution = body.optString("resolution", "1080x2400"),
                online = true,
                runPolicy = "ask_every_time",
                hasSavedCredential = false,
            )
            computers[nodeId] = comp
            return@synchronized HttpResult.json(computerToJson(comp))
        }
        // R-1: MUST be GET /api/v1/computers/local/poll!
        if (method == "GET" && path == "/api/v1/computers/local/poll") {
            val arr = JSONArray()
            localCommandsQueue.forEach { arr.put(it) }
            localCommandsQueue.clear()
            return@synchronized HttpResult.json(JSONObject().put("commands", arr))
        }
        if (method == "POST" && path.startsWith("/api/v1/computers/commands/") && path.endsWith("/approve")) {
            val cmdId = path.removePrefix("/api/v1/computers/commands/").removeSuffix("/approve")
            return@synchronized HttpResult.json(JSONObject().put("command_id", cmdId).put("approved", body.optBoolean("approved", true)))
        }
        if (method == "POST" && path.startsWith("/api/v1/computers/commands/") && path.endsWith("/complete")) {
            val cmdId = path.removePrefix("/api/v1/computers/commands/").removeSuffix("/complete")
            return@synchronized HttpResult.json(JSONObject().put("command_id", cmdId).put("completed", true))
        }
        if (method == "POST" && path == "/api/v1/computers/exec") {
            val nodeId = body.optString("node_id")
            val comp = computers[nodeId] ?: return@synchronized HttpResult.error(404, "not_found", "Computer $nodeId not found")
            if (comp.runPolicy == "disabled") {
                return@synchronized HttpResult.error(403, "policy_disabled", "Computer $nodeId run_policy is disabled")
            }
            val op = body.optJSONObject("operation") ?: JSONObject()
            val opType = op.optString("op")
            val cmdText = op.optString("command", opType)
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("command_id", "exec-${idGen.incrementAndGet()}")
                    .put("node_id", nodeId)
                    .put("ok", true)
                    .put("exit_code", 0)
                    .put("stdout", "server-exec-stdout: $cmdText")
                    .put("stderr", "")
                    .put("entries", JSONArray())
                    .put("executed_by", "ssh")
                    .put("duration_ms", 12),
            )
        }
        if (path.startsWith("/api/v1/computers/")) {
            val rest = path.removePrefix("/api/v1/computers/")
            when {
                method == "PATCH" && rest.endsWith("/policy") -> {
                    val nodeId = rest.removeSuffix("/policy")
                    val comp = computers[nodeId] ?: return@synchronized HttpResult.error(404, "not_found", "Computer $nodeId not found")
                    body.optString("run_policy").takeIf { it.isNotBlank() }?.let { comp.runPolicy = it }
                    body.optString("display_name").takeIf { it.isNotBlank() }?.let { comp.displayName = it }
                    return@synchronized HttpResult.json(computerToJson(comp))
                }
                method == "POST" && rest.endsWith("/test") -> {
                    val nodeId = rest.removeSuffix("/test")
                    val comp = computers[nodeId] ?: return@synchronized HttpResult.error(404, "not_found", "Computer $nodeId not found")
                    if (comp.testShouldSucceed) {
                        comp.online = true
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("command_id", "test-${idGen.incrementAndGet()}")
                                .put("node_id", nodeId)
                                .put("ok", true)
                                .put("exit_code", 0)
                                .put("stdout", "SSH handshake OK")
                                .put("stderr", "")
                                .put("entries", JSONArray())
                                .put("executed_by", "ssh")
                                .put("duration_ms", 9),
                        )
                    } else {
                        comp.online = false
                        return@synchronized HttpResult.json(
                            JSONObject()
                                .put("command_id", "test-${idGen.incrementAndGet()}")
                                .put("node_id", nodeId)
                                .put("ok", false)
                                .put("exit_code", 255)
                                .put("stdout", "")
                                .put("stderr", comp.testFailureStderr)
                                .put("entries", JSONArray())
                                .put("executed_by", "ssh")
                                .put("duration_ms", 15),
                        )
                    }
                }
                method == "DELETE" && !rest.contains('/') -> {
                    computers.remove(rest)
                    return@synchronized HttpResult.json(JSONObject().put("deleted", true).put("node_id", rest))
                }
            }
        }

        // UU-Remote Assist
        if (method == "GET" && path == "/api/v1/remote/assist/info") {
            return@synchronized HttpResult.json(assistInfoJson())
        }
        if (method == "POST" && path == "/api/v1/remote/assist/allow") {
            assistAllow = body.optBoolean("allow_assist", body.optBoolean("allowAssist", false))
            return@synchronized HttpResult.json(assistInfoJson())
        }
        if (method == "POST" && path == "/api/v1/remote/assist/reset-code") {
            assistCode = "Tmp${idGen.incrementAndGet().toString().takeLast(5)}A"
            assistCodeMode = "temporary"
            return@synchronized HttpResult.json(assistInfoJson())
        }
        if (method == "POST" && path == "/api/v1/remote/assist/set-code") {
            val code = body.optString("code")
            // Spec §8.4: MUST be 8..16 ASCII alphanumeric with BOTH letters AND digits
            if (!isValidAssistCode(code)) {
                return@synchronized HttpResult.error(
                    400,
                    "invalid_assist_code",
                    "Assist code must be 8..16 ASCII alphanumeric containing both letters and digits",
                )
            }
            assistCode = code
            assistCodeMode = "custom"
            return@synchronized HttpResult.json(assistInfoJson())
        }
        if (method == "POST" && path == "/api/v1/remote/assist/start") {
            val reqAssistId = body.optString("assist_id")
            val reqCode = body.optString("access_code")
            if (reqAssistId != assistId) {
                return@synchronized HttpResult.error(404, "assist_id_not_found", "Unknown assist_id $reqAssistId")
            }
            if (!assistAllow) {
                return@synchronized HttpResult.error(403, "assist_disabled", "Remote assist is disabled on target host")
            }
            if (reqCode != assistCode) {
                return@synchronized HttpResult.error(403, "invalid_access_code", "Invalid remote assist access code")
            }
            val rsId = "rsess-${idGen.incrementAndGet()}"
            val nodeId = computers.keys.firstOrNull() ?: "cloud:${me.userId}:assist"
            val rs = MockRemoteSession(rsId, nodeId, "human", body.optString("requester_name", "Fabushi Android"), body.optString("mode", "desktop"))
            remoteSessions[rsId] = rs
            return@synchronized HttpResult.json(
                JSONObject()
                    .put("session_id", rsId)
                    .put("target_node_id", nodeId)
                    .put("target_device_id", "dev-remote-1")
                    .put("temp_relay_token", "rly_tok_${idGen.incrementAndGet()}")
                    .put("requires_host_confirm", false),
            )
        }
        if (method == "GET" && path == "/api/v1/remote/sessions") {
            val arr = JSONArray()
            remoteSessions.values.forEach { s ->
                arr.put(
                    JSONObject()
                        .put("session_id", s.sessionId)
                        .put("target_node_id", s.targetNodeId)
                        .put("controller_kind", s.controllerKind)
                        .put("controller_name", s.controllerName)
                        .put("relay_mode", s.relayMode),
                )
            }
            return@synchronized HttpResult.json(JSONObject().put("sessions", arr))
        }
        if (method == "POST" && path == "/api/v1/remote/sessions/disconnect") {
            val sid = body.optString("session_id")
            remoteSessions.remove(sid)
            return@synchronized HttpResult.json(JSONObject().put("disconnected", true).put("session_id", sid))
        }
        if (method == "POST" && path == "/api/v1/remote/sessions/disconnect-all") {
            val count = remoteSessions.size
            remoteSessions.clear()
            return@synchronized HttpResult.json(JSONObject().put("disconnected_count", count))
        }
        if (method == "POST" && path == "/api/v1/remote/webrtc/signal") {
            return@synchronized HttpResult.json(JSONObject().put("accepted", true))
        }

        // OAuth 2.1 MCP Grants
        if (method == "GET" && path == "/api/v1/mcp/grants") {
            val arr = JSONArray()
            mcpGrants.values.forEach { g ->
                arr.put(
                    JSONObject()
                        .put("family_id", g.familyId)
                        .put("client_name", g.clientName)
                        .put("scopes", JSONArray(g.scopes))
                        .put("allowed_node_ids", JSONArray(g.allowedNodeIds)),
                )
            }
            return@synchronized HttpResult.json(JSONObject().put("grants", arr))
        }
        if (method == "DELETE" && path.startsWith("/api/v1/mcp/grants/")) {
            val fid = path.removePrefix("/api/v1/mcp/grants/")
            mcpGrants.remove(fid)
            return@synchronized HttpResult.json(JSONObject().put("revoked", true).put("family_id", fid))
        }
        if (method == "GET" && path == "/api/v1/mcp/audit") {
            return@synchronized HttpResult.json(JSONObject().put("entries", JSONArray()))
        }

        HttpResult.error(404, "not_found", "Unhandled route $method $path")
    }

    // ---- JSON Serializers ----

    private fun chatToJson(c: MockChat): JSONObject = JSONObject()
        .put("id", c.id)
        .put("title", c.title)
        .put("kind", c.kind)
        .put("is_pinned", c.isPinned)
        .put("is_muted", c.isMuted)
        .put("is_archived", c.isArchived)
        .put("unread_count", c.unreadCount)
        .put("ttl_seconds", c.ttlSeconds)
        .put("last_message_date_ms", c.lastMessageDateMs)
        .put("is_bot", c.isBot)
        .put("read_outbox_max_id", c.readOutboxMaxId)
        .put("pinned_message_ids", JSONArray(c.pinnedMessageIds))
        .apply {
            c.pinnedMessageIds.lastOrNull()?.let { put("pinned_message_id", it) }
            c.muteUntilMs?.let { put("mute_until_ms", it) }
            c.peerUserId?.let { put("peer_user_id", it) }
            c.draftText?.let { put("draft", JSONObject().put("text", it)) }
        }

    private fun reactionsToJson(m: MockMessage): JSONArray {
        val arr = JSONArray()
        m.reactions.forEach { (emoji, count) ->
            arr.put(
                JSONObject()
                    .put("emoji", emoji)
                    .put("count", count)
                    .put("is_chosen_by_me", emoji in m.myReactions),
            )
        }
        return arr
    }

    private fun messageToJson(m: MockMessage, pinnedIds: List<Long>): JSONObject = JSONObject()
        .put("id", m.id)
        .put("chat_id", m.chatId)
        .put("sender_id", m.senderId)
        .put("sender_name", m.senderName)
        .put("text", m.text)
        .put("content_type", m.contentType)
        .put("date_ms", m.dateMs)
        .put("is_edited", m.isEdited)
        .put("pinned", m.id in pinnedIds)
        .put("reactions", reactionsToJson(m))
        .apply {
            m.replyTo?.let { put("reply_to", it) }
            m.clientNonce?.let { put("client_nonce", it) }
            m.fileId?.let { put("file_id", it) }
            m.filename?.let { put("filename", it) }
            m.mimeType?.let { put("mime_type", it) }
            if (m.sizeBytes > 0) put("size_bytes", m.sizeBytes)
            m.inlineKeyboard?.let { put("inline_keyboard", it) }
            m.poll?.let { put("poll", it) }
        }

    private fun folderToJson(f: MockFolder): JSONObject = JSONObject()
        .put("id", f.id)
        .put("title", f.title)
        .put("include_contacts", f.includeContacts)
        .put("include_bots", f.includeBots)
        .put("include_groups", f.includeGroups)
        .put("include_channels", f.includeChannels)
        .put("exclude_muted", f.excludeMuted)

    private fun botToJson(b: MockBot): JSONObject = JSONObject()
        .put("bot_user_id", b.botUserId)
        .put("agent_id", b.agentId)
        .put("username", b.username)
        .put("display_name", b.displayName)
        .put("description", b.description)
        .put("execution_state", b.executionState)
        .put("avatar_shape", b.avatarShape)
        .put("avatar_color", b.avatarColor)
        .put("preferred_model", b.preferredModel)
        .put("approval_mode", b.approvalMode)
        .put("companion_chat_id", b.companionChatId)

    private fun computerToJson(c: MockComputer): JSONObject = JSONObject()
        .put("node_id", c.nodeId)
        .put("owner_user_id", c.ownerUserId)
        .put("kind", c.kind)
        .put("display_name", c.displayName)
        .put("host", c.host)
        .put("ssh_port", c.sshPort)
        .put("os_name", c.osName)
        .put("resolution", c.resolution)
        .put("online", c.online)
        .put("run_policy", c.runPolicy)
        .put("has_saved_credential", c.hasSavedCredential)

    private fun assistInfoJson(): JSONObject = JSONObject()
        .put("assist_id", assistId)
        .put("allow_assist", assistAllow)
        .put("code_mode", assistCodeMode)
        .put("current_code", assistCode)

    override fun close() {
        running.set(false)
        dropAllWebSockets()
        trackedSockets.forEach { runCatching { it.close() } }
        trackedSockets.clear()
        runCatching { serverSocket.close() }
        executor.shutdownNow()
    }
}
