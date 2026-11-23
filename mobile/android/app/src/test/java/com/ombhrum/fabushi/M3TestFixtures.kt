@file:OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)

package com.ombhrum.fabushi

import android.app.Application
import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import com.ombhrum.fabushi.telegram.TelegramUiState
import com.ombhrum.fabushi.telegram.TelegramViewModel
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.nio.file.Files
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.json.JSONObject
import org.junit.rules.TestWatcher
import org.junit.runner.Description

class InMemorySharedPreferences : SharedPreferences {
    private val data = ConcurrentHashMap<String, Any>()
    private val listeners = CopyOnWriteArrayList<SharedPreferences.OnSharedPreferenceChangeListener>()

    override fun getAll(): MutableMap<String, *> = HashMap(data)
    override fun getString(key: String, defValue: String?): String? = data[key] as? String ?: defValue
    @Suppress("UNCHECKED_CAST")
    override fun getStringSet(key: String, defValues: MutableSet<String>?): MutableSet<String>? =
        (data[key] as? Set<String>)?.toMutableSet() ?: defValues
    override fun getInt(key: String, defValue: Int): Int = data[key] as? Int ?: defValue
    override fun getLong(key: String, defValue: Long): Long = data[key] as? Long ?: defValue
    override fun getFloat(key: String, defValue: Float): Float = data[key] as? Float ?: defValue
    override fun getBoolean(key: String, defValue: Boolean): Boolean = data[key] as? Boolean ?: defValue
    override fun contains(key: String): Boolean = data.containsKey(key)

    override fun edit(): SharedPreferences.Editor = object : SharedPreferences.Editor {
        private val pending = LinkedHashMap<String, Any?>()
        private var clearAll = false

        override fun putString(key: String, value: String?): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putStringSet(key: String, values: MutableSet<String>?): SharedPreferences.Editor = apply { pending[key] = values?.toSet() }
        override fun putInt(key: String, value: Int): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putLong(key: String, value: Long): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putFloat(key: String, value: Float): SharedPreferences.Editor = apply { pending[key] = value }
        override fun putBoolean(key: String, value: Boolean): SharedPreferences.Editor = apply { pending[key] = value }
        override fun remove(key: String): SharedPreferences.Editor = apply { pending[key] = null }
        override fun clear(): SharedPreferences.Editor = apply { clearAll = true }
        override fun commit(): Boolean {
            apply()
            return true
        }
        override fun apply() {
            if (clearAll) data.clear()
            pending.forEach { (k, v) ->
                if (v == null) data.remove(k) else data[k] = v
                listeners.forEach { it.onSharedPreferenceChanged(this@InMemorySharedPreferences, k) }
            }
        }
    }

    override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) {
        listeners.add(listener)
    }

    override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener) {
        listeners.remove(listener)
    }
}

class M3TestApplication(
    val rootDir: File = Files.createTempDirectory("fabushi-m3-test-").toFile(),
    val grantedPermissions: MutableSet<String> = mutableSetOf(),
) : Application() {
    private val filesDirectory = File(rootDir, "files").apply { mkdirs() }
    private val cacheDirectory = File(rootDir, "cache").apply { mkdirs() }
    private val externalDirectory = File(rootDir, "external").apply { mkdirs() }
    private val prefsByName = ConcurrentHashMap<String, InMemorySharedPreferences>()

    override fun getApplicationContext(): Context = this
    override fun getFilesDir(): File = filesDirectory
    override fun getCacheDir(): File = cacheDirectory
    override fun getExternalFilesDir(type: String?): File = externalDirectory
    override fun getSharedPreferences(name: String, mode: Int): SharedPreferences =
        prefsByName.computeIfAbsent(name) { InMemorySharedPreferences() }

    override fun checkPermission(permission: String, pid: Int, uid: Int): Int =
        if (permission in grantedPermissions) PackageManager.PERMISSION_GRANTED else PackageManager.PERMISSION_DENIED

    fun telegramPrefs(): SharedPreferences = getSharedPreferences("fabushi.telegram.prefs", 0)

    fun cleanup() {
        rootDir.deleteRecursively()
    }
}

class M3MainDispatcherRule : TestWatcher() {
    private var executor: ExecutorService? = null

    override fun starting(description: Description) {
        val exec = Executors.newSingleThreadExecutor { r -> Thread(r, "m3-test-main").apply { isDaemon = true } }
        executor = exec
        Dispatchers.setMain(exec.asCoroutineDispatcher())
    }

    override fun finished(description: Description) {
        Dispatchers.resetMain()
        executor?.shutdownNow()
        executor = null
    }
}

fun awaitState(
    vm: TelegramViewModel,
    timeoutMs: Long = 5_000L,
    pollMs: Long = 20L,
    predicate: (TelegramUiState) -> Boolean,
): TelegramUiState {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (System.currentTimeMillis() < deadline) {
        val snap = vm.state.value
        if (predicate(snap)) return snap
        Thread.sleep(pollMs)
    }
    val finalSnap = vm.state.value
    check(predicate(finalSnap)) {
        "Timed out after ${timeoutMs}ms waiting for TelegramUiState predicate. Final snapshot: " +
            "auth=${finalSnap.isAuthenticated}, sync=${finalSnap.syncStatusLabel}, ws=${finalSnap.serverWsConnected}, " +
            "chats=${finalSnap.chats.size}, toast=${finalSnap.toastBanner}, authErr=${finalSnap.authError}"
    }
    return finalSnap
}

fun awaitCondition(timeoutMs: Long = 5_000L, pollMs: Long = 20L, predicate: () -> Boolean) {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (System.currentTimeMillis() < deadline) {
        if (predicate()) return
        Thread.sleep(pollMs)
    }
    check(predicate()) { "Timed out after ${timeoutMs}ms waiting for condition" }
}

data class RecordedHttpRequest(
    val method: String,
    val path: String,
    val rawQuery: String,
    val headers: Map<String, String>,
    val body: String,
) {
    fun bodyJsonOrNull(): JSONObject? = runCatching { JSONObject(body) }.getOrNull()
}

data class MockHttpResponse(
    val status: Int = 200,
    val body: String = "{}",
    val contentType: String = "application/json",
    val delayMs: Long = 0L,
)

/**
 * Pure-JDK HTTP/1.1 + RFC 6455 WebSocket loopback server for M3 unit/integration tests.
 * Enforces all 5 wire safeguards: UTF-8 byte Content-Length, case-insensitive headers & bounded
 * body reads, omits Sec-WebSocket-Extensions, supports 7/16/64-bit WS payload lengths, and
 * synchronizes WS OutputStream writes with clean teardown.
 */
class LoopbackTelegramTestServer(
    private val httpHandler: (RecordedHttpRequest) -> MockHttpResponse,
    private val wsOnClientMessage: ((String, (String) -> Unit) -> Unit)? = null,
    private val wsAuthorize: ((RecordedHttpRequest) -> Boolean)? = null,
) : AutoCloseable {
    private val serverSocket = ServerSocket(0)
    private val running = AtomicBoolean(true)
    private val executor = Executors.newCachedThreadPool { r -> Thread(r, "m3-loopback-srv").apply { isDaemon = true } }
    private val trackedSockets = CopyOnWriteArrayList<Socket>()
    private val wsClients = CopyOnWriteArrayList<WsClientHandle>()

    val recordedRequests = CopyOnWriteArrayList<RecordedHttpRequest>()
    val wsRequests = CopyOnWriteArrayList<RecordedHttpRequest>()
    val wsConnectCount = AtomicInteger(0)

    val port: Int get() = serverSocket.localPort
    val baseUrl: String get() = "http://127.0.0.1:$port"
    val wsBaseUrl: String get() = "ws://127.0.0.1:$port"

    private class WsClientHandle(val socket: Socket, val out: OutputStream) {
        private val writeLock = Any()

        fun sendTextFrame(payload: String) {
            val bytes = payload.toByteArray(Charsets.UTF_8)
            synchronized(writeLock) {
                writeFrame(0x81, bytes)
            }
        }

        fun sendPongFrame(payload: ByteArray) {
            synchronized(writeLock) {
                writeFrame(0x8A, payload)
            }
        }

        private fun writeFrame(firstByte: Int, payload: ByteArray) {
            out.write(firstByte)
            val len = payload.size
            when {
                len < 126 -> out.write(len)
                len <= 65535 -> {
                    out.write(126)
                    out.write((len shr 8) and 0xFF)
                    out.write(len and 0xFF)
                }
                else -> {
                    out.write(127)
                    val l = len.toLong()
                    for (shift in 56 downTo 0 step 8) {
                        out.write(((l ushr shift) and 0xFFL).toInt())
                    }
                }
            }
            out.write(payload)
            out.flush()
        }
    }

    init {
        executor.submit {
            while (running.get()) {
                val sock = runCatching { serverSocket.accept() }.getOrNull() ?: break
                sock.tcpNoDelay = true
                trackedSockets.add(sock)
                executor.submit { handleConnection(sock) }
            }
        }
    }

    fun pushWsEvent(json: String) {
        wsClients.forEach { client ->
            runCatching { client.sendTextFrame(json) }
        }
    }

    fun dropAllWebSockets() {
        val snapshot = wsClients.toList()
        wsClients.clear()
        snapshot.forEach { runCatching { it.socket.close() } }
    }

    private fun handleConnection(socket: Socket) {
        try {
            val input = BufferedInputStream(socket.getInputStream())
            val out = socket.getOutputStream()
            val req = readHttpRequest(input) ?: return

            val upgrade = req.headers["upgrade"]
            if (upgrade.equals("websocket", ignoreCase = true)) {
                wsRequests.add(req)
                val authorized = wsAuthorize?.invoke(req) ?: true
                if (!authorized) {
                    writeHttpResponse(out, MockHttpResponse(status = 401, body = """{"error":"unauthorized"}"""))
                    socket.close()
                    return
                }
                val key = req.headers["sec-websocket-key"].orEmpty()
                val acceptBytes = MessageDigest.getInstance("SHA-1")
                    .digest((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray(Charsets.US_ASCII))
                val accept = Base64.getEncoder().encodeToString(acceptBytes)
                val handshake = "HTTP/1.1 101 Switching Protocols\r\n" +
                    "Upgrade: websocket\r\n" +
                    "Connection: Upgrade\r\n" +
                    "Sec-WebSocket-Accept: $accept\r\n\r\n"
                out.write(handshake.toByteArray(Charsets.US_ASCII))
                out.flush()

                val handle = WsClientHandle(socket, out)
                wsClients.add(handle)
                wsConnectCount.incrementAndGet()
                try {
                    readWebSocketLoop(input, handle)
                } finally {
                    wsClients.remove(handle)
                    runCatching { socket.close() }
                }
                return
            }

            recordedRequests.add(req)
            val resp = httpHandler(req)
            if (resp.delayMs > 0L) Thread.sleep(resp.delayMs)
            writeHttpResponse(out, resp)
            socket.close()
        } catch (_: Throwable) {
            runCatching { socket.close() }
        } finally {
            trackedSockets.remove(socket)
        }
    }

    private fun readHttpRequest(input: InputStream): RecordedHttpRequest? {
        val headerBuf = ByteArrayOutputStream()
        var state = 0
        while (true) {
            val b = input.read()
            if (b < 0) return null
            headerBuf.write(b)
            state = when (state) {
                0 -> if (b == '\r'.code) 1 else 0
                1 -> if (b == '\n'.code) 2 else if (b == '\r'.code) 1 else 0
                2 -> if (b == '\r'.code) 3 else 0
                3 -> if (b == '\n'.code) 4 else 0
                else -> 0
            }
            if (state == 4) break
        }
        val rawHeader = headerBuf.toString(Charsets.UTF_8.name())
        val lines = rawHeader.split("\r\n")
        val reqLine = lines.firstOrNull()?.split(" ") ?: return null
        if (reqLine.size < 2) return null
        val method = reqLine[0].uppercase()
        val fullTarget = reqLine[1]
        val qIdx = fullTarget.indexOf('?')
        val path = if (qIdx >= 0) fullTarget.substring(0, qIdx) else fullTarget
        val rawQuery = if (qIdx >= 0) fullTarget.substring(qIdx + 1) else ""

        val headers = LinkedHashMap<String, String>()
        for (i in 1 until lines.size) {
            val line = lines[i]
            val colon = line.indexOf(':')
            if (colon > 0) {
                headers[line.substring(0, colon).trim().lowercase()] = line.substring(colon + 1).trim()
            }
        }

        val contentLength = headers["content-length"]?.toIntOrNull() ?: 0
        val body = if (contentLength > 0) {
            val bytes = ByteArray(contentLength)
            var off = 0
            while (off < contentLength) {
                val read = input.read(bytes, off, contentLength - off)
                if (read <= 0) break
                off += read
            }
            String(bytes, 0, off, Charsets.UTF_8)
        } else ""

        return RecordedHttpRequest(method, path, rawQuery, headers, body)
    }

    private fun writeHttpResponse(out: OutputStream, resp: MockHttpResponse) {
        val bodyBytes = resp.body.toByteArray(Charsets.UTF_8)
        val statusText = when (resp.status) {
            200 -> "OK"
            201 -> "Created"
            400 -> "Bad Request"
            401 -> "Unauthorized"
            403 -> "Forbidden"
            404 -> "Not Found"
            500 -> "Internal Server Error"
            else -> "Status"
        }
        val header = "HTTP/1.1 ${resp.status} $statusText\r\n" +
            "Content-Type: ${resp.contentType}; charset=utf-8\r\n" +
            "Content-Length: ${bodyBytes.size}\r\n" +
            "Connection: close\r\n\r\n"
        out.write(header.toByteArray(Charsets.US_ASCII))
        out.write(bodyBytes)
        out.flush()
    }

    private fun readWebSocketLoop(input: InputStream, handle: WsClientHandle) {
        while (running.get() && !handle.socket.isClosed) {
            val b1 = input.read()
            if (b1 < 0) break
            val b2 = input.read()
            if (b2 < 0) break
            val opcode = b1 and 0x0F
            val masked = (b2 and 0x80) != 0
            var len = (b2 and 0x7F).toLong()
            if (len == 126L) {
                val hi = input.read()
                val lo = input.read()
                if (hi < 0 || lo < 0) break
                len = ((hi shl 8) or lo).toLong()
            } else if (len == 127L) {
                var v = 0L
                for (i in 0 until 8) {
                    val x = input.read()
                    if (x < 0) return
                    v = (v shl 8) or (x.toLong() and 0xFFL)
                }
                len = v
            }
            val mask = if (masked) ByteArray(4) { input.read().toByte() } else null
            val payload = ByteArray(len.toInt())
            var off = 0
            while (off < payload.size) {
                val n = input.read(payload, off, payload.size - off)
                if (n <= 0) return
                off += n
            }
            if (mask != null) {
                for (i in payload.indices) {
                    payload[i] = (payload[i].toInt() xor mask[i % 4].toInt()).toByte()
                }
            }
            when (opcode) {
                0x8 -> break // Close
                0x9 -> handle.sendPongFrame(payload) // Ping -> Pong
                0x1 -> {
                    val text = String(payload, Charsets.UTF_8)
                    wsOnClientMessage?.invoke(text) { reply -> handle.sendTextFrame(reply) }
                }
            }
        }
    }

    override fun close() {
        running.set(false)
        dropAllWebSockets()
        trackedSockets.forEach { runCatching { it.close() } }
        trackedSockets.clear()
        runCatching { serverSocket.close() }
        executor.shutdownNow()
    }
}
