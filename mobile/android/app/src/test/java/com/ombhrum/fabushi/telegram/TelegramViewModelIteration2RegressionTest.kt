package com.ombhrum.fabushi.telegram

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ombhrum.fabushi.LoopbackTelegramTestServer
import com.ombhrum.fabushi.M3MainDispatcherRule
import com.ombhrum.fabushi.M3TestApplication
import com.ombhrum.fabushi.MockHttpResponse
import com.ombhrum.fabushi.awaitCondition
import com.ombhrum.fabushi.awaitState
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
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
 * Iteration 2 regression tests for TelegramViewModel & TelegramMikaChannel against real
 * libmahayana_app_host.so and LoopbackTelegramTestServer.
 */
class TelegramViewModelIteration2RegressionTest {
    @get:Rule
    val mainDispatcherRule = M3MainDispatcherRule()

    @Test
    fun magpieStrategyPersistsAcrossColdRestartAndImportEnvRoutesCorrectly() {
        val app = M3TestApplication()
        val store1 = ViewModelStore()
        val store2 = ViewModelStore()
        try {
            val vm1 = ViewModelProvider(store1, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm1) { it.syncStatusLabel != "同步中…" }

            // 1. setMagpieStrategy dispatches magpie.setStrategy (not nonexistent magpie.setRoutingStrategy)
            vm1.setMagpieStrategy(MagpieRoutingStrategy.CHEAPEST)
            val afterSet = awaitState(vm1) { it.magpieGateway.strategy == MagpieRoutingStrategy.CHEAPEST }
            assertEquals(MagpieRoutingStrategy.CHEAPEST, afterSet.magpieGateway.strategy)

            // 2. importMagpieEnvOrUri routes dotenv containing URLs ("://") to magpie.importEnv
            vm1.importMagpieEnvOrUri("ANTHROPIC_BASE_URL=https://api.anthropic.com/v1\nANTHROPIC_API_KEY=sk-ant-regression-123456")
            val afterImport = awaitState(vm1) {
                it.magpieGateway.presets.firstOrNull { p -> p.id == "anthropic" }?.configured == true
            }
            assertTrue(afterImport.magpieGateway.liveReady)

            // Single-line dotenv with URL must NOT be misrouted to magpie.importUri (which fails with unsupported scheme)
            vm1.clearToastBanner()
            vm1.importMagpieEnvOrUri("DEEPSEEK_API_KEY=sk-ds-test-999999")
            awaitState(vm1) {
                it.magpieGateway.presets.firstOrNull { p -> p.id == "deepseek" }?.configured == true
            }

            store1.clear()

            // 3. Cold restart hydrates PascalCase "Cheapest" back to MagpieRoutingStrategy.CHEAPEST
            val vm2 = ViewModelProvider(store2, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            val hydrated = awaitState(vm2) {
                it.syncStatusLabel != "同步中…" &&
                    it.magpieGateway.strategy == MagpieRoutingStrategy.CHEAPEST &&
                    it.magpieGateway.presets.firstOrNull { p -> p.id == "anthropic" }?.configured == true
            }
            assertEquals(MagpieRoutingStrategy.CHEAPEST, hydrated.magpieGateway.strategy)
        } finally {
            store1.clear()
            store2.clear()
            app.cleanup()
        }
    }

    @Test
    fun addAccountThenSwitchAccountAndColdRestartSwitchSucceedWithoutGenerationFenceMismatch() {
        val app = M3TestApplication()
        val store1 = ViewModelStore()
        val store2 = ViewModelStore()
        try {
            val vm1 = ViewModelProvider(store1, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm1) { it.syncStatusLabel != "同步中…" }

            // Add two accounts (each advances UI generationFence, while Rust tg_fence stays in sync via rustAccountGeneration)
            vm1.addAccount("Alice Prime", "", "@alice_prime")
            val afterFirst = awaitState(vm1) { it.accounts.any { a -> a.username == "@alice_prime" } }
            val firstId = afterFirst.accounts.first { it.username == "@alice_prime" }.id

            vm1.addAccount("Bob Second", "", "@bob_second")
            val afterSecond = awaitState(vm1) { it.accounts.any { a -> a.username == "@bob_second" } }
            val secondId = afterSecond.accounts.first { it.username == "@bob_second" }.id

            // Switch back to firstId -> MUST succeed without "GenerationFence mismatch"
            vm1.clearToastBanner()
            vm1.switchAccount(firstId)
            val afterSwitch1 = awaitState(vm1) { it.activeAccountId == firstId && it.toastBanner?.contains("已切换至账号") == true }
            assertEquals(firstId, afterSwitch1.activeAccountId)

            store1.clear()

            // Cold restart hydrates rustAccountGeneration from telegram.auth.accountsList and switches cleanly again
            val vm2 = ViewModelProvider(store2, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm2) { it.syncStatusLabel != "同步中…" && it.accounts.size >= 2 }
            vm2.clearToastBanner()
            vm2.switchAccount(secondId)
            val afterSwitch2 = awaitState(vm2) { it.activeAccountId == secondId && it.toastBanner?.contains("已切换至账号") == true }
            assertEquals(secondId, afterSwitch2.activeAccountId)
        } finally {
            store1.clear()
            store2.clear()
            app.cleanup()
        }
    }

    @Test
    fun hermesStudioHydrationMutationsApprovalsAndMagpieReplyRoundTripAgainstRealRustHost() {
        val app = M3TestApplication()
        val store1 = ViewModelStore()
        val store2 = ViewModelStore()
        val llmMode = AtomicReference("tool_call")

        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when {
                    req.path.endsWith("/chat/completions") -> {
                        if (llmMode.get() == "tool_call") {
                            MockHttpResponse(
                                body = JSONObject()
                                    .put("id", "chatcmpl-hermes-1")
                                    .put("model", "gpt-4o-mini")
                                    .put(
                                        "choices",
                                        JSONArray().put(
                                            JSONObject()
                                                .put("index", 0)
                                                .put(
                                                    "message",
                                                    JSONObject()
                                                        .put("role", "assistant")
                                                        .put("content", "")
                                                        .put(
                                                            "tool_calls",
                                                            JSONArray().put(
                                                                JSONObject()
                                                                    .put("id", "call_term_1")
                                                                    .put("type", "function")
                                                                    .put(
                                                                        "function",
                                                                        JSONObject()
                                                                            .put("name", "terminal_exec")
                                                                            .put("arguments", """{"command":"echo test"}"""),
                                                                    ),
                                                            ),
                                                        ),
                                                )
                                                .put("finish_reason", "tool_calls"),
                                        ),
                                    )
                                    .put("usage", JSONObject().put("prompt_tokens", 12).put("completion_tokens", 8))
                                    .toString(),
                            )
                        } else {
                            MockHttpResponse(
                                body = JSONObject()
                                    .put("id", "chatcmpl-reply-2")
                                    .put("model", "gpt-4o-mini")
                                    .put(
                                        "choices",
                                        JSONArray().put(
                                            JSONObject()
                                                .put("index", 0)
                                                .put(
                                                    "message",
                                                    JSONObject()
                                                        .put("role", "assistant")
                                                        .put("content", "Hello from Magpie reply"),
                                                )
                                                .put("finish_reason", "stop"),
                                        ),
                                    )
                                    .put("usage", JSONObject().put("prompt_tokens", 10).put("completion_tokens", 5))
                                    .toString(),
                            )
                        }
                    }
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )

        try {
            // Seed real Rust Hermes entities + configure Magpie openai provider pointing at loopback mock LLM
            val createdSessionId: String
            val createdCardId: String
            val createdApprovalId: String
            MahayanaHost(app, featureHostTest = false).use { host ->
                host.request(
                    "magpie.upsertProvider",
                    JSONObject()
                        .put("presetId", "openai")
                        .put("apiKey", "sk-hermes-loopback-key")
                        .put("baseUrl", "${server.baseUrl}/v1")
                        .put("defaultModel", "gpt-4o-mini")
                        .put("enabled", true),
                )
                val sess = host.request(
                    "hermes.createSession",
                    JSONObject().put("title", "Alpha Root").put("providerId", "openai").put("model", "gpt-4o-mini"),
                )
                createdSessionId = sess.getString("sessionId")

                host.request(
                    "hermes.skillUpsert",
                    JSONObject()
                        .put("name", "rust-audit")
                        .put("description", "Audit Rust JNI contracts")
                        .put("bodyMarkdown", "# rust-audit\nCheck every contract"),
                )
                val card = host.request(
                    "hermes.kanbanAddCard",
                    JSONObject().put("title", "Verify M3 contracts").put("columnId", "todo"),
                )
                createdCardId = card.getString("cardId")

                // Trigger a real pending approval row in hermes_approvals via hermes.sendTurn
                val chatRes = host.request(
                    "hermes.sendTurn",
                    JSONObject().put("sessionId", createdSessionId).put("text", "Run echo test in terminal"),
                )
                assertEquals("waiting_approval", chatRes.getString("status"))
                createdApprovalId = chatRes.getJSONObject("pendingApproval").getString("approvalId")
                assertTrue(createdApprovalId.startsWith("apr_"))
            }

            // Hydrate in TelegramViewModel and verify ALL 5 Hermes collections parse Rust's canonical keys
            val vm1 = ViewModelProvider(store1, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            val hydrated = awaitState(vm1) {
                it.syncStatusLabel != "同步中…" &&
                    it.hermesStudio.sessions.any { s -> s.id == createdSessionId } &&
                    it.hermesStudio.skills.any { s -> s.id == "rust-audit" } &&
                    it.hermesStudio.kanbanCards.any { c -> c.id == createdCardId } &&
                    it.hermesStudio.approvals.any { a -> a.id == createdApprovalId }
            }

            val rootSession = hydrated.hermesStudio.sessions.first { it.id == createdSessionId }
            assertEquals("Alpha Root", rootSession.title)
            assertEquals("openai", rootSession.provider)

            val skill = hydrated.hermesStudio.skills.first { it.id == "rust-audit" }
            assertEquals("rust-audit", skill.name)
            assertTrue(skill.enabled)

            val card = hydrated.hermesStudio.kanbanCards.first { it.id == createdCardId }
            assertEquals(KanbanColumn.BACKLOG, card.column)
            assertEquals("hermes", card.assigneeAgent)

            val approval = hydrated.hermesStudio.approvals.first { it.id == createdApprovalId }
            assertEquals("terminal_exec", approval.toolName)
            assertTrue(approval.summary.contains("terminal_exec"))
            assertEquals(ActionRiskTier.R3_HIGH, approval.riskTier)
            assertEquals("pending", approval.status)

            // Exercise branchHermesSession (uses "sessionId"), addHermesCronJob (uses "hermes.cronUpsert" + "schedule"),
            // advanceKanbanCard (uses "columnId": "in_progress"), and decideHermesApproval (Contract #15)
            vm1.branchHermesSession(createdSessionId)
            val afterBranch = awaitState(vm1) {
                it.hermesStudio.sessions.any { s -> s.parentBranchId == createdSessionId }
            }
            assertTrue(afterBranch.hermesStudio.sessions.any { s -> s.parentBranchId == createdSessionId })

            vm1.addHermesCronJob("Daily Check", "0 9 * * *", "Run daily health check")
            val afterCron = awaitState(vm1) {
                it.hermesStudio.cronJobs.any { j -> j.scheduleExpr == "0 9 * * *" && j.prompt == "Run daily health check" }
            }
            val cronId = afterCron.hermesStudio.cronJobs.first { it.scheduleExpr == "0 9 * * *" }.id
            assertTrue(cronId.isNotBlank())

            vm1.advanceKanbanCard(createdCardId)
            awaitState(vm1) {
                it.hermesStudio.kanbanCards.firstOrNull { c -> c.id == createdCardId }?.column == KanbanColumn.IN_PROGRESS
            }

            vm1.decideHermesApproval(createdApprovalId, approved = false)
            awaitState(vm1) {
                it.hermesStudio.approvals.firstOrNull { a -> a.id == createdApprovalId }?.status == "denied"
            }

            // Verify Magpie "reply" key in submitTalkModeUtterance
            llmMode.set("reply")
            vm1.submitTalkModeUtterance("Say hello")
            val afterTalk = awaitState(vm1) { it.talkOrbState.assistantReply == "Hello from Magpie reply" }
            assertEquals("Hello from Magpie reply", afterTalk.talkOrbState.assistantReply)

            store1.clear()

            // Cold restart verifies cron, branched session, moved kanban card, and denied approval persisted in SQLite
            val vm2 = ViewModelProvider(store2, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            val reloaded = awaitState(vm2) {
                it.syncStatusLabel != "同步中…" &&
                    it.hermesStudio.cronJobs.any { j -> j.id == cronId } &&
                    it.hermesStudio.kanbanCards.any { c -> c.id == createdCardId && c.column == KanbanColumn.IN_PROGRESS } &&
                    it.hermesStudio.approvals.any { a -> a.id == createdApprovalId && a.status == "denied" }
            }
            assertEquals(KanbanColumn.IN_PROGRESS, reloaded.hermesStudio.kanbanCards.first { it.id == createdCardId }.column)
            assertEquals("denied", reloaded.hermesStudio.approvals.first { it.id == createdApprovalId }.status)
            assertTrue(reloaded.hermesStudio.sessions.any { it.parentBranchId == createdSessionId })
        } finally {
            store1.clear()
            store2.clear()
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun sendTonTransferRejects64BitOverflowCleanlyWithoutWraparound() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.clearToastBanner()

            // 2e10 TON * 1e9 nanotons = 2e19 > Long.MAX_VALUE (~9.22e18); must fail closed instead of wrapping modulo 2^64
            vm.sendTonTransfer("EQD0vdSA_NedR9uvbgN9EikRX-suesDxGeFg69XQMavfLqIw", 2e10, "overflow")
            val state = awaitState(vm) { it.toastBanner?.contains("超出允许范围") == true }
            assertTrue(state.toastBanner!!.contains("超出允许范围"))
            assertTrue(state.tonWallet.transactions.isEmpty())
        } finally {
            store.clear()
            app.cleanup()
        }
    }

    @Test
    fun antiFlickerUnpinMessageServerUnpinAllUnmuteClearsMuteUntilAndRapidDoubleTogglePin() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val unpinDelayGate = CountDownLatch(1)
        val unpinStarted = CountDownLatch(1)
        val chat101PinnedMsg = AtomicReference<Long?>(501L)
        val chat101Muted = AtomicBoolean(true)
        val chat102PinCallNum = AtomicInteger(0)
        val firstPinStarted = CountDownLatch(1)
        val allowFirstPinComplete = CountDownLatch(1)
        val secondPinCompleted = CountDownLatch(1)

        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when {
                    req.path == "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1","token":"tok_antiflicker_1234567890"}""",
                    )
                    req.path == "/api/v1/auth/me" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1"}""",
                    )
                    req.path == "/api/v1/chats" -> {
                        val c101 = JSONObject()
                            .put("id", 101)
                            .put("title", "Pin & Mute Test")
                            .put("kind", "direct")
                            .put("is_pinned", false)
                            .put("is_muted", chat101Muted.get())
                            .put("mute_until_ms", if (chat101Muted.get()) 1999999999000L else JSONObject.NULL)
                            .put("pinned_message_id", chat101PinnedMsg.get() ?: JSONObject.NULL)
                            .put("last_message_date_ms", 5000L)
                        val c102 = JSONObject()
                            .put("id", 102)
                            .put("title", "Rapid Double Toggle")
                            .put("kind", "direct")
                            .put("is_pinned", false)
                            .put("last_message_date_ms", 4000L)
                        MockHttpResponse(body = JSONObject().put("chats", JSONArray().put(c101).put(c102)).toString())
                    }
                    req.path == "/api/v1/chats/101/mute" -> {
                        val muted = req.bodyJsonOrNull()?.optBoolean("muted", false) ?: false
                        chat101Muted.set(muted)
                        MockHttpResponse(body = """{"chat_id":101,"is_muted":$muted,"mute_until_ms":null}""")
                    }
                    req.path == "/api/v1/chats/101/messages" -> {
                        // Deliberately return STALE pinned: true while unpin is in-flight!
                        MockHttpResponse(
                            body = """{"chat_id":101,"has_more":false,"messages":[
                                {"id":501,"chat_id":101,"sender_id":2,"sender_name":"Bob","text":"Important","pinned":true,"date_ms":2000}
                            ]}""",
                        )
                    }
                    req.path == "/api/v1/chats/101/pins" && req.method == "GET" -> {
                        val list = chat101PinnedMsg.get()?.let { listOf(it) } ?: emptyList()
                        MockHttpResponse(body = JSONObject().put("chat_id", 101).put("pinned_message_ids", JSONArray(list)).toString())
                    }
                    req.path == "/api/v1/chats/101/pins/501" && req.method == "DELETE" -> {
                        unpinStarted.countDown()
                        unpinDelayGate.await(5, TimeUnit.SECONDS)
                        chat101PinnedMsg.set(null)
                        MockHttpResponse(body = """{"chat_id":101,"pinned_message_ids":[]}""")
                    }
                    req.path == "/api/v1/chats/102/pin" && req.method == "POST" -> {
                        val callNo = chat102PinCallNum.incrementAndGet()
                        val pinned = req.bodyJsonOrNull()?.optBoolean("pinned", false) ?: false
                        if (callNo == 1) {
                            firstPinStarted.countDown()
                            allowFirstPinComplete.await(5, TimeUnit.SECONDS)
                            MockHttpResponse(body = """{"chat_id":102,"is_pinned":$pinned}""")
                        } else {
                            secondPinCompleted.countDown()
                            MockHttpResponse(body = """{"chat_id":102,"is_pinned":$pinned}""")
                        }
                    }
                    req.path == "/api/v1/chats/101/draft" -> MockHttpResponse(body = """{"chat_id":101,"draft":null}""")
                    req.path == "/api/v1/folders" -> MockHttpResponse(body = """{"folders":[]}""")
                    req.path == "/api/v1/contacts" -> MockHttpResponse(body = """{"contacts":[]}""")
                    req.path == "/api/v1/auth/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    req.path == "/api/v1/bots/my" || req.path == "/api/v1/fabushi-bots" -> MockHttpResponse(body = """{"bots":[]}""")
                    req.path == "/api/v1/computers" -> MockHttpResponse(body = """{"computers":[]}""")
                    req.path == "/api/v1/remote/assist/info" -> MockHttpResponse(body = """{"assist_id":"111222333","allow_assist":false}""")
                    req.path == "/api/v1/remote/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    req.path == "/api/v1/mcp/grants" -> MockHttpResponse(body = """{"grants":[]}""")
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )

        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }
            vm.loginWithPassword("alice", "pw")
            val authed = awaitState(vm) { it.isAuthenticated && it.chats.size == 2 }
            assertEquals(1999999999000L, authed.chats.first { it.id == "101" }.muteUntilMs)

            // 1. Unmute clears muteUntilMs immediately and authoritatively
            vm.toggleMuteChat("101")
            val afterUnmute = awaitState(vm) {
                val c = it.chats.first { ch -> ch.id == "101" }
                !c.isMuted && c.muteUntilMs == null
            }
            assertNull(afterUnmute.chats.first { it.id == "101" }.muteUntilMs)

            // 2. In-flight message unpin stays unpinned during overlapping getMessages returning stale pinned=true
            vm.openChat("101")
            awaitState(vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == "501" }?.pinned == true }

            vm.togglePinMessage("101", "501")
            assertTrue("Expected DELETE /pins/501 to start", unpinStarted.await(5, TimeUnit.SECONDS))
            assertFalse(vm.state.value.messagesByChat["101"]!!.first { it.id == "501" }.pinned)

            // Trigger overlapping GET /messages returning stale pinned=true while DELETE is still in flight
            val msgGetsBefore = server.recordedRequests.count { it.path == "/api/v1/chats/101/messages" }
            vm.openChat("101")
            awaitCondition { server.recordedRequests.count { it.path == "/api/v1/chats/101/messages" } > msgGetsBefore }
            Thread.sleep(100L)
            assertFalse(
                "In-flight unpin MUST NOT flicker back to pinned=true from stale getMessages",
                vm.state.value.messagesByChat["101"]!!.first { it.id == "501" }.pinned,
            )

            // Let DELETE finish
            unpinDelayGate.countDown()
            awaitState(vm) { it.toastBanner?.contains("已取消置顶") == true }

            // 3. Server unpin-all clears pinnedMessageIds on refreshAllFromHost (even if chat previously had pins)
            // First put a local pin in state via WS PinnedMessageChanged event, then refresh when server returns pinned_message_id=null
            awaitState(vm) { it.serverWsConnected }
            server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessageChanged","data":{"chat_id":101,"message_id":501}}}""")
            awaitState(vm) { "501" in (it.chats.firstOrNull { c -> c.id == "101" }?.pinnedMessageIds.orEmpty()) }
            vm.refreshAllFromHost()
            val afterUnpinAllRefresh = awaitState(vm) {
                it.chats.firstOrNull { c -> c.id == "101" }?.pinnedMessageIds?.isEmpty() == true
            }
            assertTrue(afterUnpinAllRefresh.chats.first { it.id == "101" }.pinnedMessageIds.isEmpty())

            // 4. Rapid double-toggle pin on chat 102: request #1 (pin=true) finishes AFTER request #2 (pin=false)
            vm.togglePinChat("102") // Request #1 -> true (delayed)
            assertTrue(firstPinStarted.await(5, TimeUnit.SECONDS))
            vm.togglePinChat("102") // Request #2 -> false (fast)
            assertTrue(secondPinCompleted.await(5, TimeUnit.SECONDS))
            awaitState(vm) { it.chats.first { c -> c.id == "102" }.isPinned == false }

            // Release stale Request #1 -> tokenized CAS guard must ignore its late response!
            allowFirstPinComplete.countDown()
            Thread.sleep(150L)
            assertFalse(
                "Stale Request #1 (is_pinned=true) must NOT clobber settled Request #2 (is_pinned=false)",
                vm.state.value.chats.first { it.id == "102" }.isPinned,
            )
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun signOutToAuthWizardIsSynchronousAndImmediateUserBLoginIsNeverWiped() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> {
                        val user = req.bodyJsonOrNull()?.optString("username").orEmpty()
                        if (user == "userB") {
                            MockHttpResponse(body = """{"user_id":2,"username":"userB","first_name":"User B","session_id":"s2","token":"tok_user_b_1234567890"}""")
                        } else {
                            MockHttpResponse(body = """{"user_id":1,"username":"userA","first_name":"User A","session_id":"s1","token":"tok_user_a_1234567890"}""")
                        }
                    }
                    "/api/v1/auth/me" -> {
                        val auth = req.headers["authorization"].orEmpty()
                        if (auth.contains("tok_user_b")) {
                            MockHttpResponse(body = """{"user_id":2,"username":"userB","first_name":"User B","session_id":"s2"}""")
                        } else {
                            MockHttpResponse(body = """{"user_id":1,"username":"userA","first_name":"User A","session_id":"s1"}""")
                        }
                    }
                    "/api/v1/chats" -> MockHttpResponse(body = """{"chats":[]}""")
                    "/api/v1/folders" -> MockHttpResponse(body = """{"folders":[]}""")
                    "/api/v1/contacts" -> MockHttpResponse(body = """{"contacts":[]}""")
                    "/api/v1/auth/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    "/api/v1/bots/my", "/api/v1/fabushi-bots" -> MockHttpResponse(body = """{"bots":[]}""")
                    "/api/v1/computers" -> MockHttpResponse(body = """{"computers":[]}""")
                    "/api/v1/remote/assist/info" -> MockHttpResponse(body = """{"assist_id":"123456789","allow_assist":false}""")
                    "/api/v1/remote/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    "/api/v1/mcp/grants" -> MockHttpResponse(body = """{"grants":[]}""")
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }

            // User A logs in
            vm.loginWithPassword("userA", "pwA")
            awaitState(vm) { it.isAuthenticated }

            // Sign out -> SQLite session MUST be synchronously cleared BEFORE signOutToAuthWizard returns!
            vm.signOutToAuthWizard()
            assertFalse(vm.state.value.isAuthenticated)
            MahayanaHost(app, featureHostTest = false).use { host ->
                val sessImmediatelyAfterLogout = host.request("telegram.server.session")
                assertFalse(
                    "telegram.server.session MUST be authenticated==false immediately after signOutToAuthWizard()",
                    sessImmediatelyAfterLogout.optBoolean("authenticated", true),
                )
            }

            // Immediately log in as User B -> MUST succeed and never be wiped by any racing logout
            vm.loginWithPassword("userB", "pwB")
            awaitState(vm) { it.isAuthenticated && it.activeAccount?.username == "@userB" }
            Thread.sleep(200L)
            assertTrue(vm.state.value.isAuthenticated)
            MahayanaHost(app, featureHostTest = false).use { host ->
                val sessB = host.request("telegram.server.session")
                assertTrue(sessB.optBoolean("authenticated", false))
                assertEquals("userB", sessB.optString("username"))
            }
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun sameGenerationWsReplacementPreservesConnectedStateAndParsesProtocolObjects() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1","token":"tok space+ws_1234567890"}""",
                    )
                    "/api/v1/auth/me" -> MockHttpResponse(
                        body = """{"user_id":1,"username":"alice","first_name":"Alice","session_id":"s1"}""",
                    )
                    "/api/v1/chats" -> MockHttpResponse(
                        body = """{"chats":[{"id":101,"title":"Original Title","kind":"group","last_message_date_ms":5000}]}""",
                    )
                    "/api/v1/chats/101/messages" -> MockHttpResponse(
                        body = """{"chat_id":101,"has_more":false,"messages":[
                            {
                                "id":601,
                                "chat_id":101,
                                "sender_id":2,
                                "sender_name":"Bob",
                                "text":"Edited reply with MiniApp",
                                "date_ms":5000,
                                "edit_date":"2026-11-23T05:00:00Z",
                                "reply_to":{"reply_to_msg_id":500,"sender_name":"Alice","quote_text":"Original quote"},
                                "inline_keyboard":[[{"text":"Open Dharma","web_app_url":"https://fabushi.ombhrum.com/miniapps/global-dharma/"}]]
                            }
                        ]}""",
                    )
                    "/api/v1/chats/101/pins" -> MockHttpResponse(body = """{"chat_id":101,"pinned_message_ids":[]}""")
                    "/api/v1/chats/101/draft" -> MockHttpResponse(body = """{"chat_id":101,"draft":null}""")
                    "/api/v1/folders" -> MockHttpResponse(body = """{"folders":[]}""")
                    "/api/v1/contacts" -> MockHttpResponse(body = """{"contacts":[]}""")
                    "/api/v1/auth/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    "/api/v1/bots/my", "/api/v1/fabushi-bots" -> MockHttpResponse(body = """{"bots":[]}""")
                    "/api/v1/computers" -> MockHttpResponse(body = """{"computers":[]}""")
                    "/api/v1/remote/assist/info" -> MockHttpResponse(body = """{"assist_id":"123456789","allow_assist":false}""")
                    "/api/v1/remote/sessions" -> MockHttpResponse(body = """{"sessions":[]}""")
                    "/api/v1/mcp/grants" -> MockHttpResponse(body = """{"grants":[]}""")
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }

            vm.loginWithPassword("alice", "pw")
            awaitState(vm) { it.isAuthenticated && it.serverWsConnected && it.chats.size == 1 }
            assertEquals(1, server.wsConnectCount.get())
            // Verify RFC 3986 %20 encoding for spaces in WS token query param
            assertTrue(
                "Space in token must be percent-encoded as %20, got ${server.wsRequests.first().rawQuery}",
                server.wsRequests.first().rawQuery.contains("tok%20space%2Bws_1234567890"),
            )

            // Same-generation WS replacement via refreshAllFromHost()
            vm.refreshAllFromHost()
            awaitCondition { server.wsConnectCount.get() >= 2 }
            Thread.sleep(250L) // Wait for superseded WS #1's onClosed to fire
            assertTrue(
                "Superseded WS #1 closing must NEVER clobber serverWsConnected to false",
                vm.state.value.serverWsConnected,
            )

            // Push ChatAction TitleChanged using canonical "new_title" field over live WS #2
            server.pushWsEvent(
                """{"type":"Update","payload":{"type":"ChatAction","data":{"chat_id":101,"action":"TitleChanged","payload":{"new_title":"Renamed Via New Title"}}}}""",
            )
            val renamed = awaitState(vm) { it.chats.firstOrNull { c -> c.id == "101" }?.title == "Renamed Via New Title" }
            assertEquals("Renamed Via New Title", renamed.chats.first { it.id == "101" }.title)

            // Verify parseServerMessageJson (reply_to object + ISO-8601 edit_date) & parseInlineKeyboard (web_app_url)
            vm.openChat("101")
            val opened = awaitState(vm) { it.messagesByChat["101"]?.any { m -> m.id == "601" } == true }
            val msg = opened.messagesByChat["101"]!!.first { it.id == "601" }
            assertTrue("ISO-8601 edit_date must mark message as edited", msg.edited)
            assertNotNull("reply_to object must parse into replyHeader", msg.replyHeader)
            assertEquals("500", msg.replyHeader?.replyToMessageId)
            assertEquals("Original quote", msg.replyHeader?.quoteText)
            assertEquals(1, msg.inlineKeyboard.size)
            val btn = msg.inlineKeyboard.first().buttons.first()
            assertEquals(InlineKeyboardButtonKind.MINI_APP, btn.kind)
            assertEquals("https://fabushi.ombhrum.com/miniapps/global-dharma/", btn.payload)
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun telegramMikaChannelColonAskIdRoundTripsThroughClickInlineButtonAndNonOwnerDoesNotWipePendingAsk() = runBlocking {
        val app = M3TestApplication()
        val store = ViewModelStore()
        val channel = TelegramMikaChannel()
        val owners = MemoryOwnerIdentity()
        val ownerPeer = ChannelPeer(TelegramMikaChannel.ID, "user:42", "chat:room:7")
        val intruderPeer = ChannelPeer(TelegramMikaChannel.ID, "user:999", "chat:room:7")
        owners.bind(ownerPeer)

        var activeAsk: Delivery.Ask? = null
        val confirmDecisions = CopyOnWriteArrayList<Pair<String, ConfirmChoice>>()
        val answers = CopyOnWriteArrayList<Pair<String, String>>()

        val gateway = ChannelGateway(
            owners = owners,
            activeAsk = { tid -> activeAsk?.takeIf { it.threadId == tid } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean {
                    answers.add(askId to text)
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
        val reg = gateway.start(channel)
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }

            // Empty askId decodes to null
            val (decodedT, decodedEmptyAsk) = vm.parseMikaConfirmPayload("chat%3A1:", "fallback")
            assertEquals("chat:1", decodedT)
            assertNull(decodedEmptyAsk)

            // 1. Colon-containing threadId & askId round-trip cleanly through TelegramMikaChannel -> clickInlineButton
            val colonAsk = Delivery.Ask("del-colon", "chat:room:7", "ask:confirm:42", Delivery.Ask.Kind.Confirm, "Approve colon ask?")
            activeAsk = colonAsk
            channel.send(ownerPeer, colonAsk)
            val card = channel.deliveredEvents().last()
            val approveBtn = card.inlineButtons[0]

            val capturedCalls = CopyOnWriteArrayList<Triple<Boolean, String?, String?>>()
            vm.attachMikaBridge(
                object : MikaBridgeCallbacks {
                    override fun resolveConfirmation(approved: Boolean, threadId: String?, askId: String?) {
                        capturedCalls.add(Triple(approved, threadId, askId))
                    }
                },
            )

            // 2. Non-owner resolveConfirmation MUST NOT wipe owner's pendingAsks
            channel.resolveConfirmation(intruderPeer, "ask:confirm:42", approved = true)
            assertTrue("Non-owner confirmation must not reach gateway route", confirmDecisions.isEmpty())
            assertNotNull("Non-owner must NOT wipe owner's pending confirm ask", channel.pendingAsk("chat:room:7"))

            // 3. Click inline Approve button -> exact decoded threadId & askId forwarded to bridge!
            vm.clickInlineButton("chat:room:7", approveBtn)
            assertEquals(1, capturedCalls.size)
            assertEquals(Triple(true, "chat:room:7", "ask:confirm:42"), capturedCalls[0])

            // Resolve as owner via channel -> succeeds and clears pendingAsk
            assertTrue(channel.resolveConfirmationForThread("chat:room:7", approved = true, askId = "ask:confirm:42"))
            assertEquals(listOf("ask:confirm:42" to ConfirmChoice.Once), confirmDecisions.toList())
            assertNull(channel.pendingAsk("chat:room:7"))

            // 4. Non-owner answerAsk MUST NOT wipe owner's pending Input ask
            val inputAsk = Delivery.Ask("del-input", "chat:room:7", "ask:input:99", Delivery.Ask.Kind.Input, "Enter code")
            activeAsk = inputAsk
            channel.send(ownerPeer, inputAsk)
            channel.answerAsk(intruderPeer, "m-intruder", "spoof", "ask:input:99")
            assertTrue(answers.isEmpty())
            assertNotNull("Non-owner must NOT wipe owner's pending input ask", channel.pendingAsk("chat:room:7"))

            // Owner answerAsk succeeds and clears pendingAsk
            assertTrue(channel.answerAsk(ownerPeer, "m-owner", "1234", "ask:input:99"))
            assertEquals(listOf("ask:input:99" to "1234"), answers.toList())
            assertNull(channel.pendingAsk("chat:room:7"))
        } finally {
            reg.close()
            channel.close()
            store.clear()
            app.cleanup()
        }
    }
}
