package com.ombhrum.fabushi.e2e

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.lifecycle.Lifecycle
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.FabushiRemoteDeviceGateway
import com.ombhrum.fabushi.MarketplacePlugin
import com.ombhrum.fabushi.MiniAppLifecycleGate
import com.ombhrum.fabushi.MiniAppNativeWebMcpBridge
import com.ombhrum.fabushi.MiniAppToolContract
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.e2e.harness.E2eViewModelFixture
import com.ombhrum.fabushi.e2e.harness.FaultRule
import com.ombhrum.fabushi.e2e.harness.MainDispatcherRule
import com.ombhrum.fabushi.e2e.harness.MockGpuiTelegramServer
import com.ombhrum.fabushi.e2e.harness.TestFabushiApplication
import com.ombhrum.fabushi.e2e.harness.awaitCondition
import com.ombhrum.fabushi.e2e.harness.awaitState
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import com.ombhrum.fabushi.telegram.MikaBridgeCallbacks
import com.ombhrum.fabushi.telegram.MikaBridgeUiSnapshot
import com.ombhrum.fabushi.telegram.TelegramSubScreen
import com.ombhrum.fabushi.telegram.TelegramThemePreset
import com.ombhrum.fabushi.telegram.TelegramThemeSettings
import com.ombhrum.fabushi.telegram.publishTelegramSemanticSurface
import com.ombhrum.fabushi.telegram.resolveTelegramPalette
import com.ombhrum.fabushi.telegram.resolveUiStrings
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Tier 3 — Cross-Feature Pairwise Integration E2E Suite (10 Tests: E2E-T3-01 .. E2E-T3-10).
 */
class E2eTier3PairwiseIntegrationTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    private fun contrastRatio(a: Color, b: Color): Double {
        val la = a.luminance().toDouble()
        val lb = b.luminance().toDouble()
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    @Test
    fun e2e_t3_01_pinChatAntiFlickerUnderConcurrentWsMessageAndRefresh() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Pinned Alpha", isPinned = true, lastMessageDateMs = 1_000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(102L, 1L, "Recent Beta", isPinned = false, lastMessageDateMs = 5_000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                val init = awaitState(fix.vm) { it.chats.size == 2 }
                assertEquals(listOf("101", "102"), init.chats.map { it.id })

                // Slow down POST /api/v1/chats/101/pin by 450ms so concurrent refresh sees server's stale is_pinned=true
                server.addFault(
                    FaultRule(
                        method = "POST",
                        pathPrefix = "/api/v1/chats/101/pin",
                        status = 0,
                        delayMs = 450L,
                        remainingHits = AtomicInteger(1),
                    ),
                )

                val chatGetsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats" }
                fix.vm.togglePinChat("101")
                assertFalse("Optimistic unpin should flip immediately", fix.vm.state.value.chats.first { it.id == "101" }.isPinned)
                awaitCondition(description = "POST /api/v1/chats/101/pin entered 450ms server delay") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/pin" }
                }

                // While unpin is in-flight, push WS NewMessage on 102 and trigger refreshAllFromHost()
                server.pushWsEvent("""{"type":"Update","payload":{"type":"NewMessage","data":{"id":901,"chat_id":102,"sender_id":2,"sender_name":"Bob","text":"Concurrent ping","date_ms":6000}}}""")
                fix.vm.refreshAllFromHost()

                // Verify concurrent GET /api/v1/chats arrives WHILE server still has isPinned == true, without flickering UI!
                var sawConcurrentGetWhileInFlight = false
                awaitCondition(timeoutMs = 5_000L, pollMs = 15L, description = "concurrent GET /api/v1/chats during in-flight unpin") {
                    assertFalse("Chat 101 flickered back to pinned!", fix.vm.state.value.chats.first { it.id == "101" }.isPinned)
                    val getsNow = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats" }
                    if (getsNow > chatGetsBefore && server.getChatSnapshot(101L)?.isPinned == true) {
                        sawConcurrentGetWhileInFlight = true
                    }
                    getsNow > chatGetsBefore
                }
                assertTrue("Concurrent GET /api/v1/chats must execute while server still holds isPinned == true", sawConcurrentGetWhileInFlight)

                // Wait for server pin write to finish without flickering, then verify final order sorted by lastMessageAtMs
                awaitCondition(timeoutMs = 5_000L, pollMs = 15L, description = "server unpin persisted") {
                    assertFalse("Chat 101 flickered back to pinned!", fix.vm.state.value.chats.first { it.id == "101" }.isPinned)
                    server.getChatSnapshot(101L)?.isPinned == false
                }
                val settled = awaitState(fix.vm) {
                    !it.chats.first { c -> c.id == "101" }.isPinned && it.chats.map { c -> c.id } == listOf("102", "101")
                }
                assertFalse(settled.chats.first { it.id == "101" }.isPinned)
                assertEquals(listOf("102", "101"), settled.chats.map { it.id })
            }
        }
    }

    @Test
    fun e2e_t3_02_botCreationComputerBindingAndLiveBotActivityChangedWs() {
        MockGpuiTelegramServer().use { server ->
            server.seedComputer(
                MockGpuiTelegramServer.MockComputer(
                    nodeId = "cloud:1:prod-1",
                    ownerUserId = 1L,
                    displayName = "Prod Runner",
                    host = "10.0.0.10",
                    online = true,
                    runPolicy = "auto",
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.createFabushiBot("Agent Runner", "runner_ai_bot", "Runs prod commands")
                val created = awaitState(fix.vm) { it.fabushiBots.any { b -> b.username == "runner_ai_bot" } }
                val bot = created.fabushiBots.first { it.username == "runner_ai_bot" }
                val botIdStr = bot.botUserId.toString()
                val chatIdStr = bot.companionChatId.toString()

                // Bind cloud:1:prod-1 to bot
                fix.vm.setBotComputer(botIdStr, "cloud:1:prod-1")
                awaitState(fix.vm) { it.botComputerBindings[botIdStr] == "cloud:1:prod-1" }
                assertTrue(server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/bots/$botIdStr/computer" })

                // Open companion chat (already populated by createFabushiBot) and await initial GET /messages
                awaitState(fix.vm) { it.chats.any { c -> c.id == chatIdStr } }
                fix.vm.openChat(chatIdStr)
                awaitCondition(description = "openChat GET /messages completed") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats/$chatIdStr/messages" }
                }
                awaitState(fix.vm) { it.activeChatId == chatIdStr && it.messagesByChat.containsKey(chatIdStr) }
                fix.vm.sendComposerMessage(chatIdStr, "Check disk usage on prod-1")
                awaitState(fix.vm) { it.messagesByChat[chatIdStr]?.any { m -> m.text.contains("disk usage") && m.deliveryState == "sent" } == true }

                // WS BotActivityChanged("running_tool") -> NewMessage -> BotActivityChanged("idle")
                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":${bot.companionChatId},"bot_user_id":${bot.botUserId},"state":"running_tool","detail":"shell_exec df -h"}}}""",
                )
                val running = awaitState(fix.vm) { it.chats.first { c -> c.id == chatIdStr }.botExecutionState == "running_tool" }
                assertTrue(running.chats.first { it.id == chatIdStr }.typingLabel.orEmpty().contains("df -h"))

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"NewMessage","data":{"id":9501,"chat_id":${bot.companionChatId},"sender_id":${bot.botUserId},"sender_name":"Agent Runner","text":"Disk usage is 32%","date_ms":9000}}}""",
                )
                awaitState(fix.vm) { it.messagesByChat[chatIdStr]?.any { m -> m.text == "Disk usage is 32%" } == true }

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":${bot.companionChatId},"bot_user_id":${bot.botUserId},"state":"idle"}}}""",
                )
                val idle = awaitState(fix.vm) { it.chats.first { c -> c.id == chatIdStr }.botExecutionState == "idle" }
                assertTrue(idle.chats.first { it.id == chatIdStr }.typingLabel.isNullOrBlank())
            }
        }
    }

    @Test
    fun e2e_t3_03_chatAndMikaConfirmationGateApproveAndDeclineEndToEnd() = runBlocking {
        val channel = TelegramMikaChannel()
        val owners = MemoryOwnerIdentity()
        val ownerPeer = ChannelPeer(TelegramMikaChannel.ID, "user:1", "saved-local")
        owners.bind(ownerPeer)

        val activeAskRef = AtomicReference<Delivery.Ask?>(null)
        val stepSinkRef = AtomicReference<((String) -> Unit)?>(null)
        val confirmLog = CopyOnWriteArrayList<Pair<String, ConfirmChoice>>()

        val gateway = ChannelGateway(
            owners = owners,
            activeAsk = { tid -> activeAskRef.get()?.takeIf { it.threadId == tid } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean = true
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                    confirmLog.add(askId to choice)
                    activeAskRef.set(null)
                    if (choice == ConfirmChoice.Once) {
                        stepSinkRef.get()?.invoke("任务完成：已执行确认操作")
                    } else {
                        stepSinkRef.get()?.invoke("任务已取消：用户拒绝确认")
                    }
                    return true
                }
                override suspend fun cancelAsk(threadId: String) { activeAskRef.set(null) }
                override suspend fun stop(runId: RunId) = Unit
            },
        )
        val reg = gateway.start(channel)
        try {
            E2eViewModelFixture().use { fix ->
                fix.vm.openSavedMessages()
                val chatId = awaitState(fix.vm) { it.activeChatId != null }.activeChatId!!
                val peerForChat = ChannelPeer(TelegramMikaChannel.ID, "user:1", chatId)
                owners.bind(peerForChat)

                var askSeq = 0
                fix.vm.attachMikaBridge(
                    object : MikaBridgeCallbacks {
                        override fun currentSnapshot(): MikaBridgeUiSnapshot = MikaBridgeUiSnapshot(
                            available = true,
                            sandboxReady = true,
                            pendingAskId = activeAskRef.get()?.askId,
                            pendingAskThreadId = activeAskRef.get()?.threadId,
                            pendingConfirmationPrompt = activeAskRef.get()?.text,
                        )

                        override fun submitPhoneTask(prompt: String, sourceConversationId: String?, onStepUpdate: (String) -> Unit) {
                            val tid = sourceConversationId ?: chatId
                            stepSinkRef.set(onStepUpdate)
                            val askId = "ask-mika-${++askSeq}"
                            val ask = Delivery.Ask("del-$askSeq", tid, askId, Delivery.Ask.Kind.Confirm, "Confirm: $prompt")
                            activeAskRef.set(ask)
                            runBlocking { channel.send(peerForChat, ask) }
                            onStepUpdate("待确认：$prompt")
                        }

                        override fun stopActiveTask() = Unit

                        override fun resolveConfirmation(approved: Boolean, threadId: String?, askId: String?) {
                            val tid = threadId ?: chatId
                            runBlocking { channel.resolveConfirmationForThread(tid, approved, askId) }
                        }

                        override fun answerAsk(text: String, threadId: String?, askId: String?): Boolean = true
                    },
                )

                // 1. Approve flow
                fix.vm.runWithMikaFromChat(chatId, "Toggle Wi-Fi")
                val waiting1 = awaitState(fix.vm) {
                    it.messagesByChat[chatId]?.any { m -> m.mikaTaskStatus == "waiting_confirmation" } == true
                }
                assertEquals("ask-mika-1", waiting1.mikaSnapshot.pendingAskId)
                fix.vm.resolveMikaConfirmation(approved = true, threadId = chatId, askId = "ask-mika-1")
                awaitState(fix.vm) {
                    it.messagesByChat[chatId]?.any { m -> m.mikaTaskStatus == "completed" } == true
                }
                assertEquals("ask-mika-1" to ConfirmChoice.Once, confirmLog.last())

                // 2. Decline flow
                fix.vm.runWithMikaFromChat(chatId, "Factory reset phone")
                awaitState(fix.vm) { it.mikaSnapshot.pendingAskId == "ask-mika-2" }
                fix.vm.resolveMikaConfirmation(approved = false, threadId = chatId, askId = "ask-mika-2")
                awaitState(fix.vm) {
                    it.messagesByChat[chatId]?.any { m -> m.mikaTaskStatus == "failed" && m.mikaStepLogs.any { l -> l.contains("拒绝") } } == true
                }
                assertEquals("ask-mika-2" to ConfirmChoice.Deny, confirmLog.last())
            }
        } finally {
            reg.close()
            channel.close()
        }
    }

    @Test
    fun e2e_t3_04_authRemoteDeviceGatewayAndSemanticSurfaceFullCycle() = runBlocking {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Gateway Chat", lastMessageDateMs = 2000L))
            E2eViewModelFixture().use { fix ->
                val surface = FabushiAppAgentSurface()
                val gateway = FabushiRemoteDeviceGateway(
                    context = fix.app,
                    surface = surface,
                    metadata = mapOf("suite" to "e2e_t3_04"),
                    gatewayUrl = "${server.wsBaseUrl}/agent",
                    retryDelayMs = 100L,
                )
                try {
                    fix.loginAndConnect(server)
                    fix.vm.openChat("101")
                    val opened = awaitState(fix.vm) { it.activeChatId == "101" }
                    publishTelegramSemanticSurface(surface, opened, fix.vm)

                    gateway.setLoggedIn(true)
                    awaitCondition(timeoutMs = 5_000L, description = "gateway registered") { gateway.isRegistered() }

                    val snap = gateway.invokeTool(FabushiAppAgentSurface.SnapshotTool, JSONObject())
                    val gen = snap.getLong("generation")
                    gateway.invokeTool(
                        FabushiAppAgentSurface.ActionTool,
                        JSONObject()
                            .put("generation", gen)
                            .put("agentId", "telegram.chat.composer:101")
                            .put("action", "setValue")
                            .put("value", "Driven via MCP Gateway"),
                    )
                    assertEquals("Driven via MCP Gateway", fix.vm.state.value.chats.first { it.id == "101" }.draftText)

                    val gen2 = surface.snapshot().generation
                    gateway.invokeTool(
                        FabushiAppAgentSurface.ActionTool,
                        JSONObject().put("generation", gen2).put("agentId", "telegram.nav.settings").put("action", "invoke"),
                    )
                    assertEquals(TelegramSubScreen.SETTINGS, fix.vm.state.value.subScreen)

                    fix.vm.signOutToAuthWizard()
                    awaitState(fix.vm) { !it.isAuthenticated }
                    gateway.setLoggedIn(false)
                    assertFalse(gateway.isRegistered())
                } finally {
                    gateway.close()
                }
            }
        }
    }

    @Test
    fun e2e_t3_05_searchPaginationAndMediaOnlyMessagesTogether() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Archive Chat", lastMessageDateMs = 65_000L))
            val photoIds = setOf(5, 10, 20, 40, 60)
            val voiceIds = setOf(3, 12, 30, 55)
            for (i in 1..65) {
                when {
                    i in photoIds -> server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "", contentType = "photo", dateMs = 1_000L * i,
                            fileId = "f_photo_$i", filename = "img_$i.jpg", mimeType = "image/jpeg",
                        ),
                    )
                    i in voiceIds -> server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "", contentType = "voice", dateMs = 1_000L * i,
                            fileId = "f_voice_$i", filename = "voice_$i.ogg", mimeType = "audio/ogg",
                        ),
                    )
                    i == 7 -> server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "Unique golden needle keyword", dateMs = 1_000L * i,
                        ),
                    )
                    else -> server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "Regular entry #$i", dateMs = 1_000L * i,
                        ),
                    )
                }
            }

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)

                // Page 1: openChat loads newest 50 messages (ids 16..65) with hasMoreHistory == true
                fix.vm.openChat("101")
                val page1 = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 50 }
                assertTrue("Page 1 must report hasMoreHistory == true", page1.chats.first { it.id == "101" }.hasMoreHistory)
                val page1Msgs = page1.messagesByChat["101"]!!
                assertEquals(3, page1Msgs.count { it.contentType == "photo" && it.text.isEmpty() && !it.mediaBlobId.isNullOrBlank() })
                assertEquals(2, page1Msgs.count { it.contentType == "voice" && it.text.isEmpty() && !it.mediaBlobId.isNullOrBlank() })

                // Page 2: loadOlderMessages paginates via ?before_id=16 -> 65 unique messages, zero duplicates, photo & voice preserved on both pages
                fix.vm.loadOlderMessages("101", limit = 50)
                val full = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 65 }
                assertTrue(
                    "MockGpuiTelegramServer must record GET /api/v1/chats/101/messages with before_id= query param",
                    server.recordedRequests.any {
                        it.method == "GET" && it.path == "/api/v1/chats/101/messages" && it.rawQuery.contains("before_id=")
                    },
                )
                val msgs = full.messagesByChat["101"]!!
                assertEquals("All 65 messages must be unique with zero duplicates", 65, msgs.map { it.id }.distinct().size)
                assertFalse("hasMoreHistory must be false after loading all 65 messages", full.chats.first { it.id == "101" }.hasMoreHistory)
                assertEquals(
                    "All 5 blank-text photo messages across both pages must be preserved",
                    photoIds.size,
                    msgs.count { it.contentType == "photo" && it.text.isEmpty() && !it.mediaBlobId.isNullOrBlank() },
                )
                assertEquals(
                    "All 4 blank-text voice messages across both pages must be preserved",
                    voiceIds.size,
                    msgs.count { it.contentType == "voice" && it.text.isEmpty() && !it.mediaBlobId.isNullOrBlank() },
                )

                // Server search locates hit id=7 cleanly; opening/selecting hit chat exposes hit 7 in messagesByChat["101"]
                fix.vm.closeChat()
                awaitState(fix.vm) { it.activeChatId == null }

                fix.vm.searchServerMessages("golden needle")
                val searched = awaitState(fix.vm) { it.serverSearchResults.any { m -> m.id == "7" } }
                val hit = searched.serverSearchResults.first { it.id == "7" }
                assertEquals("Unique golden needle keyword", hit.text)
                assertEquals("101", hit.conversationId)
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/search/messages" })

                val getsBeforeReopen = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats/101/messages" }
                fix.vm.openChat(hit.conversationId)
                awaitCondition(description = "re-opened chat 101 GET /messages") {
                    server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats/101/messages" } > getsBeforeReopen
                }
                awaitState(fix.vm) {
                    it.activeChatId == "101" &&
                        it.messagesByChat["101"]?.size == 50 &&
                        it.chats.first { c -> c.id == "101" }.hasMoreHistory
                }
                fix.vm.loadOlderMessages("101", limit = 50)
                val openedWithHit = awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "7" } == true }
                assertEquals("101", openedWithHit.activeChatId)
                val loadedHit = openedWithHit.messagesByChat["101"]!!.first { it.id == "7" }
                assertEquals("Unique golden needle keyword", loadedHit.text)
            }
        }
    }

    @Test
    fun e2e_t3_06_themeSwitchLanguageSwitchAndSettingsSignOutPreservesPreferences() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                assertEquals(TelegramThemePreset.APPLE_DAY, fix.vm.state.value.themeSettings.preset)
                assertEquals("zh-CN", fix.vm.state.value.uiLanguage)

                fix.vm.updateThemeSettings { it.copy(preset = TelegramThemePreset.APPLE_NIGHT) }
                fix.vm.updateLanguageAndTranslate("en-US", showTranslateButton = true, autoTranslate = false)
                awaitState(fix.vm) { it.themeSettings.preset == TelegramThemePreset.APPLE_NIGHT && it.uiLanguage == "en-US" }

                val nightPalette = resolveTelegramPalette(fix.vm.state.value.themeSettings)
                assertEquals(Color(0xFF1E1E1E), nightPalette.appBackground)
                assertTrue(contrastRatio(nightPalette.primaryText, nightPalette.appBackground) >= 4.5)
                assertEquals("Sign Out", resolveUiStrings(fix.vm.state.value.uiLanguage).signOut)

                // Sign Out from Settings -> AuthWizard stays in APPLE_NIGHT + en-US with cleared session
                fix.vm.signOutToAuthWizard()
                val signedOut = awaitState(fix.vm) { !it.isAuthenticated }
                assertEquals(TelegramThemePreset.APPLE_NIGHT, signedOut.themeSettings.preset)
                assertEquals("en-US", signedOut.uiLanguage)
                assertTrue(signedOut.chats.isEmpty())
            }
        }
    }

    @Test
    fun e2e_t3_07_inFlightReactionProtectedDuringConcurrentHistoryReload() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "React Chat", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "React to me", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "501" } == true }

                // Delay POST /reactions by 450ms so concurrent openChat history reload fetches stale empty reactions
                server.addFault(
                    FaultRule(
                        method = "POST",
                        pathPrefix = "/api/v1/chats/101/messages/501/reactions",
                        status = 0,
                        delayMs = 450L,
                        remainingHits = AtomicInteger(1),
                    ),
                )

                val msgGetsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats/101/messages" }
                fix.vm.toggleReaction("101", "501", "🔥")
                assertTrue(fix.vm.state.value.messagesByChat["101"]!!.first { it.id == "501" }.reactions.any { it.reaction == "🔥" && it.count == 1 })
                awaitCondition(description = "POST /reactions entered 450ms server delay") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/messages/501/reactions" }
                }

                // Trigger concurrent history reload while reaction is in-flight and prove GET /messages overlaps it
                fix.vm.openChat("101")
                var sawConcurrentReloadWhileInFlight = false
                awaitCondition(timeoutMs = 5_000L, pollMs = 15L, description = "concurrent GET /messages during in-flight reaction") {
                    val reacts = fix.vm.state.value.messagesByChat["101"]!!.first { it.id == "501" }.reactions
                    assertTrue("Optimistic reaction must not be clobbered while in-flight!", reacts.any { r -> r.reaction == "🔥" && r.count == 1 })
                    val getsNow = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats/101/messages" }
                    if (getsNow > msgGetsBefore && server.getMessageSnapshot(101L, 501L)?.reactions?.get("🔥") == null) {
                        sawConcurrentReloadWhileInFlight = true
                    }
                    getsNow > msgGetsBefore && getsNow >= 2
                }
                assertTrue("Concurrent GET /messages must overlap in-flight POST /reactions", sawConcurrentReloadWhileInFlight)

                // Wait for reaction HTTP to persist on server (never losing reaction during wait) and verify exact count == 1
                awaitCondition(timeoutMs = 5_000L, pollMs = 15L, description = "server reaction persisted") {
                    val reacts = fix.vm.state.value.messagesByChat["101"]!!.first { it.id == "501" }.reactions
                    assertTrue("Reaction lost before server settle!", reacts.any { r -> r.reaction == "🔥" && r.count == 1 })
                    server.getMessageSnapshot(101L, 501L)?.reactions?.get("🔥") == 1
                }
                val settled = awaitState(fix.vm) {
                    val fire = it.messagesByChat["101"]?.firstOrNull { m -> m.id == "501" }?.reactions?.firstOrNull { r -> r.reaction == "🔥" }
                    fire != null && fire.count == 1 && fire.chosenByMe
                }
                val finalReacts = settled.messagesByChat["101"]!!.first { it.id == "501" }.reactions
                val fire = finalReacts.firstOrNull { it.reaction == "🔥" }
                assertTrue("Settled reaction count must be 1, got $finalReacts", fire != null && fire.count == 1 && fire.chosenByMe)
            }
        }
    }

    @Test
    fun e2e_t3_08_messagePinDeleteMessageAndWsPinnedMessagesChanged() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "PinDelete Chat", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 1L, "Alice", "Ephemeral pin", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "501" } == true }

                fix.vm.clearToastBanner()
                fix.vm.togglePinMessage("101", "501")
                awaitState(fix.vm) {
                    it.chats.first { c -> c.id == "101" }.pinnedMessageIds == listOf("501") &&
                        !it.toastBanner.isNullOrBlank()
                }
                awaitCondition(description = "POST /pins completed") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/pins" }
                }

                fix.vm.deleteMessage("101", "501", forEveryone = true)
                awaitCondition(description = "DELETE /messages/501 completed") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/chats/101/messages/501" }
                }
                server.pushWsEvent("""{"type":"Update","payload":{"type":"DeleteMessages","data":{"chat_id":101,"message_ids":[501]}}}""")
                server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessagesChanged","data":{"chat_id":101,"pinned_ids":[]}}}""")

                val cleaned = awaitState(fix.vm) {
                    it.messagesByChat["101"]?.none { m -> m.id == "501" } == true &&
                        it.chats.first { c -> c.id == "101" }.pinnedMessageIds.isEmpty()
                }
                assertTrue(cleaned.chats.first { it.id == "101" }.pinnedMessageIds.isEmpty())
            }
        }
    }

    @Test
    fun e2e_t3_09_magpieKeySavePersistsAcrossColdRestartWithHonestOther47Presets() {
        val rootApp = TestFabushiApplication()
        try {
            E2eViewModelFixture(app = rootApp, deleteRootDirOnClose = false).use { fix ->
                // Before saving any key, all 48 presets are unconfigured and chatComplete fails closed
                assertTrue(fix.vm.state.value.magpieGateway.presets.all { !it.configured })
                MahayanaHost(rootApp, featureHostTest = false).use { host ->
                    val err = host.requestResult(
                        "magpie.chatComplete",
                        JSONObject().put("messages", JSONArray().put(JSONObject().put("role", "user").put("content", "test"))),
                    )
                    assertTrue(err.isFailure && err.exceptionOrNull()?.message.orEmpty().contains("PROVIDER_NOT_CONFIGURED"))
                }

                // Save real OpenAI key through ViewModel
                fix.vm.configureMagpieProvider("openai", "sk-e2e-pairwise-12345")
                val afterSave = awaitState(fix.vm) {
                    it.magpieGateway.presets.first { p -> p.id == "openai" }.configured
                }
                assertEquals(1, afterSave.magpieGateway.presets.count { it.configured })
                assertEquals(47, afterSave.magpieGateway.presets.count { !it.configured })
            }

            // Cold restart -> verify ONLY openai remains configured
            val restartedApp = rootApp.coldRestartApp()
            E2eViewModelFixture(app = restartedApp, deleteRootDirOnClose = false).use { fix2 ->
                val rehydrated = awaitState(fix2.vm) {
                    it.magpieGateway.presets.first { p -> p.id == "openai" }.configured
                }
                assertEquals(1, rehydrated.magpieGateway.presets.count { it.configured })
                assertEquals(47, rehydrated.magpieGateway.presets.count { !it.configured })
            }
        } finally {
            rootApp.cleanup()
        }
    }

    @Test
    fun e2e_t3_10_miniAppSecurityAndLiveWsMiniAppControlStatusRevocation() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Dharma Chat", lastMessageDateMs = 2000L))
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.serverWsConnected }

                // Live WS MiniAppControlStatus updates PiP state
                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"MiniAppControlStatus","data":{"bot_username":"dharma_bot","short_name":"global_dharma","chat_id":101,"bot_user_id":9001,"app_title":"全球法布施","mode":"ai_controlling","status_text":"AI 控制中","active_tool":"dharma.offer"}}}""",
                )
                awaitState(fix.vm) { it.miniAppPip.controlMode == "ai_controlling" && it.miniAppPip.activeTool == "dharma.offer" }

                // MiniAppLifecycleGate + Bridge enforce AND-GD-05 on ON_STOP & onDispose
                val gate = MiniAppLifecycleGate()
                gate.onLocalHtmlLoaded()
                val plugin = MarketplacePlugin(
                    pluginId = "global-dharma",
                    displayName = "Global Dharma",
                    description = "Test",
                    latestVersion = "2.3.0",
                    tools = listOf(MiniAppToolContract("state.read", "Read", "none")),
                )
                val bridge = MiniAppNativeWebMcpBridge(
                    plugin = plugin,
                    context = fix.app,
                    localDocumentActive = { gate.localDocumentActive },
                    callRuntimeToolJson = { _, _, _ -> """{"val":1}""" },
                    approvalPrompt = { true },
                )
                assertTrue(JSONObject(bridge.callTool("state.read", "{}")).getBoolean("ok"))

                gate.onLifecycleEvent(Lifecycle.Event.ON_STOP)
                assertFalse(gate.localDocumentActive)
                assertFalse(JSONObject(bridge.callTool("state.read", "{}")).getBoolean("ok"))

                gate.onDispose()
                assertTrue(gate.disposed)
                var lateDelivered = false
                assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "miniapp.control")) { lateDelivered = true })
                assertFalse(lateDelivered)
            }
        }
    }
}
