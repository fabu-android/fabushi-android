package com.ombhrum.fabushi.e2e

import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.FabushiRemoteDeviceGateway
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
import com.ombhrum.fabushi.telegram.TelegramThemePreset
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Tier 4 — Real-World Full End-to-End Application Journeys (4 Journeys: E2E-T4-01 .. E2E-T4-04).
 * Full stack: Pure-JDK MockGpuiTelegramServer (HTTP + WS) <-> Real Rust JNI Host (SQLite WAL + ureq)
 * <-> Real Kotlin TelegramViewModel (JNI + OkHttp WS + Mika + Gateway).
 */
class E2eTier4RealWorldJourneysTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    @Test
    fun e2e_t4_01_fullDailyMessagingPinningAndColdRestartPersistenceJourney() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Family Group", kind = "group", isPinned = false, lastMessageDateMs = 5_000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(102L, 1L, "Work Direct", kind = "direct", isPinned = false, lastMessageDateMs = 8_000L))
            // Seed 55 messages (including blank-text photo at id=51) so pagination + media preservation both run
            for (i in 1..55) {
                if (i == 51) {
                    server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "", contentType = "photo", dateMs = 1_000L * i,
                            fileId = "f_journey_photo", filename = "family.jpg", mimeType = "image/jpeg",
                        ),
                    )
                } else {
                    server.seedMessage(
                        MockGpuiTelegramServer.MockMessage(
                            id = i.toLong(), chatId = 101L, senderId = 2L, senderName = "Bob",
                            text = "History #$i", dateMs = 1_000L * i,
                        ),
                    )
                }
            }

            val rootApp = TestFabushiApplication()
            try {
                var createdMsgId = ""
                // Session 1: Cold start (zero fake data, APPLE_DAY) -> login -> paginate -> send -> read -> react -> edit -> pin -> draft -> folder
                E2eViewModelFixture(app = rootApp, deleteRootDirOnClose = false).use { fix ->
                    assertFalse(fix.vm.state.value.isAuthenticated)
                    assertTrue(fix.vm.state.value.chats.isEmpty())
                    assertEquals(TelegramThemePreset.APPLE_DAY, fix.vm.state.value.themeSettings.preset)

                    fix.loginAndConnect(server, "alice", "password123")
                    awaitState(fix.vm) { it.chats.size == 2 && it.serverWsConnected }

                    // Open chat 101 -> initial 50 messages -> paginate older 5 -> 55 total with photo preserved
                    fix.vm.openChat("101")
                    awaitState(fix.vm) { it.messagesByChat["101"]?.size == 50 }
                    fix.vm.loadOlderMessages("101", limit = 50)
                    val fullHist = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 55 }
                    assertTrue(fullHist.messagesByChat["101"]!!.any { it.id == "51" && it.contentType == "photo" && it.text.isEmpty() })

                    // Send message (sending -> sent) -> WS ReadHistory -> read
                    fix.vm.sendComposerMessage("101", "Dinner at 7pm?")
                    val sentState = awaitState(fix.vm) {
                        it.messagesByChat["101"]?.any { m -> m.text == "Dinner at 7pm?" && m.deliveryState == "sent" } == true
                    }
                    createdMsgId = sentState.messagesByChat["101"]!!.first { it.text == "Dinner at 7pm?" }.id
                    server.pushWsEvent(
                        """{"type":"Update","payload":{"type":"ReadHistory","data":{"chat_id":101,"max_id":$createdMsgId,"is_outbox":true,"unread_count":0}}}""",
                    )
                    awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == createdMsgId }?.deliveryState == "read" }

                    // React 🔥, Edit (PUT), Pin message, Pin chat, Save draft, Create folder
                    fix.vm.toggleReaction("101", createdMsgId, "🔥")
                    awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == createdMsgId }?.reactions?.any { r -> r.reaction == "🔥" } == true }

                    fix.vm.setEditTarget(fix.vm.state.value.messagesByChat["101"]!!.first { it.id == createdMsgId })
                    fix.vm.sendComposerMessage("101", "Dinner at 7:30pm!")
                    awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == createdMsgId }?.text == "Dinner at 7:30pm!" }

                    fix.vm.togglePinMessage("101", createdMsgId)
                    awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds.contains(createdMsgId) }

                    fix.vm.togglePinChat("101")
                    awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.isPinned }

                    fix.vm.saveServerDraft("102", "Follow up tomorrow")
                    awaitCondition(description = "draft PUT recorded") {
                        server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/chats/102/draft" }
                    }

                    fix.vm.upsertCustomFolder("VIP", includeContacts = true, includeBots = false, includeGroups = true, includeChannels = false, excludeMuted = false)
                    awaitState(fix.vm) { it.folderTabs.any { f -> f.title == "VIP" } }

                    // Deterministically await server persistence for all optimistic mutations before Session 1 store.clear()
                    val createdMsgIdLong = createdMsgId.toLong()
                    awaitCondition(timeoutMs = 5_000L, description = "Session 1 mutations persisted on MockGpuiTelegramServer") {
                        val hasReactionReq = server.recordedRequests.any {
                            it.method == "POST" && it.path == "/api/v1/chats/101/messages/$createdMsgId/reactions"
                        }
                        val hasPinsReq = server.recordedRequests.any {
                            it.method == "POST" && it.path == "/api/v1/chats/101/pins"
                        }
                        val chatSnap = server.getChatSnapshot(101L)
                        val msgSnap = server.getMessageSnapshot(101L, createdMsgIdLong)
                        hasReactionReq &&
                            hasPinsReq &&
                            chatSnap?.isPinned == true &&
                            createdMsgIdLong in chatSnap.pinnedMessageIds &&
                            msgSnap?.isEdited == true &&
                            msgSnap.text == "Dinner at 7:30pm!" &&
                            msgSnap.reactions["🔥"] == 1
                    }
                }

                // Session 2: Full cold restart on SAME SQLite WAL directory
                val restartedApp = rootApp.coldRestartApp()
                E2eViewModelFixture(
                    app = restartedApp,
                    preseedEndpoint = server.baseUrl,
                    expectAuthenticated = true,
                    deleteRootDirOnClose = false,
                ).use { fix2 ->
                    val rehydrated = awaitState(fix2.vm) {
                        it.chats.size == 2 && it.folderTabs.any { f -> f.title == "VIP" }
                    }
                    assertEquals("Pinned chat 101 must sort first after cold restart", "101", rehydrated.chats.first().id)
                    assertTrue(rehydrated.chats.first { it.id == "101" }.isPinned)
                    assertTrue(rehydrated.chats.first { it.id == "101" }.pinnedMessageIds.contains(createdMsgId))

                    fix2.vm.fetchServerDraft("102")
                    awaitState(fix2.vm) { it.chats.first { c -> c.id == "102" }.draftText == "Follow up tomorrow" }

                    fix2.vm.openChat("101")
                    val opened2 = awaitState(fix2.vm) {
                        it.messagesByChat["101"]?.any { m -> m.id == createdMsgId && m.text == "Dinner at 7:30pm!" } == true
                    }
                    val restoredMsg = opened2.messagesByChat["101"]!!.first { it.id == createdMsgId }
                    assertTrue(restoredMsg.edited)
                    assertTrue(restoredMsg.pinned)
                    assertTrue(restoredMsg.reactions.any { it.reaction == "🔥" && it.count == 1 })
                }
            } finally {
                rootApp.cleanup()
            }
        }
    }

    @Test
    fun e2e_t4_02_realAiBotLifecycleMusePatchComputerBindingAndLiveActivityJourney() {
        MockGpuiTelegramServer().use { server ->
            server.seedComputer(
                MockGpuiTelegramServer.MockComputer(
                    nodeId = "cloud:1:gpu-box",
                    ownerUserId = 1L,
                    displayName = "GPU Worker",
                    host = "10.1.1.8",
                    online = true,
                    runPolicy = "auto",
                ),
            )
            server.codingAgentProbes = JSONArray()
                .put(JSONObject().put("provider_kind", "codex").put("installed", true).put("version", "0.42.0"))
                .put(JSONObject().put("provider_kind", "claude_code").put("installed", false))
            server.callbackActionToast = "Build approved"
            server.callbackUpdatedText = "Build #42 succeeded!"

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.refreshServerBots()
                awaitCondition(description = "GET /api/v1/fabushi-bots") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/fabushi-bots" }
                }

                // Create Fabushi bot
                fix.vm.createFabushiBot("Release Muse", "release_muse_bot", "Automates releases", avatarShape = "hexagon", avatarColor = "#34C759")
                val created = awaitState(fix.vm) { it.fabushiBots.any { b -> b.username == "release_muse_bot" } }
                val bot = created.fabushiBots.first { it.username == "release_muse_bot" }
                val botIdStr = bot.botUserId.toString()
                val chatIdStr = bot.companionChatId.toString()
                assertTrue(created.lastCreatedBotToken.orEmpty().startsWith("srv_fbot_"))

                // Patch avatar shape/color & model via PATCH /api/v1/fabushi-bots/{id}
                fix.vm.patchFabushiBot(botIdStr, avatarShape = "star", avatarColor = "#AF52DE", preferredModel = "gpt-5.4")
                awaitState(fix.vm) {
                    val b = it.fabushiBots.firstOrNull { item -> item.botUserId == bot.botUserId }
                    b?.avatarShape == "star" && b.avatarColor == "#AF52DE" && b.preferredModel == "gpt-5.4"
                }

                // Probe coding agents & bind cloud computer
                fix.vm.probeCodingAgents()
                awaitState(fix.vm) { it.codingAgentProbes.size == 2 }
                fix.vm.setBotComputer(botIdStr, "cloud:1:gpu-box")
                awaitState(fix.vm) { it.botComputerBindings[botIdStr] == "cloud:1:gpu-box" }

                // Open companion chat (already populated by createFabushiBot), await GET /messages, send prompt, stream WS thinking -> running_tool -> NewMessage -> idle
                awaitState(fix.vm) { it.chats.any { c -> c.id == chatIdStr } }
                fix.vm.openChat(chatIdStr)
                awaitCondition(description = "openChat GET /messages completed") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats/$chatIdStr/messages" }
                }
                awaitState(fix.vm) { it.activeChatId == chatIdStr && it.messagesByChat.containsKey(chatIdStr) }
                fix.vm.sendComposerMessage(chatIdStr, "Build release v2.3.0")
                awaitState(fix.vm) { it.messagesByChat[chatIdStr]?.any { m -> m.text == "Build release v2.3.0" && m.deliveryState == "sent" } == true }

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":${bot.companionChatId},"bot_user_id":${bot.botUserId},"state":"thinking","detail":"Planning build"}}}""",
                )
                awaitState(fix.vm) { it.chats.first { c -> c.id == chatIdStr }.botExecutionState == "thinking" }

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":${bot.companionChatId},"bot_user_id":${bot.botUserId},"state":"running_tool","detail":"gradle assembleRelease"}}}""",
                )
                awaitState(fix.vm) { it.chats.first { c -> c.id == chatIdStr }.botExecutionState == "running_tool" }

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"NewMessage","data":{"id":8801,"chat_id":${bot.companionChatId},"sender_id":${bot.botUserId},"sender_name":"Release Muse","text":"Ready to publish?","date_ms":9900,"inline_keyboard":[[{"text":"Approve Build","callback_data":"build:approve"}]]}}}""",
                )
                val withKb = awaitState(fix.vm) { it.messagesByChat[chatIdStr]?.any { m -> m.id == "8801" && m.inlineKeyboard.isNotEmpty() } == true }

                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":${bot.companionChatId},"bot_user_id":${bot.botUserId},"state":"idle"}}}""",
                )
                awaitState(fix.vm) { it.chats.first { c -> c.id == chatIdStr }.botExecutionState == "idle" }

                // Press inline button -> POST /api/v1/chats/{id}/callback -> PeerBotActionResult applied
                val btn = withKb.messagesByChat[chatIdStr]!!.first { it.id == "8801" }.inlineKeyboard.first().buttons.first()
                fix.vm.clickInlineButton(chatIdStr, btn, messageId = "8801")
                awaitState(fix.vm) { it.messagesByChat[chatIdStr]?.firstOrNull { m -> m.id == "8801" }?.text == "Build #42 succeeded!" }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/${bot.companionChatId}/callback" })
            }
        }
    }

    @Test
    fun e2e_t4_03_magpieCloudComputerAssistMcpGrantsAndMikaConfirmationJourney() = runBlocking {
        MockGpuiTelegramServer().use { server ->
            server.seedMcpGrant(MockGpuiTelegramServer.MockMcpGrant("fam-j3", "Cursor IDE", listOf("remote.exec"), listOf("cloud:1:main")))

            val rootApp = TestFabushiApplication()
            val channel = TelegramMikaChannel()
            val owners = MemoryOwnerIdentity()
            val activeAsk = AtomicReference<Delivery.Ask?>(null)
            val stepSink = AtomicReference<((String) -> Unit)?>(null)
            val decisions = CopyOnWriteArrayList<ConfirmChoice>()
            val gateway = ChannelGateway(
                owners = owners,
                activeAsk = { tid -> activeAsk.get()?.takeIf { it.threadId == tid } },
                route = object : ChannelGateway.Route {
                    override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                    override suspend fun answer(threadId: String, askId: String, text: String): Boolean = true
                    override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                        decisions.add(choice)
                        activeAsk.set(null)
                        stepSink.get()?.invoke("任务完成：操作已执行")
                        return true
                    }
                    override suspend fun cancelAsk(threadId: String) { activeAsk.set(null) }
                    override suspend fun stop(runId: RunId) = Unit
                },
            )
            val reg = gateway.start(channel)
            try {
                // Phase 1: Verify 48 presets unconfigured -> save OpenAI key -> cold restart -> verify persisted
                E2eViewModelFixture(app = rootApp, deleteRootDirOnClose = false).use { fix ->
                    assertTrue(fix.vm.state.value.magpieGateway.presets.all { !it.configured })
                    fix.vm.configureMagpieProvider("openai", "sk-journey-3-real")
                    awaitState(fix.vm) { it.magpieGateway.presets.first { p -> p.id == "openai" }.configured }
                }

                // Phase 2: Cold restart -> login -> Cloud Computer CRUD/exec -> Assist -> Sessions -> MCP Grants -> Mika Confirm
                val restartedApp = rootApp.coldRestartApp()
                E2eViewModelFixture(app = restartedApp, preseedEndpoint = server.baseUrl, deleteRootDirOnClose = false).use { fix2 ->
                    awaitState(fix2.vm) { it.magpieGateway.presets.first { p -> p.id == "openai" }.configured }
                    fix2.loginAndConnect(server)

                    // Cloud computer upsert -> test -> patch policy -> exec
                    fix2.vm.upsertCloudComputer("Workbench Box", "10.9.0.5", secret = "ssh-pass-secret")
                    val withNode = awaitState(fix2.vm) { it.cloudComputer.nodes.any { n -> n.name == "Workbench Box" } }
                    val nodeId = withNode.cloudComputer.nodes.first { it.name == "Workbench Box" }.id
                    fix2.vm.testComputer(nodeId)
                    awaitState(fix2.vm) { it.cloudComputer.nodes.first { n -> n.id == nodeId }.online }
                    fix2.vm.patchComputerPolicy(nodeId, runPolicy = "auto")
                    awaitState(fix2.vm) { it.cloudComputer.nodes.first { n -> n.id == nodeId }.runPolicy == "auto" }
                    fix2.vm.execComputer(nodeId, JSONObject().put("op", "shell_exec").put("command", "whoami"))
                    awaitState(fix2.vm) { it.cloudComputer.lastOpOutput.contains("server-exec-stdout: whoami") }

                    // UU-Remote Assist: allow=true, 10-char code, start -> disconnect
                    fix2.vm.toggleRemoteAssist(true)
                    awaitState(fix2.vm) { it.cloudComputer.assistConfig.allowAssist }
                    fix2.vm.updateRemoteAssistCode("Secure2026")
                    awaitState(fix2.vm) { it.cloudComputer.assistConfig.currentCode == "Secure2026" }
                    fix2.vm.startRemoteAssist("987654321", "Secure2026")
                    val withSess = awaitState(fix2.vm) { it.cloudComputer.remoteSessions.isNotEmpty() }
                    val sid = withSess.cloudComputer.remoteSessions.first().sessionId
                    fix2.vm.disconnectRemoteSession(sid)
                    awaitState(fix2.vm) { it.cloudComputer.remoteSessions.none { s -> s.sessionId == sid } }

                    // MCP Grants list & revoke (await hydrated grant -> revoke -> await DELETE -> refresh to confirm server removal)
                    awaitState(fix2.vm) { it.cloudComputer.mcpGrants.any { g -> g.familyId == "fam-j3" } }
                    fix2.vm.revokeRemoteMcpGrant("fam-j3")
                    awaitCondition(description = "DELETE /api/v1/mcp/grants/fam-j3 completed") {
                        server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/mcp/grants/fam-j3" }
                    }
                    val grantReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/mcp/grants" }
                    fix2.vm.refreshMcpGrants()
                    awaitCondition(description = "post-revoke GET /api/v1/mcp/grants completed") {
                        server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/mcp/grants" } > grantReqsBefore
                    }
                    awaitState(fix2.vm) { it.cloudComputer.mcpGrants.isEmpty() }

                    // Mika ConfirmationGate inline Approve
                    fix2.vm.openSavedMessages()
                    val chatId = awaitState(fix2.vm) { it.activeChatId != null }.activeChatId!!
                    val peer = ChannelPeer(TelegramMikaChannel.ID, "user:1", chatId)
                    owners.bind(peer)
                    fix2.vm.attachMikaBridge(
                        object : MikaBridgeCallbacks {
                            override fun currentSnapshot() = MikaBridgeUiSnapshot(
                                available = true,
                                sandboxReady = true,
                                pendingAskId = activeAsk.get()?.askId,
                                pendingAskThreadId = activeAsk.get()?.threadId,
                                pendingConfirmationPrompt = activeAsk.get()?.text,
                            )

                            override fun submitPhoneTask(prompt: String, sourceConversationId: String?, onStepUpdate: (String) -> Unit) {
                                val tid = sourceConversationId ?: chatId
                                stepSink.set(onStepUpdate)
                                val ask = Delivery.Ask("d-j3", tid, "ask-j3", Delivery.Ask.Kind.Confirm, "Confirm $prompt")
                                activeAsk.set(ask)
                                runBlocking { channel.send(peer, ask) }
                                onStepUpdate("待确认：$prompt")
                            }

                            override fun stopActiveTask() = Unit
                            override fun resolveConfirmation(approved: Boolean, threadId: String?, askId: String?) {
                                runBlocking { channel.resolveConfirmationForThread(threadId ?: chatId, approved, askId) }
                            }
                            override fun answerAsk(text: String, threadId: String?, askId: String?): Boolean = true
                        },
                    )
                    fix2.vm.runWithMikaFromChat(chatId, "Open Bluetooth settings")
                    awaitState(fix2.vm) { it.mikaSnapshot.pendingAskId == "ask-j3" }
                    fix2.vm.resolveMikaConfirmation(approved = true, threadId = chatId, askId = "ask-j3")
                    awaitState(fix2.vm) { it.messagesByChat[chatId]?.any { m -> m.mikaTaskStatus == "completed" } == true }
                    assertEquals(listOf(ConfirmChoice.Once), decisions.toList())
                }
            } finally {
                reg.close()
                channel.close()
                rootApp.cleanup()
            }
        }
    }

    @Test
    fun e2e_t4_04_multiAccountIsolation401ExpiryArgon2idPasscodeAndCleanSignOutJourney() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Alice Confidential", lastMessageDateMs = 5_000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(202L, 2L, "Bob Workspace", lastMessageDateMs = 5_000L))
            server.seedSession(
                MockGpuiTelegramServer.MockSession(
                    sessionId = "sess-alice-desktop",
                    userId = 1L,
                    token = "tok_alice_desktop_secondary_0123456789",
                    deviceName = "Linux Desktop",
                    isCurrent = false,
                ),
            )

            E2eViewModelFixture(wsBaseBackoffMs = 150L).use { fix ->
                val surface = FabushiAppAgentSurface()
                val gateway = FabushiRemoteDeviceGateway(
                    context = fix.app,
                    surface = surface,
                    metadata = mapOf("suite" to "e2e_t4_04"),
                    gatewayUrl = "${server.wsBaseUrl}/agent",
                    retryDelayMs = 100L,
                )
                try {
                    // 1. User A (Alice) signs in, sets Argon2id passcode, switches to APPLE_NIGHT + en-US, updates profile & revokes secondary session
                    fix.loginAndConnect(server, "alice", "password123")
                    awaitState(fix.vm) { it.chats.any { c -> c.id == "101" } && it.serverWsConnected }

                    fix.vm.configurePasscode("7391")
                    awaitState(fix.vm) { it.passcodeConfigured }
                    assertTrue(fix.vm.verifyPasscode("7391"))
                    assertFalse(fix.vm.verifyPasscode("0000"))
                    assertTrue(fix.app.telegramPrefs().all.values.none { it.toString().contains("7391") })

                    fix.vm.updateThemeSettings { it.copy(preset = TelegramThemePreset.APPLE_NIGHT) }
                    fix.vm.updateLanguageAndTranslate("en-US", showTranslateButton = true, autoTranslate = false)
                    fix.vm.updateProfile("Alice Prime", "alice", "Updated bio")
                    val sessReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/auth/sessions" }
                    fix.vm.refreshSessions()
                    awaitCondition(description = "explicit GET /api/v1/auth/sessions") {
                        server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/auth/sessions" } > sessReqsBefore
                    }
                    awaitState(fix.vm) { it.serverSessions.any { s -> s.sessionId == "sess-alice-desktop" } }
                    fix.vm.revokeSession("sess-alice-desktop")
                    awaitState(fix.vm) { it.serverSessions.none { s -> s.sessionId == "sess-alice-desktop" } }

                    // 2. Server revokes Alice's token & pushes WS Error(401) -> automatic wipe & return to AuthWizard
                    server.revokeAllTokensForUser(1L)
                    server.pushWsEvent("""{"type":"Error","payload":{"code":401,"message":"Session revoked"}}""")
                    val wiped = awaitState(fix.vm) { !it.isAuthenticated && !it.serverWsConnected && it.chats.isEmpty() }
                    assertEquals(TelegramThemePreset.APPLE_NIGHT, wiped.themeSettings.preset)
                    assertEquals("en-US", wiped.uiLanguage)

                    // 3. User B (Bob) signs in with slow Alice residue impossible -> SessionRequestFence isolates Bob
                    server.addFault(FaultRule(method = "GET", pathPrefix = "/api/v1/chats", status = 0, delayMs = 80L, remainingHits = AtomicInteger(1)))
                    fix.loginAndConnect(server, "bob", "password123")
                    val bobAuthed = awaitState(fix.vm) { it.chats.any { c -> c.id == "202" } && it.serverWsConnected }
                    assertTrue("Zero User A leakage into User B", bobAuthed.chats.none { it.id == "101" })

                    // Activate remote gateway for User B
                    gateway.setLoggedIn(true)
                    awaitCondition(timeoutMs = 5_000L, description = "User B gateway registered") { gateway.isRegistered() }

                    // 4. User B signs out cleanly -> WS closed, gateway stopped, SQLite session unauthenticated
                    fix.vm.signOutToAuthWizard()
                    awaitState(fix.vm) { !it.isAuthenticated && !it.serverWsConnected && it.chats.isEmpty() }
                    gateway.setLoggedIn(false)
                    assertFalse(gateway.isRegistered())

                    MahayanaHost(fix.app, featureHostTest = false).use { host ->
                        assertFalse(host.request("telegram.server.session").getBoolean("authenticated"))
                    }
                } finally {
                    gateway.close()
                }
            }
        }
    }
}
