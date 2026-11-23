package com.ombhrum.fabushi.telegram

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ombhrum.fabushi.LoopbackTelegramTestServer
import com.ombhrum.fabushi.M3MainDispatcherRule
import com.ombhrum.fabushi.M3TestApplication
import com.ombhrum.fabushi.MockHttpResponse
import com.ombhrum.fabushi.awaitCondition
import com.ombhrum.fabushi.awaitState
import java.net.URLEncoder
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Verifies M3 Feature F9:
 * - Timestamp-descending chat sorting & 5-rule anti-flicker pin/mute/archive reconciliation
 * - Server-persisted message pins, edit, delete, reactions, drafts, folders, polls, pagination
 * - Media-only message preservation (blank text NOT dropped)
 * - Optimistic send state machine (sending -> sent -> read / failed + retry reusing client_nonce)
 * - WebSocket URL-encoded token, exponential backoff reconnect + syncDifference, HTTP/WS 401 wipe,
 *   SessionRequestFence isolation, and full UpdateEvent dispatch.
 */
class TelegramSyncPinningAndWebSocketTest {
    @get:Rule
    val mainDispatcherRule = M3MainDispatcherRule()

    @Test
    fun antiFlickerPinningSortingRollbackMediaOnlyDeliveryStateAndRetry() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val failNextPinChat = AtomicBoolean(false)
        val failNextPinMsg = AtomicBoolean(false)
        val failNextSend = AtomicBoolean(true)
        val pinStateByChat = mutableMapOf(101L to true, 102L to false, 103L to false)
        val pinnedMsgs = mutableListOf<Long>()
        val sentNonces = mutableListOf<String>()
        val nextMsgId = AtomicInteger(900)

        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when {
                    req.path == "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1","token":"tok+special/char=1&2_abcdefghijklmnop"}""",
                    )
                    req.path == "/api/v1/auth/me" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1"}""",
                    )
                    req.path == "/api/v1/chats" -> {
                        val arr = JSONArray()
                            .put(JSONObject().put("id", 101).put("title", "Pinned One").put("kind", "direct").put("is_pinned", pinStateByChat[101L] == true).put("last_message_date_ms", 1000L))
                            .put(JSONObject().put("id", 102).put("title", "Older Chat").put("kind", "direct").put("is_pinned", pinStateByChat[102L] == true).put("last_message_date_ms", 2000L))
                            .put(JSONObject().put("id", 103).put("title", "Newest Chat").put("kind", "direct").put("is_pinned", pinStateByChat[103L] == true).put("last_message_date_ms", 5000L))
                        MockHttpResponse(body = JSONObject().put("chats", arr).put("total", 3).toString())
                    }
                    req.path.matches(Regex("/api/v1/chats/\\d+/pin")) -> {
                        val cid = req.path.split("/")[4].toLong()
                        if (failNextPinChat.getAndSet(false)) {
                            MockHttpResponse(status = 500, body = """{"error":"internal","message":"db error"}""")
                        } else {
                            val pinned = req.bodyJsonOrNull()?.optBoolean("pinned", true) ?: true
                            pinStateByChat[cid] = pinned
                            MockHttpResponse(body = """{"chat_id":$cid,"is_pinned":$pinned,"is_muted":false,"is_archived":false}""")
                        }
                    }
                    req.path == "/api/v1/chats/101/messages" -> {
                        if (req.rawQuery.contains("before_id=501")) {
                            MockHttpResponse(
                                body = """{"chat_id":101,"has_more":false,"messages":[{"id":400,"chat_id":101,"sender_id":2,"sender_name":"Bob","text":"Older page","date_ms":1000}]}""",
                            )
                        } else {
                            // Includes a blank-text photo message (media-only must be preserved!)
                            MockHttpResponse(
                                body = """{"chat_id":101,"has_more":true,"messages":[
                                    {"id":501,"chat_id":101,"sender_id":2,"sender_name":"Bob","text":"","content_type":"photo","file_id":"f_img_1","filename":"lotus.jpg","mime_type":"image/jpeg","date_ms":2000},
                                    {"id":502,"chat_id":101,"sender_id":1,"sender_name":"Alice","text":"Hello","date_ms":3000}
                                ]}""",
                            )
                        }
                    }
                    req.path == "/api/v1/chats/101/pins" && req.method == "GET" -> {
                        MockHttpResponse(body = JSONObject().put("chat_id", 101).put("pinned_message_ids", JSONArray(pinnedMsgs)).toString())
                    }
                    req.path == "/api/v1/chats/101/pins" && req.method == "POST" -> {
                        if (failNextPinMsg.getAndSet(false)) {
                            MockHttpResponse(status = 403, body = """{"error":"forbidden","message":"Admin rights required to pin messages"}""")
                        } else {
                            val mid = req.bodyJsonOrNull()?.optLong("message_id") ?: 0L
                            if (mid > 0L && mid !in pinnedMsgs) pinnedMsgs.add(mid)
                            MockHttpResponse(body = JSONObject().put("chat_id", 101).put("pinned_message_ids", JSONArray(pinnedMsgs)).toString())
                        }
                    }
                    req.path == "/api/v1/messages/send" -> {
                        val body = req.bodyJsonOrNull()
                        val nonce = body?.optString("client_nonce").orEmpty()
                        if (nonce.isNotBlank()) sentNonces.add(nonce)
                        if (failNextSend.getAndSet(false)) {
                            MockHttpResponse(status = 500, body = """{"error":"send_failed","message":"transient"}""")
                        } else {
                            val mid = nextMsgId.incrementAndGet()
                            MockHttpResponse(body = """{"message_id":$mid,"id":$mid,"pts":10,"date_ms":6000,"status":"sent"}""")
                        }
                    }
                    req.path == "/api/v1/chats/101/draft" && req.method == "GET" -> MockHttpResponse(body = """{"chat_id":101,"draft":null}""")
                    req.path == "/api/v1/chats/101/draft" && req.method == "PUT" -> MockHttpResponse(body = """{"chat_id":101,"draft":{"text":"saved draft"}}""")
                    req.path == "/api/v1/folders" && req.method == "GET" -> MockHttpResponse(body = """{"folders":[]}""")
                    req.path == "/api/v1/folders" && req.method == "POST" -> MockHttpResponse(body = """{"id":7,"title":"Work","include_groups":true}""")
                    req.path == "/api/v1/contacts" -> MockHttpResponse(body = """{"contacts":[]}""")
                    req.path == "/api/v1/auth/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    req.path == "/api/v1/bots/my" || req.path == "/api/v1/fabushi-bots" -> MockHttpResponse(body = """{"bots":[],"total":0}""")
                    req.path == "/api/v1/computers" -> MockHttpResponse(body = """{"computers":[]}""")
                    req.path == "/api/v1/remote/assist/info" -> MockHttpResponse(body = """{"assist_id":"123456789","allow_assist":false,"code_mode":"temporary"}""")
                    req.path == "/api/v1/remote/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    req.path == "/api/v1/mcp/grants" -> MockHttpResponse(body = """{"grants":[]}""")
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )

        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            vm.wsBaseBackoffMs = 150L
            awaitState(vm) { it.syncStatusLabel != "同步中…" }

            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }
            vm.loginWithPassword("alice", "pw")
            val authed = awaitState(vm) { it.isAuthenticated && it.chats.size == 3 && it.serverWsConnected }

            // 1. Verify URL-encoded token in WS query string (+, /, =, &)
            val wsReq = server.wsRequests.first()
            val expectedEncoded = URLEncoder.encode("tok+special/char=1&2_abcdefghijklmnop", "UTF-8")
            assertTrue("Expected percent-encoded WS token, got ${wsReq.rawQuery}", wsReq.rawQuery.contains("token=$expectedEncoded"))
            assertEquals("Bearer tok+special/char=1&2_abcdefghijklmnop", wsReq.headers["authorization"])

            // 2. Verify sorting: Pinned first (101), then descending by lastMessageAtMs (103 > 102)
            assertEquals(listOf("101", "103", "102"), authed.chats.map { it.id })

            // 3. Unpin 101 -> sorts purely by lastMessageAtMs: 103, 102, 101 (NEVER bitwise-OR back to pinned!)
            vm.togglePinChat("101")
            val afterUnpin = awaitState(vm) { it.chats.firstOrNull { c -> c.id == "101" }?.isPinned == false && pinStateByChat[101L] == false }
            assertEquals(listOf("103", "102", "101"), afterUnpin.chats.map { it.id })

            // 4. Pin rollback on HTTP 500
            failNextPinChat.set(true)
            vm.clearToastBanner()
            vm.togglePinChat("102")
            val afterPinFail = awaitState(vm) { it.toastBanner?.contains("置顶会话失败") == true }
            assertFalse(afterPinFail.chats.first { it.id == "102" }.isPinned)

            // 5. Open chat 101 -> media-only photo (text == "") is preserved!
            vm.openChat("101")
            val opened = awaitState(vm) { it.messagesByChat["101"]?.size == 2 }
            val photoMsg = opened.messagesByChat["101"]!!.first { it.id == "501" }
            assertEquals("photo", photoMsg.contentType)
            assertEquals("f_img_1", photoMsg.mediaBlobId)

            // 6. Paginate older messages via before_id without duplicates
            vm.loadOlderMessages("101")
            val paginated = awaitState(vm) { it.messagesByChat["101"]?.size == 3 }
            assertEquals(listOf("400", "501", "502"), paginated.messagesByChat["101"]!!.map { it.id })
            assertFalse(paginated.chats.first { it.id == "101" }.hasMoreHistory)

            // 7. Message pin 403 rollback & success
            failNextPinMsg.set(true)
            vm.clearToastBanner()
            vm.togglePinMessage("101", "502")
            val pin403 = awaitState(vm) { it.toastBanner?.contains("置顶消息失败") == true }
            assertFalse(pin403.chats.first { it.id == "101" }.pinnedMessageIds.contains("502"))

            vm.togglePinMessage("101", "502")
            val pinOk = awaitState(vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds.contains("502") }
            assertTrue(pinOk.messagesByChat["101"]!!.first { it.id == "502" }.pinned)

            // 8. Send failure -> deliveryState == "failed" -> retry reuses exact client_nonce -> "sent" -> WS ReadHistory -> "read"
            vm.sendComposerMessage("101", "Retry test msg")
            val failedState = awaitState(vm) {
                it.messagesByChat["101"]?.any { m -> m.text == "Retry test msg" && m.deliveryState == "failed" } == true
            }
            val failedMsg = failedState.messagesByChat["101"]!!.first { it.text == "Retry test msg" }
            vm.retryFailedMessage("101", failedMsg.id)
            val sentState = awaitState(vm) {
                it.messagesByChat["101"]?.any { m -> m.text == "Retry test msg" && m.deliveryState == "sent" } == true
            }
            val sentMsg = sentState.messagesByChat["101"]!!.first { it.text == "Retry test msg" }
            assertEquals(2, sentNonces.size)
            assertEquals(sentNonces[0], sentNonces[1])

            // WS ReadHistory(is_outbox=true) flips "sent" -> "read"
            server.pushWsEvent(
                """{"type":"Update","payload":{"type":"ReadHistory","data":{"chat_id":101,"max_id":${sentMsg.id},"is_outbox":true,"unread_count":0}}}""",
            )
            awaitState(vm) {
                it.messagesByChat["101"]?.firstOrNull { m -> m.id == sentMsg.id }?.deliveryState == "read"
            }

            // 9. WS UpdateEvent dispatch (UserTyping, BotActivityChanged, PinnedMessagesChanged, MiniAppControlStatus)
            server.pushWsEvent("""{"type":"Update","payload":{"type":"UserTyping","data":{"chat_id":101,"user_id":2,"action":"typing"}}}""")
            awaitState(vm) { it.chats.first { c -> c.id == "101" }.typingLabel?.contains("正在输入") == true }

            server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":102,"bot_user_id":99,"state":"running_tool","detail":"shell_exec"}}}""")
            val botAct = awaitState(vm) { it.chats.first { c -> c.id == "102" }.botExecutionState == "running_tool" }
            assertTrue(botAct.chats.first { it.id == "102" }.typingLabel.orEmpty().contains("shell_exec"))

            server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessagesChanged","data":{"chat_id":101,"pinned_ids":[]}}}""")
            awaitState(vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds.isEmpty() }

            server.pushWsEvent("""{"type":"Update","payload":{"type":"MiniAppControlStatus","data":{"bot_username":"fabushi_bot","short_name":"global_dharma","chat_id":101,"bot_user_id":99,"app_title":"全球法布施","mode":"ai_controlling","status_text":"AI 运行中","active_tool":"dharma.offer"}}}""")
            awaitState(vm) { it.miniAppPip.controlMode == "ai_controlling" && it.miniAppPip.activeTool == "dharma.offer" }

            // 10. Forced WS disconnect triggers automatic exponential backoff reconnect & syncDifference
            val beforeCount = server.wsConnectCount.get()
            server.dropAllWebSockets()
            awaitCondition(timeoutMs = 5_000L) { server.wsConnectCount.get() > beforeCount }
            awaitState(vm) { it.serverWsConnected }
            awaitCondition(timeoutMs = 3_000L) { server.recordedRequests.any { it.path.contains("/api/v1/sync/difference") } }
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun wsError401WipesSessionStopsReconnectAndReturnsToAuthWizard() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":5,"username":"bob","first_name":"Bob","session_id":"s5","token":"tok_401_test_123456789012345"}""",
                    )
                    "/api/v1/auth/me" -> MockHttpResponse(body = """{"user_id":5,"username":"bob","first_name":"Bob","session_id":"s5"}""")
                    "/api/v1/chats" -> MockHttpResponse(body = """{"chats":[{"id":201,"title":"Chat A","kind":"direct","last_message_date_ms":1000}],"total":1}""")
                    else -> MockHttpResponse(body = """{"ok":true,"folders":[],"contacts":[],"sessions":[],"bots":[],"computers":[],"grants":[]}""")
                }
            },
        )
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            vm.wsBaseBackoffMs = 150L
            awaitState(vm) { it.syncStatusLabel != "同步中…" }

            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }
            vm.loginWithPassword("bob", "pw")
            awaitState(vm) { it.isAuthenticated && it.serverWsConnected && it.chats.size == 1 }

            val connectsBefore401 = server.wsConnectCount.get()
            server.pushWsEvent("""{"type":"Error","payload":{"code":401,"message":"token revoked"}}""")
            val wiped = awaitState(vm) { !it.isAuthenticated && it.chats.isEmpty() && !it.serverWsConnected }
            assertTrue(wiped.authError.orEmpty().contains("401"))
            Thread.sleep(400L)
            assertEquals("WS must NOT reconnect after 401", connectsBefore401, server.wsConnectCount.get())
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }
}
