package com.ombhrum.fabushi.e2e

import android.content.Context
import android.content.pm.PackageManager
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.FabushiRemoteDeviceGateway
import com.ombhrum.fabushi.MarketplacePlugin
import com.ombhrum.fabushi.MiniAppNativeWebMcpBridge
import com.ombhrum.fabushi.MiniAppToolContract
import com.ombhrum.fabushi.e2e.harness.E2eViewModelFixture
import com.ombhrum.fabushi.e2e.harness.FaultRule
import com.ombhrum.fabushi.e2e.harness.MainDispatcherRule
import com.ombhrum.fabushi.e2e.harness.MockGpuiTelegramServer
import com.ombhrum.fabushi.e2e.harness.TestFabushiApplication
import com.ombhrum.fabushi.e2e.harness.awaitCondition
import com.ombhrum.fabushi.e2e.harness.awaitState
import com.ombhrum.fabushi.injectLocalWebMcp
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import com.ombhrum.fabushi.mika.TelegramMikaChannelBridge
import com.ombhrum.fabushi.telegram.GenerationFence
import com.ombhrum.fabushi.telegram.GlobalDharmaUiState
import com.ombhrum.fabushi.telegram.HermesStudioUiState
import com.ombhrum.fabushi.telegram.InlineKeyboardButtonKind
import com.ombhrum.fabushi.telegram.MessageEntity
import com.ombhrum.fabushi.telegram.MessageEntityType
import com.ombhrum.fabushi.telegram.MikaTaskSummaryItem
import com.ombhrum.fabushi.telegram.buildEmbeddedGlobalDharmaWebMcpHtml
import com.ombhrum.fabushi.telegram.buildRichMessageAnnotatedString
import com.ombhrum.fabushi.telegram.extractFirstSafeHttpUrl
import com.ombhrum.fabushi.telegram.sanitizeGlobalDharmaCountryCode
import com.wangzan101.mika.MikaApp
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.agent.TaskState
import com.wangzan101.mika.core.gate.ConfirmationRequest
import com.wangzan101.mika.core.risk.IrrevAction
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import java.io.File
import java.nio.file.Files
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Tier 5 — Adversarial Hardening Verification Suite.
 *
 * Deterministically locks every CRITICAL, HIGH, MEDIUM, and Polish defect found in Phase 2
 * adversarial audits:
 * - C1: Android ART `org.json.JSONObject.optString` `"null"` coercion across all parsers & gateway
 * - C2: Systemic stale-refresh overwrite race across Sessions, Contacts, Folders, Computers, Pins + targeted rollback
 * - H1: Atomic `GenerationFence.compareAndAdvance` + pre-login 401 session wipe prevention
 * - H2: Mika stale `askId` rejection on BOTH `resolveConfirmation` & `answerAsk` + thread-safe `recordRecentTask`
 * - H3: `FabushiRemoteDeviceGateway.close()` concurrent shutdown safety
 * - H4: WS `Ack` deduplication & monotonic `"read"` state preservation
 * - H5: `parseInlineKeyboard` kind/payload precedence alignment
 * - MiniApp H3 & M5: `injectLocalWebMcp` `$1`/`$0`/`<!--`/`</script>` escaping + fail-closed approval prompt
 * - M1/M5/M6/M7: Rich UTF-16 entity rendering & safe URL clicks, reserved folder ID filter, WebMCP country sanitization
 * - M3/M4/Polish: Cloud draft sync on `closeChat`/`sendComposerMessage`, authenticated `sendLocation`/`sendContactCard`,
 *   and `updateRemoteAssistCode` letter+digit enforcement.
 */
class E2eTier5AdversarialHardeningTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    /**
     * Emulates real Android ART `libcore/json/src/main/java/org/json/JSONObject.java` semantics
     * where `optString(key)` returns literal `"null"` for `JSONObject.NULL` (whereas JVM `org.json:json`
     * silently returns `""`, masking real-device bugs!).
     */
    private class ArtEmulatingJSONObject(source: String) : JSONObject(source) {
        override fun optString(key: String?): String = optString(key, "")

        override fun optString(key: String?, defaultValue: String?): String {
            if (key == null) return defaultValue ?: ""
            val value = opt(key)
            if (value === JSONObject.NULL) return "null"
            return if (value == null) (defaultValue ?: "") else value.toString()
        }

        override fun optJSONObject(key: String?): JSONObject? {
            val inner = super.optJSONObject(key) ?: return null
            return ArtEmulatingJSONObject(inner.toString())
        }
    }

    private class TestMikaApp(private val rootDir: File) : MikaApp() {
        override fun getFilesDir(): File = File(rootDir, "files").apply { mkdirs() }
        override fun getCacheDir(): File = File(rootDir, "cache").apply { mkdirs() }
        override fun getApplicationContext(): Context = this
    }

    // =========================================================================
    // C1: Android ART org.json "null" Coercion Immunity
    // =========================================================================

    @Test
    fun t5_c1_aospJsonNullCoercionDoesNotCorruptMessagesChatsBotsComputersHermesOrGateway() = runBlocking {
        E2eViewModelFixture().use { fixture ->
            val vm = fixture.vm

            // 1. Incoming messages with explicit JSON null fields must NOT parse clientNonce="null" or media_id="null"
            val msg1Json = ArtEmulatingJSONObject(
                """
                {
                  "id": 101,
                  "chat_id": 5001,
                  "sender_id": 22,
                  "sender_name": "Bob",
                  "text": "First incoming message",
                  "client_nonce": null,
                  "media_id": null,
                  "file_id": null,
                  "filename": null,
                  "mime_type": null,
                  "thumbnail_data_url": null
                }
                """.trimIndent(),
            )
            val parsed1 = vm.parseServerMessageJson(msg1Json, "5001", myUid = 1L, readOutboxMaxId = 0L)
            assertNotNull(parsed1)
            assertNull("client_nonce: null must parse as null, never \"null\"", parsed1!!.clientNonce)
            assertEquals("text", parsed1.contentType)
            assertNull(parsed1.mediaBlobId)
            assertNull(parsed1.mediaFileName)
            assertNull(parsed1.mediaMimeType)
            assertNull(parsed1.mediaThumbnailDataUrl)

            // 2. Two consecutive incoming messages with client_nonce: null must BOTH survive (no chat wipe!)
            val msg2Json = ArtEmulatingJSONObject(
                """
                {
                  "id": 102,
                  "chat_id": 5001,
                  "sender_id": 22,
                  "sender_name": "Bob",
                  "text": "Second incoming message",
                  "client_nonce": null,
                  "media_id": null
                }
                """.trimIndent(),
            )
            vm.applyIncomingNewMessage(msg1Json, 5001L)
            vm.applyIncomingNewMessage(msg2Json, 5001L)
            val chatMsgs = vm.state.value.messagesByChat["5001"].orEmpty()
            assertEquals("Both messages must remain (client_nonce:null must not wipe prior msg)", 2, chatMsgs.size)
            assertEquals(listOf("101", "102"), chatMsgs.map { it.id })

            // 3. Chat object with null optional fields
            val chatJson = ArtEmulatingJSONObject(
                """
                {
                  "chat_id": 5001,
                  "title": "Alice",
                  "about": null,
                  "peer_bio": null,
                  "peer_username": null,
                  "username": null,
                  "last_message_text": null,
                  "preview": null
                }
                """.trimIndent(),
            )
            val parsedChat = vm.parseServerChatObject(chatJson)
            assertNotNull(parsedChat)
            assertNull(parsedChat!!.peerBio)
            assertNull(parsedChat.username)
            assertFalse(parsedChat.subtitle.contains("null"))
            assertEquals("", parsedChat.preview)

            // 4. Inline keyboard with web_app_url: null & url: null stays CALLBACK
            val kbRow = JSONArray().put(
                ArtEmulatingJSONObject(
                    """{"text":"Approve","callback_data":"ok_action","web_app_url":null,"url":null}""",
                ),
            )
            val parsedKb = vm.parseInlineKeyboard(JSONArray().put(kbRow))
            assertNotNull(parsedKb)
            assertEquals(1, parsedKb!!.size)
            val btn = parsedKb[0].buttons[0]
            assertEquals(InlineKeyboardButtonKind.CALLBACK, btn.kind)
            assertEquals("ok_action", btn.payload)

            // 5. Bot DTO with bound_node_id: null
            val botJson = ArtEmulatingJSONObject(
                """{"bot_user_id": 9001, "username": "helper_bot", "display_name": "Helper", "bound_node_id": null}""",
            )
            val parsedBot = vm.parseFabushiBotDto(botJson, isFabushi = true)
            assertNotNull(parsedBot)
            assertNull(parsedBot!!.boundNodeId)

            // 6. Computer node with null optional fields
            val compJson = ArtEmulatingJSONObject(
                """
                {
                  "node_id": "cloud-1",
                  "display_name": "Cloud 1",
                  "host": null,
                  "ip_address": null,
                  "ssh_username": null,
                  "auth_method": null,
                  "device_id": null,
                  "latest_frame_data_url": null
                }
                """.trimIndent(),
            )
            val parsedComp = vm.parseServerComputerObject(compJson)
            assertNotNull(parsedComp)
            assertNull(parsedComp!!.host)
            assertNull(parsedComp.sshUsername)
            assertNull(parsedComp.authMethod)
            assertNull(parsedComp.deviceId)
            assertNull(parsedComp.latestFrameDataUrl)

            // 7. Hermes session with parentSessionId: null & parentBranchId: null
            val hermesSessions = JSONObject().put(
                "sessions",
                JSONArray().put(
                    ArtEmulatingJSONObject(
                        """{"sessionId":"h1","title":"Main","providerId":"openai","model":"gpt-4o","parentSessionId":null,"parentBranchId":null}""",
                    ),
                ),
            )
            val parsedHermes = vm.parseHermesStudioState(
                current = HermesStudioUiState(),
                sessionsResp = hermesSessions,
                memoryResp = null,
                skillsResp = null,
                cronResp = null,
                kanbanResp = null,
                approvalsResp = null,
            )
            assertEquals(1, parsedHermes.sessions.size)
            assertNull(parsedHermes.sessions[0].parentBranchId)

            // 8. FabushiRemoteDeviceGateway.invokeTool with null agentId / ref / value under ART semantics
            val surface = FabushiAppAgentSurface()
            val clicked = AtomicInteger(0)
            surface.publish(
                screen = "home",
                elements = listOf(
                    FabushiAppAgentSurface.Element(
                        agentId = "btn.submit",
                        role = "button",
                        name = "Submit",
                    ),
                ),
                actions = mapOf(
                    "btn.submit" to FabushiAppAgentSurface.Action(
                        allowed = setOf("invoke"),
                        invoke = { _ -> clicked.incrementAndGet() },
                    ),
                ),
            )
            val gen = surface.snapshot().generation
            val gateway = FabushiRemoteDeviceGateway(
                context = fixture.app,
                surface = surface,
                metadata = emptyMap(),
                gatewayUrl = "ws://127.0.0.1:1/agent",
            )
            try {
                val findRes = gateway.invokeTool(
                    FabushiAppAgentSurface.FindTool,
                    ArtEmulatingJSONObject("""{"ref":null,"agentId":null,"role":"button","name":null,"text":null}"""),
                )
                assertEquals(1, findRes.getJSONArray("matches").length())

                val actRes = gateway.invokeTool(
                    FabushiAppAgentSurface.ActionTool,
                    ArtEmulatingJSONObject("""{"generation":$gen,"ref":null,"agentId":"btn.submit","action":"invoke","value":null}"""),
                )
                assertEquals("home", actRes.getString("screen"))
                assertEquals(1, clicked.get())
            } finally {
                gateway.close()
            }
        }
    }

    // =========================================================================
    // C2: Systemic Stale-Refresh Overwrite Race + Targeted Single-Item Rollback
    // =========================================================================

    @Test
    fun t5_c2_staleRefreshOverwriteRacePreventedAcrossSessionsContactsFoldersComputersAndPins() {
        MockGpuiTelegramServer().use { server ->
            val aliceId = 1L
            val bobId = 2L
            val carolId = 3L
            server.seedSession(
                MockGpuiTelegramServer.MockSession(
                    sessionId = "sess-extra-1",
                    userId = aliceId,
                    token = "tok-extra-1",
                    deviceName = "iPad Pro",
                    platform = "ios",
                    isCurrent = false,
                ),
            )
            server.seedContact(
                MockGpuiTelegramServer.MockContact(
                    userId = bobId,
                    ownerUserId = aliceId,
                    username = "bob",
                    displayName = "Bob",
                ),
            )
            server.seedContact(
                MockGpuiTelegramServer.MockContact(
                    userId = carolId,
                    ownerUserId = aliceId,
                    username = "carol",
                    displayName = "Carol",
                ),
            )
            val folder1 = MockGpuiTelegramServer.MockFolder(id = 41L, ownerUserId = aliceId, title = "Focus")
            server.seedFolder(folder1)
            val node1 = MockGpuiTelegramServer.MockComputer(
                nodeId = "node-cloud-1",
                ownerUserId = aliceId,
                displayName = "Ubuntu Box",
                host = "10.0.0.1",
                online = true,
            )
            val node2 = MockGpuiTelegramServer.MockComputer(
                nodeId = "node-cloud-2",
                ownerUserId = aliceId,
                displayName = "Build Box",
                host = "10.0.0.2",
                online = true,
            )
            server.seedComputer(node1)
            server.seedComputer(node2)
            val chat = MockGpuiTelegramServer.MockChat(
                id = 2001L,
                ownerUserId = aliceId,
                title = "Pinned Race Chat",
                kind = "direct",
                peerUserId = bobId,
            )
            server.seedChat(chat)
            val msg1 = MockGpuiTelegramServer.MockMessage(
                id = 3001L,
                chatId = chat.id,
                senderId = bobId,
                senderName = "Bob",
                text = "Pin me then unpin me",
                dateMs = System.currentTimeMillis() - 5_000L,
            )
            server.seedMessage(msg1)
            chat.pinnedMessageIds.add(msg1.id)

            E2eViewModelFixture().use { fixture ->
                fixture.loginAndConnect(server)
                val vm = fixture.vm

                // Verify baseline hydrated
                awaitState(vm, description = "baseline sessions/contacts/folders/computers") { s ->
                    s.serverSessions.any { it.sessionId == "sess-extra-1" } &&
                        s.serverContacts.any { it.userId == carolId } &&
                        s.folderTabs.any { it.id == folder1.id.toString() } &&
                        s.cloudComputer.nodes.any { it.id == node2.nodeId }
                }

                // 1) Sessions stale-refresh race (root cause of e2e_t1_a5)
                val staleSessionsBody = """
                    {"sessions":[
                      {"session_id":"sess-cur","device_name":"Current","platform":"android","is_current":true},
                      {"session_id":"sess-extra-1","device_name":"iPad Pro","platform":"ios","is_current":false}
                    ]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/auth/sessions",
                        status = 200,
                        errorBody = staleSessionsBody,
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.refreshSessions()
                Thread.sleep(40)
                vm.revokeSession("sess-extra-1")
                awaitCondition(description = "revokeSession completes") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path.contains("sess-extra-1") }
                }
                Thread.sleep(420)
                assertFalse(
                    "Stale refreshSessions must NEVER resurrect revoked sess-extra-1",
                    vm.state.value.serverSessions.any { it.sessionId == "sess-extra-1" },
                )

                // 2) Contacts stale-refresh race
                val staleContactsBody = """
                    {"contacts":[
                      {"user_id":$bobId,"username":"bob","display_name":"Bob"},
                      {"user_id":$carolId,"username":"carol","display_name":"Carol"}
                    ]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/contacts",
                        status = 200,
                        errorBody = staleContactsBody,
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.refreshContacts()
                Thread.sleep(40)
                vm.deleteContact(carolId.toString())
                awaitCondition(description = "deleteContact completes") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path.contains("/contacts/$carolId") }
                }
                Thread.sleep(420)
                assertFalse(
                    "Stale refreshContacts must NEVER resurrect deleted Carol",
                    vm.state.value.serverContacts.any { it.userId == carolId },
                )

                // 2b) Targeted single-item rollback: failing deleteContact(bob) restores ONLY Bob, NOT already-deleted Carol!
                server.addFault(
                    FaultRule(
                        method = "DELETE",
                        pathPrefix = "/api/v1/contacts/$bobId",
                        status = 500,
                        errorBody = """{"error":"server_error","message":"boom"}""",
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.deleteContact(bobId.toString())
                awaitState(vm, description = "Bob restored on failure without resurrecting Carol") { s ->
                    s.serverContacts.any { it.userId == bobId } &&
                        s.toastBanner?.contains("删除联系人失败") == true
                }
                assertFalse(
                    "Targeted rollback must NOT resurrect previously deleted Carol",
                    vm.state.value.serverContacts.any { it.userId == carolId },
                )

                // 3) Folders stale-refresh race
                val staleFoldersBody = """
                    {"folders":[{"id":${folder1.id},"title":"Focus"}]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/folders",
                        status = 200,
                        errorBody = staleFoldersBody,
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.refreshFolders()
                Thread.sleep(40)
                vm.deleteCustomFolder(folder1.id.toString())
                awaitCondition(description = "deleteCustomFolder completes") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path.contains("/folders/${folder1.id}") }
                }
                Thread.sleep(420)
                assertFalse(
                    "Stale refreshFolders must NEVER resurrect deleted folder",
                    vm.state.value.folderTabs.any { it.id == folder1.id.toString() },
                )

                // 4) Computers stale-refresh race
                val staleComputersBody = """
                    {"computers":[
                      {"node_id":"${node1.nodeId}","display_name":"Ubuntu Box","kind":"cloud_server","host":"10.0.0.1","online":true},
                      {"node_id":"${node2.nodeId}","display_name":"Build Box","kind":"cloud_server","host":"10.0.0.2","online":true}
                    ]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/computers",
                        status = 200,
                        errorBody = staleComputersBody,
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.refreshComputers()
                Thread.sleep(40)
                vm.deleteComputer(node2.nodeId)
                awaitCondition(description = "deleteComputer completes") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path.contains("/computers/${node2.nodeId}") }
                }
                Thread.sleep(420)
                assertFalse(
                    "Stale refreshComputers must NEVER resurrect deleted node-cloud-2",
                    vm.state.value.cloudComputer.nodes.any { it.id == node2.nodeId },
                )

                // 5) Pins post-completion anti-flicker stale-refresh race
                vm.openChat(chat.id.toString())
                awaitState(vm, description = "chat messages and initial pin loaded") { s ->
                    s.chats.firstOrNull { it.id == chat.id.toString() }?.pinnedMessageIds?.contains(msg1.id.toString()) == true
                }
                val stalePinsBody = """{"chat_id":${chat.id},"pinned_message_ids":[${msg1.id}]}"""
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/chats/${chat.id}/pins",
                        status = 200,
                        errorBody = stalePinsBody,
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.fetchPinnedMessages(chat.id.toString(), chat.id)
                Thread.sleep(40)
                // Unpin completes quickly and removes inFlightMessagePins BEFORE delayed GET /pins finishes!
                vm.togglePinMessage(chat.id.toString(), msg1.id.toString())
                awaitCondition(description = "unpin message completes") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path.contains("/chats/${chat.id}/pins") }
                }
                Thread.sleep(420)
                assertFalse(
                    "Stale fetchPinnedMessages finishing after unpin must NEVER re-pin message",
                    vm.state.value.chats.first { it.id == chat.id.toString() }.pinnedMessageIds.contains(msg1.id.toString()),
                )

                // 6) deleteMessage HTTP 500 targeted rollback must restore ONLY the deleted message & pin
                //    WITHOUT wiping a mid-flight WS NewMessage that arrived while DELETE was in flight.
                vm.togglePinMessage(chat.id.toString(), msg1.id.toString())
                awaitState(vm, description = "msg1 re-pinned before deleteMessage rollback check") { s ->
                    s.chats.firstOrNull { it.id == chat.id.toString() }?.pinnedMessageIds?.contains(msg1.id.toString()) == true
                }
                server.addFault(
                    FaultRule(
                        method = "DELETE",
                        pathPrefix = "/api/v1/chats/${chat.id}/messages/${msg1.id}",
                        status = 500,
                        errorBody = """{"error":"server_error","message":"delete boom"}""",
                        delayMs = 260L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.deleteMessage(chat.id.toString(), msg1.id.toString(), forEveryone = false)
                assertFalse(
                    "Optimistic deleteMessage removes target message immediately",
                    vm.state.value.messagesByChat[chat.id.toString()].orEmpty().any { it.id == msg1.id.toString() },
                )
                val wsMidDelete = JSONObject()
                    .put("id", 3002L)
                    .put("chat_id", chat.id)
                    .put("sender_id", bobId)
                    .put("sender_name", "Bob")
                    .put("text", "Mid-flight WS during delete")
                    .put("is_outgoing", false)
                    .put("status", "sent")
                    .put("date_ms", System.currentTimeMillis())
                vm.applyIncomingNewMessage(wsMidDelete, chat.id)
                awaitState(vm, description = "deleteMessage HTTP 500 targeted rollback restores msg1") { s ->
                    s.toastBanner?.contains("删除消息失败") == true &&
                        s.messagesByChat[chat.id.toString()].orEmpty().any { it.id == msg1.id.toString() }
                }
                val msgsAfterDeleteRollback = vm.state.value.messagesByChat[chat.id.toString()].orEmpty()
                assertTrue(
                    "deleteMessage rollback MUST preserve mid-flight WS message 3002",
                    msgsAfterDeleteRollback.any { it.id == "3002" },
                )
                assertTrue(
                    "deleteMessage rollback MUST restore deleted message pin",
                    vm.state.value.chats.first { it.id == chat.id.toString() }.pinnedMessageIds.contains(msg1.id.toString()),
                )

                // 7) clearChatHistory HTTP 500 targeted rollback must merge back cleared messages & pins
                //    WITHOUT wiping a mid-flight WS NewMessage that arrived while POST /clear was in flight.
                server.addFault(
                    FaultRule(
                        method = "POST",
                        pathPrefix = "/api/v1/chats/${chat.id}/clear",
                        status = 500,
                        errorBody = """{"error":"server_error","message":"clear boom"}""",
                        delayMs = 260L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.clearChatHistory(chat.id.toString())
                assertTrue(
                    "Optimistic clearChatHistory empties chat immediately",
                    vm.state.value.messagesByChat[chat.id.toString()].orEmpty().isEmpty(),
                )
                val wsMidClear = JSONObject()
                    .put("id", 3003L)
                    .put("chat_id", chat.id)
                    .put("sender_id", bobId)
                    .put("sender_name", "Bob")
                    .put("text", "Mid-flight WS during clear")
                    .put("is_outgoing", false)
                    .put("status", "sent")
                    .put("date_ms", System.currentTimeMillis() + 50L)
                vm.applyIncomingNewMessage(wsMidClear, chat.id)
                awaitState(vm, description = "clearChatHistory HTTP 500 rollback merges cleared messages + WS msg") { s ->
                    s.toastBanner?.contains("清空记录失败") == true &&
                        s.messagesByChat[chat.id.toString()].orEmpty().any { it.id == msg1.id.toString() }
                }
                val msgsAfterClearRollback = vm.state.value.messagesByChat[chat.id.toString()].orEmpty()
                assertTrue(
                    "clearChatHistory rollback MUST restore cleared msg 3001",
                    msgsAfterClearRollback.any { it.id == msg1.id.toString() },
                )
                assertTrue(
                    "clearChatHistory rollback MUST restore cleared msg 3002",
                    msgsAfterClearRollback.any { it.id == "3002" },
                )
                assertTrue(
                    "clearChatHistory rollback MUST preserve mid-flight WS msg 3003",
                    msgsAfterClearRollback.any { it.id == "3003" },
                )
                assertTrue(
                    "clearChatHistory rollback MUST restore cleared pin",
                    vm.state.value.chats.first { it.id == chat.id.toString() }.pinnedMessageIds.contains(msg1.id.toString()),
                )

                // 8) setChatTtl stale-refresh race: slower concurrent refreshAllFromHost() must NOT revert TTL
                val staleChatsForTtl = """
                    {"chats":[{
                      "id":${chat.id},
                      "title":"Pinned Race Chat",
                      "kind":"direct",
                      "peer_user_id":$bobId,
                      "ttl_seconds":0
                    }]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/chats",
                        status = 200,
                        errorBody = staleChatsForTtl,
                        delayMs = 350L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.refreshAllFromHost()
                Thread.sleep(60)
                vm.setChatTtl(chat.id.toString(), 86400)
                awaitCondition(description = "setChatTtl POST completes") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/${chat.id}/ttl" }
                }
                Thread.sleep(500)
                assertEquals(
                    "Stale refreshAllFromHost must NEVER revert setChatTtl back to 0",
                    86400,
                    vm.state.value.chats.first { it.id == chat.id.toString() }.ttlSeconds,
                )

                // 9) votePoll stale-refresh race: slower concurrent fetchServerMessagesForChat must NOT revert poll vote
                vm.sendPollOrQuiz(
                    conversationId = chat.id.toString(),
                    question = "Ship v2.3.0?",
                    options = listOf("Ship it", "Hold"),
                    isAnonymous = true,
                    multipleAnswers = false,
                    isQuiz = false,
                    correctIndex = 0,
                    explanation = null,
                )
                awaitState(vm, description = "poll message created with server pollId") { s ->
                    s.messagesByChat[chat.id.toString()].orEmpty().any { !it.pollId.isNullOrBlank() && it.poll != null }
                }
                val pollMsg = vm.state.value.messagesByChat[chat.id.toString()].orEmpty()
                    .first { !it.pollId.isNullOrBlank() && it.poll != null }
                val pollId = pollMsg.pollId!!
                val firstOptionId = pollMsg.poll!!.options.first().id
                val stalePollMessagesBody = """
                    {"chat_id":${chat.id},"has_more":false,"messages":[{
                      "id":${pollMsg.id.toLong()},
                      "chat_id":${chat.id},
                      "sender_id":$aliceId,
                      "sender_name":"Alice",
                      "text":"📊 Ship v2.3.0?",
                      "content_type":"poll",
                      "date_ms":${pollMsg.timestampMs},
                      "poll":{
                        "id":"$pollId",
                        "question":"Ship v2.3.0?",
                        "is_anonymous":true,
                        "multiple_choice":false,
                        "is_quiz":false,
                        "options":[
                          {"id":"$firstOptionId","text":"Ship it","voter_count":0,"chosen":false},
                          {"id":"opt-2","text":"Hold","voter_count":0,"chosen":false}
                        ]
                      }
                    }]}
                """.trimIndent()
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/chats/${chat.id}/messages",
                        status = 200,
                        errorBody = stalePollMessagesBody,
                        delayMs = 350L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                vm.openChat(chat.id.toString())
                Thread.sleep(60)
                vm.votePoll(chat.id.toString(), pollMsg.id, firstOptionId)
                awaitCondition(description = "votePoll POST completes") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/polls/$pollId/vote" }
                }
                Thread.sleep(480)
                val votedPoll = vm.state.value.messagesByChat[chat.id.toString()].orEmpty()
                    .first { it.id == pollMsg.id }
                    .poll!!
                assertTrue(
                    "Stale fetchServerMessagesForChat must NEVER revert chosen poll option",
                    votedPoll.options.first().chosen,
                )
                assertTrue(
                    "Stale fetchServerMessagesForChat must NEVER revert poll voterCount back to 0",
                    votedPoll.options.first().voterCount >= 1,
                )
            }
        }
    }

    // =========================================================================
    // H1: Atomic GenerationFence.compareAndAdvance + Pre-Login 401 Race
    // =========================================================================

    @Test
    fun t5_h1_generationFenceCompareAndAdvanceIsAtomicAndPreLogin401CannotWipeFreshSession() {
        // 1. Atomic CAS contention under 16 concurrent threads
        val fence = GenerationFence(1L)
        val threads = 16
        val startGate = CountDownLatch(1)
        val doneGate = CountDownLatch(threads)
        val winners = AtomicInteger(0)
        val pool = Executors.newFixedThreadPool(threads)
        try {
            repeat(threads) {
                pool.submit {
                    startGate.await()
                    if (fence.compareAndAdvance(1L)) {
                        winners.incrementAndGet()
                    }
                    doneGate.countDown()
                }
            }
            startGate.countDown()
            assertTrue(doneGate.await(5, TimeUnit.SECONDS))
        } finally {
            pool.shutdownNow()
        }
        assertEquals("Exactly one thread must win compareAndAdvance(1L)", 1, winners.get())
        assertEquals(2L, fence.currentGeneration)
        assertFalse(fence.compareAndAdvance(1L))

        // 2. End-to-end pre-login 401 race: delayed 401 from gen G1 arrives AFTER loginWithPassword starts
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fixture ->
                fixture.loginAndConnect(server)
                val vm = fixture.vm

                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/auth/sessions",
                        status = 401,
                        errorBody = """{"error":"unauthorized","message":"stale token"}""",
                        delayMs = 300L,
                        remainingHits = AtomicInteger(1),
                    ),
                )
                // Fire a request under old generation that will return 401 in 300ms
                vm.refreshSessions()
                Thread.sleep(40)
                // Start fresh login immediately — advances generationFence at entry BEFORE writing new token
                vm.loginWithPassword("alice", "password123")
                awaitState(vm, timeoutMs = 5_000L, description = "fresh login succeeds") { s ->
                    s.isAuthenticated && !s.isAuthenticating
                }
                // Wait past the 300ms delayed stale 401
                Thread.sleep(420)
                assertTrue(
                    "Stale pre-login 401 must NEVER wipe newly logged-in session",
                    vm.state.value.isAuthenticated,
                )
            }
        }
    }

    // =========================================================================
    // H2: Mika Stale askId Rejection (Confirm & Input) + Concurrent recordRecentTask
    // =========================================================================

    @Test
    fun t5_h2_mikaStaleAskIdRejectedOnConfirmAndInputAndConcurrentRecordRecentTaskIsThreadSafe() = runBlocking {
        val tempDir = Files.createTempDirectory("fabushi-mika-t5").toFile()
        val mikaApp = TestMikaApp(tempDir)
        val bridge = TelegramMikaChannelBridge(mikaApp)
        val ownerPeer = ChannelPeer(TelegramMikaChannel.ID, "user:1", "thread-1")
        bridge.owners.bind(ownerPeer)

        var activeAsk: Delivery.Ask? = null
        val confirmDecisions = CopyOnWriteArrayList<Pair<String, ConfirmChoice>>()
        val inputAnswers = CopyOnWriteArrayList<Pair<String, String>>()

        val gateway = ChannelGateway(
            owners = bridge.owners,
            activeAsk = { tid -> activeAsk?.takeIf { it.threadId == tid } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean {
                    inputAnswers.add(askId to text)
                    return true
                }
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                    confirmDecisions.add(askId to choice)
                    return true
                }
                override suspend fun cancelAsk(threadId: String) {
                    activeAsk = null
                }
                override suspend fun stop(runId: RunId) = Unit
            },
        )
        val reg = gateway.start(bridge.channel)
        try {
            // 1. Stale askId on resolveConfirmation must NOT approve newer ask or clear pending task state
            val confirmAsk = Delivery.Ask("d-c1", "thread-1", "ask-confirm-current", Delivery.Ask.Kind.Confirm, "Confirm payment?")
            activeAsk = confirmAsk
            bridge.channel.send(ownerPeer, confirmAsk)
            mikaApp.taskState._state.value = TaskState.WaitingConfirmation(
                ConfirmationRequest(
                    action = IrrevAction.pay,
                    target = "Checkout",
                    content = "Irreversible payment",
                    amount = "100",
                    app = "com.example.pay",
                ),
            )

            bridge.resolveConfirmation(approved = true, threadId = "thread-1", askId = "ask-confirm-STALE")
            delay(80)
            assertTrue("Stale confirm askId must produce zero confirmations", confirmDecisions.isEmpty())
            assertNotNull("Pending channel confirm ask must remain intact", bridge.channel.pendingAsk("thread-1"))
            assertTrue(
                "TaskState.WaitingConfirmation must remain intact on stale askId",
                mikaApp.taskState.state.value is TaskState.WaitingConfirmation,
            )

            // Valid matching askId succeeds
            bridge.resolveConfirmation(approved = true, threadId = "thread-1", askId = "ask-confirm-current")
            delay(80)
            assertEquals(listOf("ask-confirm-current" to ConfirmChoice.Once), confirmDecisions.toList())

            // 2. Stale askId on answerAsk must return false WITHOUT falling through to app.answerPhoneTask
            val inputAsk = Delivery.Ask("d-i1", "thread-1", "ask-input-current", Delivery.Ask.Kind.Input, "Which address?")
            activeAsk = inputAsk
            bridge.channel.send(ownerPeer, inputAsk)
            mikaApp.taskState._state.value = TaskState.WaitingInput(prompt = "Which address?")

            val staleInputAccepted = bridge.answerAsk("123 Main St", threadId = "thread-1", askId = "ask-input-STALE")
            assertFalse("Stale input askId must return false", staleInputAccepted)
            assertTrue("Stale input askId must NOT dispatch answer", inputAnswers.isEmpty())

            // Even when channel has NO pending input ask, passing an explicit stale askId must return false
            val staleOnOtherThread = bridge.answerAsk("123 Main St", threadId = "thread-other", askId = "ask-expired-99")
            assertFalse("Explicit stale askId with no pending channel ask must return false", staleOnOtherThread)

            // Matching askId succeeds
            val validInputAccepted = bridge.answerAsk("456 Lotus Rd", threadId = "thread-1", askId = "ask-input-current")
            assertTrue(validInputAccepted)
            delay(80)
            assertEquals(listOf("ask-input-current" to "456 Lotus Rd"), inputAnswers.toList())

            // 3. Concurrent recordRecentTask stress test (zero ArrayIndexOutOfBoundsException, bounded <= 15)
            val workers = 8
            val perWorker = 80
            val pool = Executors.newFixedThreadPool(workers)
            val failure = AtomicReference<Throwable?>(null)
            val latch = CountDownLatch(workers)
            try {
                for (w in 0 until workers) {
                    pool.submit {
                        try {
                            for (i in 0 until perWorker) {
                                bridge.recordRecentTask(
                                    MikaTaskSummaryItem(
                                        taskId = "task-$w-$i",
                                        prompt = "Goal $w-$i",
                                        status = "running",
                                        latestStep = "Step $i",
                                        updatedAtLabel = "10:00",
                                    ),
                                )
                            }
                        } catch (t: Throwable) {
                            failure.compareAndSet(null, t)
                        } finally {
                            latch.countDown()
                        }
                    }
                }
                assertTrue(latch.await(10, TimeUnit.SECONDS))
            } finally {
                pool.shutdownNow()
            }
            assertNull("Concurrent recordRecentTask must never throw", failure.get())
            assertTrue("Recent tasks list must stay bounded <= 15", bridge.recentTasksSnapshot().size <= 15)
        } finally {
            reg.close()
            bridge.close()
            tempDir.deleteRecursively()
        }
    }

    // =========================================================================
    // H3: FabushiRemoteDeviceGateway.close() Shutdown Race Safety
    // =========================================================================

    @Test
    fun t5_h3_remoteDeviceGatewayCloseRaceSafetyRejectsReconnectAndPostCloseSetLoggedIn() {
        val app = TestFabushiApplication()
        val surface = FabushiAppAgentSurface()
        val gateway = FabushiRemoteDeviceGateway(
            context = app,
            surface = surface,
            metadata = mapOf("test" to "h3"),
            gatewayUrl = MockGpuiTelegramServer.closedLoopbackUrl().replace("http://", "ws://") + "/agent",
            retryDelayMs = 50L,
        )
        val failure = AtomicReference<Throwable?>(null)
        val pool = Executors.newFixedThreadPool(6)
        val latch = CountDownLatch(6)
        try {
            repeat(5) { idx ->
                pool.submit {
                    try {
                        repeat(30) { step ->
                            gateway.setLoggedIn((idx + step) % 2 == 0)
                            Thread.sleep(2)
                        }
                    } catch (t: Throwable) {
                        failure.compareAndSet(null, t)
                    } finally {
                        latch.countDown()
                    }
                }
            }
            pool.submit {
                try {
                    Thread.sleep(15)
                    gateway.close()
                    // Repeated close() and post-close setLoggedIn(true) must be safe no-ops
                    gateway.close()
                    gateway.setLoggedIn(true)
                } catch (t: Throwable) {
                    failure.compareAndSet(null, t)
                } finally {
                    latch.countDown()
                }
            }
            assertTrue(latch.await(10, TimeUnit.SECONDS))
            assertNull("Concurrent setLoggedIn/close must never throw RejectedExecutionException", failure.get())
            assertTrue(gateway.isClosed())
            assertFalse(gateway.isRegistered())
        } finally {
            pool.shutdownNow()
            gateway.close()
            app.cleanup()
        }
    }

    // =========================================================================
    // H4: WS Ack Deduplication & Monotonic "read" Preservation
    // =========================================================================

    @Test
    fun t5_h4_wsAckDeduplicatesAndPreservesMonotonicReadState() {
        MockGpuiTelegramServer().use { server ->
            val aliceId = 1L
            val bobId = 2L
            val chat = MockGpuiTelegramServer.MockChat(
                id = 2100L,
                ownerUserId = aliceId,
                title = "Ack Race Chat",
                kind = "direct",
                peerUserId = bobId,
            )
            server.seedChat(chat)
            E2eViewModelFixture().use { fixture ->
                fixture.loginAndConnect(server)
                val vm = fixture.vm
                val gen = vm.currentGenerationForTest()

                // 1. Outgoing message delivered via WS NewMessage with server id=700 and client_nonce="nonce-700"
                val newMsgJson = JSONObject()
                    .put("id", 700L)
                    .put("chat_id", chat.id)
                    .put("sender_id", aliceId)
                    .put("sender_name", "Alice")
                    .put("text", "Hello Bob")
                    .put("client_nonce", "nonce-700")
                    .put("is_outgoing", true)
                    .put("status", "sent")
                    .put("date_ms", System.currentTimeMillis())
                vm.applyIncomingNewMessage(newMsgJson, chat.id)

                // 2. Mark outbox read up to 700 via ReadHistoryOut
                val readFrame = JSONObject()
                    .put("type", "Update")
                    .put(
                        "payload",
                        JSONObject()
                            .put("type", "ReadHistoryOut")
                            .put("data", JSONObject().put("chat_id", chat.id).put("max_id", 700L)),
                    )
                vm.handleWsServerFrame(readFrame, gen)
                val afterRead = vm.state.value.messagesByChat[chat.id.toString()].orEmpty().first { it.id == "700" }
                assertEquals("read", afterRead.deliveryState)

                // 3. Delayed WS Ack arrives AFTER NewMessage + ReadHistoryOut
                val ackFrame = JSONObject()
                    .put("type", "Ack")
                    .put(
                        "payload",
                        JSONObject()
                            .put("chat_id", chat.id)
                            .put("message_id", 700L)
                            .put("client_nonce", "nonce-700"),
                    )
                vm.handleWsServerFrame(ackFrame, gen)

                val msgsAfterAck = vm.state.value.messagesByChat[chat.id.toString()].orEmpty()
                assertEquals("WS Ack must not create duplicate bubble", 1, msgsAfterAck.count { it.id == "700" })
                assertEquals(
                    "WS Ack must NEVER downgrade deliveryState from read back to sent",
                    "read",
                    msgsAfterAck.first { it.id == "700" }.deliveryState,
                )

                // 4. WS Ack with ART client_nonce: null must be ignored cleanly
                val nullNonceAck = ArtEmulatingJSONObject(
                    """{"type":"Ack","payload":{"chat_id":${chat.id},"message_id":999,"client_nonce":null}}""",
                )
                vm.handleWsServerFrame(nullNonceAck, gen)
                assertEquals(1, vm.state.value.messagesByChat[chat.id.toString()].orEmpty().count { it.id == "700" })
            }
        }
    }

    // =========================================================================
    // H5: parseInlineKeyboard Precedence Alignment
    // =========================================================================

    @Test
    fun t5_h5_parseInlineKeyboardPrecedenceMatchesKindAndPayload() {
        E2eViewModelFixture().use { fixture ->
            val vm = fixture.vm
            val row = JSONArray()
                .put(
                    JSONObject()
                        .put("text", "MiniApp Priority")
                        .put("web_app_url", "https://miniapp.example/app")
                        .put("url", "https://example.com/fallback")
                        .put("callback_data", "cb_should_not_win"),
                )
                .put(
                    JSONObject()
                        .put("text", "Url Priority")
                        .put("url", "https://example.com/docs")
                        .put("callback_data", "cb_should_not_win"),
                )
                .put(
                    JSONObject()
                        .put("text", "Callback Only")
                        .put("callback_data", "cb_action_1"),
                )
            val rows = vm.parseInlineKeyboard(JSONArray().put(row))
            assertNotNull(rows)
            val buttons = rows!![0].buttons
            assertEquals(3, buttons.size)

            assertEquals(InlineKeyboardButtonKind.MINI_APP, buttons[0].kind)
            assertEquals("https://miniapp.example/app", buttons[0].payload)

            assertEquals(InlineKeyboardButtonKind.URL, buttons[1].kind)
            assertEquals("https://example.com/docs", buttons[1].payload)

            assertEquals(InlineKeyboardButtonKind.CALLBACK, buttons[2].kind)
            assertEquals("cb_action_1", buttons[2].payload)
        }
    }

    // =========================================================================
    // MiniApp H3 & M5 + GlobalDharma M7: XSS / Regex Escaping & Approval Fail-Closed
    // =========================================================================

    @Test
    fun t5_miniAppH3AndM5_scriptEscapingRegexReplacementAndFailClosedApprovalPrompt() {
        val plugin = MarketplacePlugin(
            pluginId = "global-dharma</script><img src=x>",
            displayName = "Global Dharma",
            description = "Test",
            latestVersion = "2.3.0",
            tools = listOf(
                MiniAppToolContract(
                    name = "transfer.merit",
                    title = "Offer Merit",
                    description = "Offer merit",
                    approval = "confirm",
                ),
            ),
        )
        val adversarialAccount = JSONObject()
            .put("loggedIn", true)
            .put(
                "user",
                JSONObject()
                    .put("nickname", "Price \$100 \$0 \\path <!-- <script></script>\u2028\u2029"),
            )
        val html = "<html><head><title>App</title></head><body><pre>Literal </head> in body</pre></body></html>"

        val injected = injectLocalWebMcp(html, plugin, adversarialAccount)
        // 1. Must not throw IndexOutOfBoundsException on $1/$0/\ and must preserve literals
        assertTrue(injected.contains("Price \$100 \$0"))
        // 2. Must inject bootstrap EXACTLY ONCE (not twice on body's </head>)
        assertEquals(1, injected.split("__fabushiWebMcp").size - 1)
        assertTrue(injected.contains("<pre>Literal </head> in body</pre>"))
        // 3. Must escape <, U+2028, U+2029 inside the injected <script> block
        val scriptStart = injected.indexOf("<script>")
        val scriptEnd = injected.indexOf("</script>", scriptStart)
        assertTrue(scriptStart >= 0 && scriptEnd > scriptStart)
        val scriptBody = injected.substring(scriptStart + "<script>".length, scriptEnd)
        assertFalse("Raw <!-- must not appear inside script body", scriptBody.contains("<!--"))
        assertFalse("Raw </script> must not appear inside script body", scriptBody.contains("</script>"))
        assertFalse("Raw U+2028 must be escaped", scriptBody.contains("\u2028"))
        assertFalse("Raw U+2029 must be escaped", scriptBody.contains("\u2029"))
        assertTrue(scriptBody.contains("\\u003c"))

        // 4. MiniAppNativeWebMcpBridge fail-closed when approvalPrompt throws (BadTokenException) or denies
        val app = TestFabushiApplication()
        try {
            val runtimeInvocations = AtomicInteger(0)
            val throwingBridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { true },
                callRuntimeToolJson = { _, _, _ ->
                    runtimeInvocations.incrementAndGet()
                    """{"done":true}"""
                },
                approvalPrompt = { throw RuntimeException("WindowManager\$BadTokenException") },
            )
            val throwResp = JSONObject(throwingBridge.callTool("transfer.merit", "{}"))
            assertFalse("Throwing approvalPrompt must fail closed", throwResp.getBoolean("ok"))
            assertEquals(0, runtimeInvocations.get())

            val denyingBridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { true },
                callRuntimeToolJson = { _, _, _ ->
                    runtimeInvocations.incrementAndGet()
                    """{"done":true}"""
                },
                approvalPrompt = { false },
            )
            val denyResp = JSONObject(denyingBridge.callTool("transfer.merit", "{}"))
            assertFalse("Denied approvalPrompt must fail closed", denyResp.getBoolean("ok"))
            assertEquals(0, runtimeInvocations.get())
        } finally {
            app.cleanup()
        }

        // 5. GlobalDharma WebMCP HTML country code sanitization (M7)
        assertEquals("CN", sanitizeGlobalDharmaCountryCode("CN',alert(1)//"))
        assertEquals("US", sanitizeGlobalDharmaCountryCode("US"))
        val dharmaHtml = buildEmbeddedGlobalDharmaWebMcpHtml(
            GlobalDharmaUiState(selectedCountryCode = "CN',alert(1)//"),
        )
        assertFalse(dharmaHtml.contains("alert(1)"))
        assertTrue(dharmaHtml.contains("{code:'CN'"))
    }

    // =========================================================================
    // M1, M5, M6: Rich Entities, Safe Clickable Links & Reserved Folder Filtering
    // =========================================================================

    @Test
    fun t5_m1m5m6_richEntitiesClickableSafeUrlsAndReservedFolderIdsFiltered() {
        val text = "Bold Italic Under Strike Code Secret Visit BadLink"
        val entities = listOf(
            MessageEntity(MessageEntityType.BOLD, 0, 4),
            MessageEntity(MessageEntityType.ITALIC, 5, 6),
            MessageEntity(MessageEntityType.UNDERLINE, 12, 5),
            MessageEntity(MessageEntityType.STRIKETHROUGH, 18, 6),
            MessageEntity(MessageEntityType.CODE, 25, 4),
            MessageEntity(MessageEntityType.SPOILER, 30, 6),
            MessageEntity(MessageEntityType.TEXT_URL, 37, 5, url = "https://fabushi.ombhrum.com"),
            MessageEntity(MessageEntityType.TEXT_URL, 43, 7, url = "javascript:alert(1)"),
        )
        val styled = buildRichMessageAnnotatedString(
            text = text,
            entities = entities,
            linkColor = Color(0xFF007AFF),
            codeBg = Color(0xFFE6E8EB),
            textColor = Color(0xFF1D1D1F),
            spoilerMaskColor = Color(0xFF8E8E93),
            revealSpoilers = false,
        )
        assertTrue(styled.spanStyles.any { it.item.fontWeight == FontWeight.Bold && it.start == 0 && it.end == 4 })
        assertTrue(styled.spanStyles.any { it.item.fontStyle == FontStyle.Italic && it.start == 5 && it.end == 11 })
        assertTrue(styled.spanStyles.any { it.item.textDecoration == TextDecoration.Underline && it.start == 12 && it.end == 17 })
        assertTrue(styled.spanStyles.any { it.item.textDecoration == TextDecoration.LineThrough && it.start == 18 && it.end == 24 })
        assertTrue(styled.spanStyles.any { it.item.fontFamily == FontFamily.Monospace && it.start == 25 && it.end == 29 })
        assertTrue(styled.spanStyles.any { it.item.background == Color(0xFF8E8E93) && it.start == 30 && it.end == 36 })

        val links = styled.getLinkAnnotations(0, text.length)
        assertEquals("Only safe http(s) URL must produce LinkAnnotation", 1, links.size)
        val urlAnn = links[0].item as LinkAnnotation.Url
        assertEquals("https://fabushi.ombhrum.com", urlAnn.url)

        assertEquals("https://fabushi.ombhrum.com", extractFirstSafeHttpUrl(text, entities))
        assertNull(extractFirstSafeHttpUrl("javascript:alert(1) file:///etc/passwd"))
        assertEquals("https://example.org/test", extractFirstSafeHttpUrl("Check https://example.org/test now"))

        // M5: Reserved built-in folder tab IDs are filtered out in parseServerFolders
        E2eViewModelFixture().use { fixture ->
            val foldersResp = JSONObject().put(
                "folders",
                JSONArray()
                    .put(JSONObject().put("id", "all").put("title", "Fake All"))
                    .put(JSONObject().put("id", "unread").put("title", "Fake Unread"))
                    .put(JSONObject().put("id", "pinned").put("title", "Fake Pinned"))
                    .put(JSONObject().put("id", "bots").put("title", "Fake Bots"))
                    .put(JSONObject().put("id", "groups").put("title", "Fake Groups"))
                    .put(JSONObject().put("id", "channels").put("title", "Fake Channels"))
                    .put(JSONObject().put("id", 77L).put("title", "Real Custom")),
            )
            val parsed = fixture.vm.parseServerFolders(foldersResp)
            assertEquals(listOf("77"), parsed.map { it.id })
        }
    }

    // =========================================================================
    // M3, M4, Polish: Cloud Drafts, Authenticated Location/Contact & Assist Code
    // =========================================================================

    @Test
    fun t5_m3m4AndPolish_cloudDraftsLocationContactPersistenceAndAssistCodeRule() {
        MockGpuiTelegramServer().use { server ->
            val aliceId = 1L
            val bobId = 2L
            val chat = MockGpuiTelegramServer.MockChat(
                id = 2200L,
                ownerUserId = aliceId,
                title = "Bob",
                kind = "direct",
                peerUserId = bobId,
            )
            server.seedChat(chat)
            val chatId = chat.id.toString()

            val locApp = TestFabushiApplication(
                grantedPermissions = mutableSetOf(
                    android.Manifest.permission.ACCESS_FINE_LOCATION,
                    android.Manifest.permission.ACCESS_COARSE_LOCATION,
                ),
            )
            E2eViewModelFixture(app = locApp).use { fixture ->
                fixture.loginAndConnect(server)
                val vm = fixture.vm

                // 1) M3: Cloud draft saved on closeChat(), cleared on sendComposerMessage() & blank closeChat()
                vm.openChat(chatId)
                awaitState(vm, description = "chat opened") { it.activeChatId == chatId }
                vm.updateDraft(chatId, "Unfinished draft text")
                vm.closeChat()
                awaitCondition(description = "PUT /draft recorded on closeChat") {
                    server.recordedRequests.any {
                        it.method == "PUT" && it.path == "/api/v1/chats/${chat.id}/draft" &&
                            it.body.contains("Unfinished draft text")
                    }
                }

                vm.openChat(chatId)
                awaitState(vm, description = "chat reopened") { it.activeChatId == chatId }
                val deletesBeforeSend = server.recordedRequests.count {
                    it.method == "DELETE" && it.path == "/api/v1/chats/${chat.id}/draft"
                }
                vm.sendComposerMessage(chatId, "Sent message clearing draft")
                awaitCondition(description = "DELETE /draft recorded on sendComposerMessage") {
                    server.recordedRequests.count {
                        it.method == "DELETE" && it.path == "/api/v1/chats/${chat.id}/draft"
                    } > deletesBeforeSend
                }

                // Blank draft closeChat clears draft on server
                val deletesBeforeClose = server.recordedRequests.count {
                    it.method == "DELETE" && it.path == "/api/v1/chats/${chat.id}/draft"
                }
                vm.closeChat()
                awaitCondition(description = "DELETE /draft recorded on blank closeChat") {
                    server.recordedRequests.count {
                        it.method == "DELETE" && it.path == "/api/v1/chats/${chat.id}/draft"
                    } > deletesBeforeClose
                }

                // 2) M4: Authenticated sendLocation & sendContactCard persist via telegram.server.sendMessage
                vm.openChat(chatId)
                awaitState(vm, description = "chat reopened for location/contact") { it.activeChatId == chatId }
                vm.sendLocation(chatId, 37.3349, -122.0090, "Apple Park")
                awaitCondition(description = "location sent to /api/v1/messages/send") {
                    server.recordedRequests.any {
                        it.method == "POST" && it.path == "/api/v1/messages/send" &&
                            it.body.contains("Apple Park")
                    }
                }
                awaitState(vm, description = "location message transitioned to sent with numeric id") { s ->
                    s.messagesByChat[chatId].orEmpty().any {
                        it.contentType == "location" && it.deliveryState == "sent" && it.id.toLongOrNull() != null
                    }
                }

                vm.sendContactCard(chatId, "Dr. Dharma", "+8613800138000")
                awaitCondition(description = "contact card sent to /api/v1/messages/send") {
                    server.recordedRequests.any {
                        it.method == "POST" && it.path == "/api/v1/messages/send" &&
                            it.body.contains("Dr. Dharma")
                    }
                }
                awaitState(vm, description = "contact card transitioned to sent with numeric id") { s ->
                    s.messagesByChat[chatId].orEmpty().any {
                        it.contentType == "contact" && it.deliveryState == "sent" && it.id.toLongOrNull() != null
                    }
                }

                // 3) Polish: updateRemoteAssistCode enforces letter + digit check
                val assistSetBefore = server.recordedRequests.count { it.path == "/api/v1/remote/assist/set-code" }
                vm.updateRemoteAssistCode("12345678") // digits only -> rejected
                vm.updateRemoteAssistCode("abcdefgh") // letters only -> rejected
                Thread.sleep(100)
                assertEquals(
                    "Digits-only and letters-only codes must be rejected before RPC",
                    assistSetBefore,
                    server.recordedRequests.count { it.path == "/api/v1/remote/assist/set-code" },
                )
                vm.updateRemoteAssistCode("Abcd1234") // letter + digit -> accepted
                awaitCondition(description = "valid assist code hits server") {
                    server.recordedRequests.count { it.path == "/api/v1/remote/assist/set-code" } == assistSetBefore + 1
                }
            }
        }
    }
}
