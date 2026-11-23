package com.ombhrum.fabushi.e2e

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.FabushiAppAgentSurface
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
import com.ombhrum.fabushi.isAllowedMiniAppUrl
import com.ombhrum.fabushi.isAllowedRemoteComputerUrl
import com.ombhrum.fabushi.miniAppCapabilityAnswer
import com.ombhrum.fabushi.telegram.TelegramChatItem
import com.ombhrum.fabushi.telegram.TelegramThemePreset
import com.ombhrum.fabushi.telegram.TelegramThemeSettings
import com.ombhrum.fabushi.telegram.isValidCustomAssistCode
import com.ombhrum.fabushi.telegram.partitionPinnedChatsGrid
import com.ombhrum.fabushi.telegram.resolveTelegramPalette
import java.net.URLEncoder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.max
import kotlin.math.min
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Tier 2 — Boundary & Corner Cases Opaque-Box E2E Suite (36 Kotlin Tests: Areas A–G).
 */
class E2eTier2BoundaryAndCornerTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    private fun contrastRatio(a: Color, b: Color): Double {
        val la = a.luminance().toDouble()
        val lb = b.luminance().toDouble()
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    // =========================================================================
    // Area A: Auth & Zero-Fake-Data Boundaries (6 Tests: A1..A6)
    // =========================================================================

    @Test
    fun e2e_t2_a1_http401AndWsError401WipeSessionEmptyChatsAndStopReconnect() {
        // Case 1: HTTP 401 on /api/v1/auth/me wipes SQLite session & ViewModel state
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Secret Chat", lastMessageDateMs = 2000L))
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server, "alice", "password123")
                awaitState(fix.vm) { it.chats.size == 1 && it.serverWsConnected }

                server.revokeAllTokensForUser(1L)
                fix.vm.refreshAllFromHost()
                val wiped = awaitState(fix.vm, timeoutMs = 6_000L) { !it.isAuthenticated && it.chats.isEmpty() }
                assertTrue(wiped.messagesByChat.isEmpty())
                assertFalse(wiped.serverWsConnected)
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    assertFalse(host.request("telegram.server.session").getBoolean("authenticated"))
                }
            }
        }

        // Case 2: WS Error(401) wipes session, empties chats, stops WS reconnect
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(201L, 1L, "WS Auth Chat", lastMessageDateMs = 2000L))
            E2eViewModelFixture(wsBaseBackoffMs = 150L).use { fix ->
                fix.loginAndConnect(server, "alice", "password123")
                awaitState(fix.vm) { it.chats.size == 1 && it.serverWsConnected }
                val connectsBefore = server.wsConnectCount.get()

                server.pushWsEvent("""{"type":"Error","payload":{"code":401,"message":"Token expired"}}""")
                val wiped = awaitState(fix.vm) { !it.isAuthenticated && !it.serverWsConnected && it.chats.isEmpty() }
                assertFalse(wiped.authError.isNullOrBlank())

                // Must NOT reconnect after 401
                Thread.sleep(450L)
                assertEquals(connectsBefore, server.wsConnectCount.get())
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    assertFalse(host.request("telegram.server.session").getBoolean("authenticated"))
                }
            }
        }
    }

    @Test
    fun e2e_t2_a2_offlineConnectionRefusedAndHttp500FailClosedWithZeroFakeChatsOrBots() {
        // 1. Offline connection refused during login
        val deadUrl = MockGpuiTelegramServer.closedLoopbackUrl()
        E2eViewModelFixture().use { fix ->
            fix.vm.updateDesktopServerEndpoint(deadUrl)
            awaitState(fix.vm) { it.serverEndpoint == deadUrl }
            fix.vm.loginWithPassword("alice", "pw")
            val failed = awaitState(fix.vm) { !it.isAuthenticating && !it.authError.isNullOrBlank() }
            assertFalse(failed.isAuthenticated)
            assertTrue(failed.chats.isEmpty())
            assertTrue(failed.fabushiBots.isEmpty())
        }

        // 2. HTTP 500 on /api/v1/chats and /api/v1/chats/{id}/messages fails closed
        MockGpuiTelegramServer().use { server ->
            server.addFault(FaultRule(method = "GET", pathPrefix = "/api/v1/chats", status = 500, errorBody = """{"error":"db_down","message":"500 failure"}"""))
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                Thread.sleep(250L)
                assertTrue(fix.vm.state.value.chats.isEmpty())
                assertTrue(fix.vm.state.value.fabushiBots.isEmpty())
            }
        }
    }

    @Test
    fun e2e_t2_a3_authenticatedEmptyAccountKeepsChatsEmptyWithoutStarterSeeds() {
        MockGpuiTelegramServer().use { server ->
            // Seed zero chats for user 1
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitCondition(description = "GET /api/v1/chats called") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats" }
                }
                Thread.sleep(150L)
                assertTrue(fix.vm.state.value.isAuthenticated)
                assertTrue(fix.vm.state.value.chats.isEmpty())
            }
        }
    }

    @Test
    fun e2e_t2_a4_alipayPoll410AndExpiredTerminateCleanlyUnauthenticated() {
        MockGpuiTelegramServer().use { server ->
            // 1. Expired status
            E2eViewModelFixture().use { fix ->
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.startAlipayLogin()
                val aid = awaitState(fix.vm) { !it.alipayAttemptId.isNullOrBlank() }.alipayAttemptId!!
                server.setAlipayAttemptStatus(aid, "expired")
                fix.vm.pollAlipayLogin()
                val after = awaitState(fix.vm) { it.alipayAttemptId == null && !it.authError.isNullOrBlank() }
                assertFalse(after.isAuthenticated)
            }

            // 2. HTTP 410 Gone
            E2eViewModelFixture().use { fix ->
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.startAlipayLogin()
                val aid = awaitState(fix.vm) { !it.alipayAttemptId.isNullOrBlank() }.alipayAttemptId!!
                server.setAlipayAttemptStatus(aid, "expired", httpStatusOverride = 410)
                fix.vm.pollAlipayLogin()
                val after = awaitState(fix.vm) { it.alipayAttemptId == null && !it.authError.isNullOrBlank() }
                assertFalse(after.isAuthenticated)
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    assertFalse(host.request("telegram.server.session").getBoolean("authenticated"))
                }
            }
        }
    }

    @Test
    fun e2e_t2_a5_staleSessionRequestFenceDiscardsSlowUserAChatsAfterUserBLogin() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Alice Private Chat", lastMessageDateMs = 5000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(202L, 2L, "Bob Clean Chat", lastMessageDateMs = 5000L))

            E2eViewModelFixture().use { fix ->
                // Make ONLY the next GET /api/v1/chats slow (600ms) so User A's response arrives AFTER User B logs in
                server.addFault(
                    FaultRule(
                        method = "GET",
                        pathPrefix = "/api/v1/chats",
                        status = 0, // Proceed normally after delay
                        delayMs = 600L,
                        remainingHits = AtomicInteger(1),
                    ),
                )

                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.loginWithPassword("alice", "password123")
                awaitState(fix.vm) { it.isAuthenticated }

                // Deterministically wait until User A's slow GET /api/v1/chats enters the 600ms server delay, then sign out and log in as User B
                awaitCondition(description = "Alice's slow GET /api/v1/chats entered 600ms server delay") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats" }
                }
                fix.vm.signOutToAuthWizard()
                awaitState(fix.vm) { !it.isAuthenticated }

                fix.vm.loginWithPassword("bob", "password123")
                val bobState = awaitState(fix.vm, timeoutMs = 6_000L) {
                    it.isAuthenticated && it.chats.any { c -> c.id == "202" }
                }
                // Wait past User A's 600ms slow response window to ensure it was discarded
                Thread.sleep(750L)
                val finalState = fix.vm.state.value
                assertTrue("User B must see own chat 202", finalState.chats.any { it.id == "202" })
                assertTrue(
                    "SessionRequestFence must discard User A's stale chat 101; got ${finalState.chats.map { it.id }}",
                    finalState.chats.none { it.id == "101" },
                )
                assertEquals(bobState.chats.map { it.id }, finalState.chats.map { it.id })
                assertTrue(
                    "Both Alice's slow GET /api/v1/chats and Bob's GET /api/v1/chats must have executed",
                    server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/chats" } >= 2,
                )
            }
        }
    }

    @Test
    fun e2e_t2_a6_unconfiguredMagpieHonestyZeroFakeModelsOrMoa() {
        E2eViewModelFixture().use { fix ->
            val magpie = fix.vm.state.value.magpieGateway
            assertFalse(magpie.liveReady)
            assertTrue(magpie.presets.all { !it.configured && !it.healthy })
            assertTrue(magpie.presets.none { it.flagshipModel.contains("gemini-3.8-flash") || it.flagshipModel.contains("gpt-6.1-sol") })

            MahayanaHost(fix.app, featureHostTest = false).use { host ->
                val res = host.requestResult(
                    "magpie.chatComplete",
                    JSONObject().put("messages", JSONArray().put(JSONObject().put("role", "user").put("content", "hi"))),
                )
                assertTrue(res.isFailure)
                assertTrue(res.exceptionOrNull()?.message.orEmpty().contains("PROVIDER_NOT_CONFIGURED"))
            }

            // Hermes MoA with 0 keys must fail closed (zero fabricated multi-model reply)
            fix.vm.clearToastBanner()
            fix.vm.runHermesMoaPrompt("Compare models")
            awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
            assertFalse(fix.vm.state.value.hermesStudio.terminalLines.any { it.contains("gemini-3.8-flash") || it.contains("gpt-6.1-sol") })
        }
    }

    // =========================================================================
    // Area B: Pinning, Chat & WS Boundaries (6 Tests: B1..B6)
    // =========================================================================

    @Test
    fun e2e_t2_b1_http403NonAdminPinMessageRollsBackOptimisticPinAndSurfacesError() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Group", kind = "group", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Msg 501", dateMs = 2000L))
            server.addFault(
                FaultRule(
                    method = "POST",
                    pathPrefix = "/api/v1/chats/101/pins",
                    status = 403,
                    errorBody = """{"error":"forbidden","message":"Admin rights required to pin messages"}""",
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "501" } == true }

                fix.vm.clearToastBanner()
                fix.vm.togglePinMessage("101", "501")
                val after = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertFalse(after.chats.first { it.id == "101" }.pinnedMessageIds.contains("501"))
                assertFalse(after.messagesByChat["101"]!!.first { it.id == "501" }.pinned)
            }
        }
    }

    @Test
    fun e2e_t2_b2_http500PinChatRevertsIsPinnedToAuthoritativeValue() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Chat 101", isPinned = false, lastMessageDateMs = 2000L))
            server.addFault(FaultRule(method = "POST", pathPrefix = "/api/v1/chats/101/pin", status = 500))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.chats.any { c -> c.id == "101" } }

                fix.vm.clearToastBanner()
                fix.vm.togglePinChat("101")
                val after = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertFalse("Failed pinChat must revert to false", after.chats.first { it.id == "101" }.isPinned)
            }
        }
    }

    @Test
    fun e2e_t2_b3_blankTextMediaOnlyMessagesArePreservedInMessagesByChat() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Media Chat", lastMessageDateMs = 4000L))
            server.seedMessage(
                MockGpuiTelegramServer.MockMessage(
                    id = 601L, chatId = 101L, senderId = 2L, senderName = "Bob",
                    text = "", contentType = "photo", dateMs = 2000L,
                    fileId = "f_photo_1", filename = "sunset.jpg", mimeType = "image/jpeg",
                ),
            )
            server.seedMessage(
                MockGpuiTelegramServer.MockMessage(
                    id = 602L, chatId = 101L, senderId = 2L, senderName = "Bob",
                    text = "", contentType = "voice", dateMs = 3000L,
                    fileId = "f_voice_1", filename = "note.ogg", mimeType = "audio/ogg",
                ),
            )
            server.seedMessage(
                MockGpuiTelegramServer.MockMessage(
                    id = 603L, chatId = 101L, senderId = 2L, senderName = "Bob",
                    text = "", contentType = "document", dateMs = 4000L,
                    fileId = "f_doc_1", filename = "spec.pdf", mimeType = "application/pdf",
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                val state = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 3 }
                val msgs = state.messagesByChat["101"]!!
                assertEquals(listOf("photo", "voice", "document"), msgs.map { it.contentType })
                assertTrue(msgs.all { it.text.isEmpty() && !it.mediaBlobId.isNullOrBlank() })
            }
        }
    }

    @Test
    fun e2e_t2_b4_sendMessageFailureMarksFailedAndRetryReusesExactClientNonceWithoutDuplicates() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Retry Chat", lastMessageDateMs = 2000L))
            // Fail ONLY the first /api/v1/messages/send attempt
            server.addFault(
                FaultRule(
                    method = "POST",
                    pathPrefix = "/api/v1/messages/send",
                    status = 500,
                    remainingHits = AtomicInteger(1),
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.activeChatId == "101" }

                fix.vm.sendComposerMessage("101", "Idempotent message")
                val failedState = awaitState(fix.vm) {
                    it.messagesByChat["101"]?.any { m -> m.text == "Idempotent message" && m.deliveryState == "failed" } == true
                }
                val failedMsg = failedState.messagesByChat["101"]!!.first { it.text == "Idempotent message" }
                assertFalse(failedMsg.deliveryState == "sent" || failedMsg.deliveryState == "read")

                fix.vm.retryFailedMessage("101", failedMsg.id)
                val sentState = awaitState(fix.vm) {
                    it.messagesByChat["101"]?.any { m -> m.text == "Idempotent message" && m.deliveryState == "sent" } == true
                }
                val matching = sentState.messagesByChat["101"]!!.filter { it.text == "Idempotent message" }
                assertEquals("Retry must not duplicate message", 1, matching.size)

                val sendReqs = server.recordedRequests.filter { it.method == "POST" && it.path == "/api/v1/messages/send" }
                assertEquals(2, sendReqs.size)
                val nonce1 = sendReqs[0].bodyJsonOrNull()?.optString("client_nonce")
                val nonce2 = sendReqs[1].bodyJsonOrNull()?.optString("client_nonce")
                assertFalse(nonce1.isNullOrBlank())
                assertEquals("Retry must reuse exact client_nonce", nonce1, nonce2)
            }
        }
    }

    @Test
    fun e2e_t2_b5_bearerTokenWithSpecialCharsIsUrlEncodedInWsQueryString() {
        MockGpuiTelegramServer().use { server ->
            val specialToken = "tok+special/char=1&2_abcdefghijklmnop"
            server.overrideNextLoginToken = specialToken

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server, "alice", "password123", awaitWs = true)
                val wsReq = server.wsRequests.first { it.path == "/api/v1/ws" }
                val expectedEncoded = URLEncoder.encode(specialToken, "UTF-8")
                assertTrue(
                    "Expected URL-encoded token '$expectedEncoded' in query '${wsReq.rawQuery}'",
                    wsReq.rawQuery.contains("token=$expectedEncoded"),
                )
                assertEquals("Bearer $specialToken", wsReq.authorization)
            }
        }
    }

    @Test
    fun e2e_t2_b6_addContact404RollsBackAndEmptyPaginationStopsCleanly() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Small Chat", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Only msg", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)

                // addContact 404 -> zero ghost contact
                fix.vm.clearToastBanner()
                fix.vm.addContactByUsername("nonexistent_ghost_user")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertTrue(fix.vm.state.value.serverContacts.isEmpty())

                // Open chat -> single message, has_more == false -> loadOlderMessages is a no-op
                fix.vm.openChat("101")
                val opened = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 1 }
                assertFalse(opened.chats.first { it.id == "101" }.hasMoreHistory)
                val beforeCount = server.recordedRequests.count { it.path == "/api/v1/chats/101/messages" }
                fix.vm.loadOlderMessages("101")
                Thread.sleep(200L)
                val afterCount = server.recordedRequests.count { it.path == "/api/v1/chats/101/messages" }
                assertEquals("Should not re-fetch once has_more == false", beforeCount, afterCount)
            }
        }
    }

    // =========================================================================
    // Area C: Bots & Coding Agents Boundaries (5 Tests: C1..C5)
    // =========================================================================

    @Test
    fun e2e_t2_c1_invalidBotUsernamesRejected400WithZeroFakeBotCreated() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                for (bad in listOf("ab", "9bot", "my_agent", "bad\$bot")) {
                    fix.vm.clearToastBanner()
                    fix.vm.createServerBot("Bad Bot", bad)
                    awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                }
                assertTrue(fix.vm.state.value.fabushiBots.isEmpty())
                assertTrue(fix.vm.state.value.lastCreatedBotToken.isNullOrBlank())
            }
        }
    }

    @Test
    fun e2e_t2_c2_nonOwnerGetBotProfileStripsAiPersonaAndPatch403FailsClosed() {
        MockGpuiTelegramServer().use { server ->
            // Bot owned by user 2 (Bob), while user 1 (Alice) queries & attempts PATCH
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9500L,
                    ownerUserId = 2L,
                    agentId = "ag-bob",
                    username = "bobs_private_bot",
                    displayName = "Bob Private Bot",
                    aiPersona = "TOP_SECRET_SYSTEM_PROMPT",
                    botToken = "srv_tok_9500",
                    companionChatId = 350L,
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server, "alice", "password123")
                fix.vm.getBotProfile("bobs_private_bot")
                val withProf = awaitState(fix.vm) { !it.lastBotProfileJson.isNullOrBlank() }
                val profJson = withProf.lastBotProfileJson!!
                assertFalse("Non-owner profile must strip ai_persona", profJson.contains("TOP_SECRET_SYSTEM_PROMPT"))
                assertFalse(JSONObject(profJson).has("ai_persona"))

                fix.vm.clearToastBanner()
                fix.vm.patchFabushiBot("9500", displayName = "Hijacked")
                val afterPatch = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                val toast = afterPatch.toastBanner.orEmpty()
                assertTrue("HTTP 403 patchFabushiBot must surface failure banner, got '$toast'", toast.contains("失败"))
                assertFalse("HTTP 403 patchFabushiBot must NOT surface success banner, got '$toast'", toast.contains("已更新"))
                assertTrue(
                    "Non-owner 403 patch must not inject or mutate Hijacked bot into state",
                    afterPatch.fabushiBots.none { it.displayName == "Hijacked" },
                )
                assertTrue(server.recordedRequests.any { it.method == "PATCH" && it.path == "/api/v1/fabushi-bots/9500" })

                // Re-fetch Bob's bot profile from server and confirm display_name is completely unchanged
                val profileReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/bots/profile/bobs_private_bot" }
                fix.vm.getBotProfile("bobs_private_bot")
                awaitCondition(description = "re-fetch Bob bot profile after rejected 403 PATCH") {
                    server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/bots/profile/bobs_private_bot" } > profileReqsBefore
                }
                val refetched = awaitState(fix.vm) { !it.lastBotProfileJson.isNullOrBlank() }
                val refetchedRaw = refetched.lastBotProfileJson!!
                val refetchedJson = JSONObject(refetchedRaw)
                val botDisplayName = refetchedJson.optJSONObject("bot")?.optString("display_name")
                    ?.ifBlank { refetchedJson.optString("display_name") }
                    ?: refetchedJson.optString("display_name")
                assertEquals("Bob Private Bot", botDisplayName)
                assertFalse(refetchedRaw.contains("Hijacked"))
            }
        }
    }

    @Test
    fun e2e_t2_c3_offlineBotCreationFailsClosedWithZeroFakeTgBotToken() {
        MockGpuiTelegramServer().use { server ->
            server.addFault(FaultRule(method = "POST", pathPrefix = "/api/v1/bots", status = 500))
            server.addFault(FaultRule(method = "POST", pathPrefix = "/api/v1/fabushi-bots", status = 500))
            server.addFault(FaultRule(method = "POST", pathPrefix = "/api/v1/botfather/nl-create", status = 500))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)

                fix.vm.clearToastBanner()
                fix.vm.createServerBot("Fail Bot", "fail_one_bot")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }

                fix.vm.clearToastBanner()
                fix.vm.createFabushiBot("Fail AI", "fail_two_bot")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }

                fix.vm.clearToastBanner()
                fix.vm.botfatherNlCreate("Build a bot")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }

                assertTrue(fix.vm.state.value.fabushiBots.isEmpty())
                assertFalse(fix.vm.state.value.lastCreatedBotToken.orEmpty().startsWith("tg_bot_"))
            }
        }
    }

    @Test
    fun e2e_t2_c4_probeCodingAgentsAllNotInstalledRendersHonestStatus() {
        MockGpuiTelegramServer().use { server ->
            // Default codingAgentProbes has all 6 providers installed = false
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.probeCodingAgents()
                val probed = awaitState(fix.vm) { it.codingAgentProbes.size == 6 }
                assertTrue(probed.codingAgentProbes.all { !it.installed })
                assertFalse(probed.toastBanner.orEmpty().contains("原生编程智能体已就绪"))
            }
        }
    }

    @Test
    fun e2e_t2_c5_botActivityChangedForNonActiveChatAndIdleClearsSafely() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Active Chat", lastMessageDateMs = 3000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(102L, 1L, "Background Bot", isBot = true, lastMessageDateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.activeChatId == "101" && it.serverWsConnected }

                // Event for non-active chat 102 and unknown chat 99999 must not crash
                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":99999,"bot_user_id":888,"state":"thinking","detail":"Ghost"}}}""")
                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":102,"bot_user_id":901,"state":"thinking","detail":"Bg work"}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "102" }.botExecutionState == "thinking" }
                assertEquals("101", fix.vm.state.value.activeChatId)

                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":102,"bot_user_id":901,"state":"idle"}}}""")
                val cleared = awaitState(fix.vm) { it.chats.first { c -> c.id == "102" }.botExecutionState == "idle" }
                assertTrue(cleared.chats.first { it.id == "102" }.typingLabel.isNullOrBlank())
            }
        }
    }

    // =========================================================================
    // Area D: Remote Control & Mika Boundaries (5 Tests: D1..D5)
    // =========================================================================

    @Test
    fun e2e_t2_d1_assistSetCodeEnforcesSpec84BoundsLettersAndDigits() {
        // Pure-Kotlin UI helper check (Spec §8.4: 8..16 ASCII alnum with BOTH letters AND digits)
        assertFalse(isValidCustomAssistCode(""))
        assertFalse(isValidCustomAssistCode("short67")) // 7 chars
        assertFalse(isValidCustomAssistCode("Abcdefgh123456789")) // 17 chars
        assertFalse(isValidCustomAssistCode("abc!@#$%1")) // non-alnum
        assertFalse(isValidCustomAssistCode("12345678")) // digits only
        assertFalse(isValidCustomAssistCode("abcdefgh")) // letters only
        assertTrue(isValidCustomAssistCode("Abcd1234")) // 8 chars letters+digits
        assertTrue(isValidCustomAssistCode("AbCdEfGh12345678")) // 16 chars letters+digits

        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.refreshRemoteAssist()
                awaitState(fix.vm) { it.cloudComputer.assistConfig.assistId == "987654321" }

                for (bad in listOf("", "short67", "Abcdefgh123456789", "abc!@#$%1", "12345678", "abcdefgh")) {
                    fix.vm.clearToastBanner()
                    fix.vm.updateRemoteAssistCode(bad)
                    awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                    assertFalse("Invalid assist code '$bad' must NOT persist", fix.vm.state.value.cloudComputer.assistConfig.currentCode == bad)
                }

                fix.vm.updateRemoteAssistCode("Abcd1234")
                awaitState(fix.vm) { it.cloudComputer.assistConfig.currentCode == "Abcd1234" }

                fix.vm.updateRemoteAssistCode("AbCdEfGh12345678")
                awaitState(fix.vm) { it.cloudComputer.assistConfig.currentCode == "AbCdEfGh12345678" }
            }
        }
    }

    @Test
    fun e2e_t2_d2_assistStartDisabledWrongCodeAndUnknownIdFailClosed() {
        MockGpuiTelegramServer().use { server ->
            server.assistAllow = false
            server.assistCode = "Good1234"

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)

                // 1. allow_assist == false -> 403
                fix.vm.clearToastBanner()
                fix.vm.startRemoteAssist("987654321", "Good1234")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertTrue(fix.vm.state.value.cloudComputer.remoteSessions.isEmpty())

                // 2. Wrong code -> 403
                server.assistAllow = true
                fix.vm.clearToastBanner()
                fix.vm.startRemoteAssist("987654321", "Wrong999")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertTrue(fix.vm.state.value.cloudComputer.remoteSessions.isEmpty())

                // 3. Unknown 9-digit assist ID -> 404
                fix.vm.clearToastBanner()
                fix.vm.startRemoteAssist("111222333", "Good1234")
                awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertTrue(fix.vm.state.value.cloudComputer.remoteSessions.isEmpty())
            }
        }
    }

    @Test
    fun e2e_t2_d3_execComputerDisabled403AndComputerExecuteOpStaleGenerationFailClosed() {
        MockGpuiTelegramServer().use { server ->
            server.seedComputer(
                MockGpuiTelegramServer.MockComputer(
                    nodeId = "cloud:1:locked",
                    ownerUserId = 1L,
                    displayName = "Locked Box",
                    host = "10.0.0.99",
                    online = true,
                    runPolicy = "disabled",
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.cloudComputer.nodes.any { n -> n.id == "cloud:1:locked" } }

                fix.vm.clearToastBanner()
                fix.vm.execComputer("cloud:1:locked", JSONObject().put("op", "shell_exec").put("command", "id"))
                val after403 = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertFalse(after403.cloudComputer.lastOpOutput.contains("执行完成 (0)"))

                // Direct JNI computer.executeOp stale generation & LOCAL_EXECUTOR_REQUIRED
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val staleRes = host.requestResult(
                        "computer.executeOp",
                        JSONObject().put("nodeId", "local-android-mika").put("generation", 999999).put("op", "tap"),
                    )
                    assertTrue(staleRes.isFailure)

                    val lease = host.request("computer.leaseAcquire", JSONObject().put("nodeId", "local-android-mika").put("holder", "e2e"))
                    val gen = lease.getLong("generation")
                    val localErr = host.requestResult(
                        "computer.executeOp",
                        JSONObject().put("nodeId", "local-android-mika").put("generation", gen).put("op", "tap"),
                    )
                    assertTrue(localErr.isFailure && localErr.exceptionOrNull()?.message.orEmpty().contains("LOCAL_EXECUTOR_REQUIRED"))
                }
            }
        }
    }

    @Test
    fun e2e_t2_d4_upsertCloudEmptyHostTestFailureAndRemoteComputerUrlAllowlist() {
        // RemoteComputerSurface URL allowlist
        assertTrue(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com/remote/1"))
        assertTrue(isAllowedRemoteComputerUrl("https://remote.fabushi.ombhrum.com/session/2"))
        assertFalse(isAllowedRemoteComputerUrl("http://fabushi.ombhrum.com/remote/1"))
        assertFalse(isAllowedRemoteComputerUrl("https://evil.example/remote/1"))
        assertFalse(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com@evil.example/remote/1"))
        assertFalse(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com:8443/remote/1"))

        MockGpuiTelegramServer().use { server ->
            server.seedComputer(
                MockGpuiTelegramServer.MockComputer(
                    nodeId = "cloud:1:down",
                    ownerUserId = 1L,
                    displayName = "Down Box",
                    host = "10.0.0.200",
                    online = false,
                    testShouldSucceed = false,
                    testFailureStderr = "Connection refused",
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.cloudComputer.nodes.any { n -> n.id == "cloud:1:down" } }

                // Empty host rejected (400 or client-side validation)
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val emptyHostRes = host.requestResult(
                        "telegram.server.upsertCloudComputer",
                        JSONObject().put("displayName", "Bad").put("host", ""),
                    )
                    assertTrue(emptyHostRes.isFailure)
                }

                // testComputer failure keeps online == false
                fix.vm.clearToastBanner()
                fix.vm.testComputer("cloud:1:down")
                val afterTest = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertFalse(afterTest.cloudComputer.nodes.first { it.id == "cloud:1:down" }.online)
            }
        }
    }

    @Test
    fun e2e_t2_d5_fabushiAppAgentSurfaceRejectsStaleGenHiddenSensitiveAndLegacyActions() {
        val surface = FabushiAppAgentSurface()
        surface.publish(
            screen = "telegram.chat",
            elements = listOf(
                FabushiAppAgentSurface.Element("btn.visible", "button", "Send", visible = true),
                FabushiAppAgentSurface.Element("btn.hidden", "button", "Hidden", visible = false),
                FabushiAppAgentSurface.Element("input.pin", "textbox", "Passcode", visible = true, sensitive = true),
                FabushiAppAgentSurface.Element("input.composer", "textbox", "Message", visible = true, sensitive = false),
            ),
            actions = mapOf(
                "btn.visible" to FabushiAppAgentSurface.Action(setOf("invoke")) {},
                "btn.hidden" to FabushiAppAgentSurface.Action(setOf("invoke")) {},
                "input.pin" to FabushiAppAgentSurface.Action(setOf("setValue")) {},
                "input.composer" to FabushiAppAgentSurface.Action(setOf("setValue")) {},
            ),
        )
        val gen = surface.snapshot().generation

        val staleErr = runCatching { surface.action(gen - 1, "btn.visible", "invoke") }.exceptionOrNull()
        assertEquals("stale_app_surface_generation", staleErr?.message)

        val hiddenErr = runCatching { surface.action(gen, "btn.hidden", "invoke") }.exceptionOrNull()
        assertEquals("app_surface_target_hidden", hiddenErr?.message)

        val sensErr = runCatching { surface.action(gen, "input.pin", "setValue", "1234") }.exceptionOrNull()
        assertEquals("sensitive_app_surface_input_requires_secure_input", sensErr?.message)

        val tapErr = runCatching { surface.action(gen, "btn.visible", "tap") }.exceptionOrNull()
        assertEquals("unsupported_app_surface_action", tapErr?.message)

        val setTextErr = runCatching { surface.action(gen, "input.composer", "set_text", "x") }.exceptionOrNull()
        assertEquals("unsupported_app_surface_action", setTextErr?.message)
    }

    // =========================================================================
    // Area E: JNI Concurrency, Security & Error Boundaries (5 Tests: E1..E5)
    // =========================================================================

    @Test
    fun e2e_t2_e1_lockFreeConcurrentJniProofUnder1500msSlowHttp() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Slow Chat", lastMessageDateMs = 2000L))
            val app = TestFabushiApplication()
            try {
                MahayanaHost(app, featureHostTest = false).use { host ->
                    host.request(
                        "telegram.server.login",
                        JSONObject().put("endpoint", server.baseUrl).put("username", "alice").put("password", "pw"),
                    )
                    // Inject 1500ms delay ONLY on GET /api/v1/chats (slept OUTSIDE server state lock)
                    server.addFault(
                        FaultRule(
                            method = "GET",
                            pathPrefix = "/api/v1/chats",
                            status = 0,
                            delayMs = 1_500L,
                            remainingHits = AtomicInteger(1),
                        ),
                    )

                    val threadAStarted = CountDownLatch(1)
                    val threadADone = CountDownLatch(1)
                    val threadAElapsedMs = AtomicLong(0L)
                    val threadAError = AtomicReference<Throwable?>()

                    val threadA = Thread({
                        threadAStarted.countDown()
                        val t0 = System.nanoTime()
                        try {
                            host.request("telegram.server.listChats")
                        } catch (t: Throwable) {
                            threadAError.set(t)
                        } finally {
                            threadAElapsedMs.set((System.nanoTime() - t0) / 1_000_000L)
                            threadADone.countDown()
                        }
                    }, "e2e-jni-thread-a")
                    threadA.start()

                    assertTrue(threadAStarted.await(2, TimeUnit.SECONDS))
                    awaitCondition(description = "Thread A GET /api/v1/chats entered 1500ms server delay") {
                        server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats" }
                    }

                    // Thread B runs local JNI calls on the SAME MahayanaHost while Thread A is in flight!
                    val tB0 = System.nanoTime()
                    val info = host.request("feature.info")
                    val sess = host.request("telegram.server.session")
                    host.request("feature.persistence.set", JSONObject().put("key", "e2e_lockfree").put("value", "ok"))
                    val threadBElapsedMs = (System.nanoTime() - tB0) / 1_000_000L

                    assertEquals("1.6.6", info.getString("desktopParityVersion"))
                    assertTrue(sess.getBoolean("authenticated"))
                    assertTrue(
                        "Thread B MUST complete in < 300ms while Thread A is blocked on 1500ms HTTP (actual: ${threadBElapsedMs}ms)",
                        threadBElapsedMs < 300L,
                    )

                    assertTrue(threadADone.await(5, TimeUnit.SECONDS))
                    assertEquals(null, threadAError.get())
                    assertTrue("Thread A should have taken >= 1300ms (actual ${threadAElapsedMs.get()}ms)", threadAElapsedMs.get() >= 1_300L)
                }
            } finally {
                app.cleanup()
            }
        }
    }

    @Test
    fun e2e_t2_e2_mahayanaHostRequestResultFailsClosedNeverSwallowsError() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                val r1 = host.requestResult("telegram.server.pinChat", JSONObject().put("chatId", 101).put("pinned", true))
                assertTrue("Unauthenticated pinChat must return Result.failure", r1.isFailure)
                assertTrue(r1.exceptionOrNull()?.message.orEmpty().contains("AUTH_REQUIRED"))

                val r2 = host.requestResult("unknown.method.does.not.exist", JSONObject())
                assertTrue(r2.isFailure)
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t2_e3_passcodeRejectsEmptyShortAndWrongPinWithZeroPlaintext() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                assertTrue(host.requestResult("telegram.auth.passcodeSet", JSONObject().put("passcode", "")).isFailure)
                assertTrue(host.requestResult("telegram.auth.passcodeSet", JSONObject().put("passcode", "123")).isFailure)

                host.request("telegram.auth.passcodeSet", JSONObject().put("passcode", "9876"))
                assertFalse(host.request("telegram.auth.passcodeVerify", JSONObject().put("passcode", "wrong")).getBoolean("valid"))
                assertTrue(host.request("telegram.auth.passcodeVerify", JSONObject().put("passcode", "9876")).getBoolean("valid"))
            }
            assertTrue(app.telegramPrefs().all.values.none { it.toString().contains("9876") })
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t2_e4_malformedInputAndUnknownMethodOnMahayanaHostFailClosedWithoutCrash() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                val unknown = host.requestResult("totally.unknown.rpc", JSONObject().put("bad", "payload"))
                assertTrue(unknown.isFailure)
                assertFalse(unknown.exceptionOrNull()?.message.isNullOrBlank())

                // Missing required params on multiple RPCs must fail closed cleanly (zero native abort/panic)
                val badSend = host.requestResult("wallet.sendTon", JSONObject())
                assertTrue(badSend.isFailure)

                val badExec = host.requestResult("telegram.server.execComputer", JSONObject().put("nodeId", "n1").put("operation", JSONObject().put("op", "not_a_real_op")))
                assertTrue(badExec.isFailure)

                // Host remains healthy after errors
                assertEquals("1.6.6", host.request("feature.info").getString("desktopParityVersion"))
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t2_e5_walletSendTonBoundariesTonConnectUnavailableAndZeroFakeSwapStake() {
        E2eViewModelFixture().use { fix ->
            MahayanaHost(fix.app, featureHostTest = false).use { host ->
                val w = host.request("wallet.createWallet", JSONObject().put("initialNanotons", 500_000_000L))
                val addr = w.getString("address")

                // amountNanotons == 0 rejected
                assertTrue(host.requestResult("wallet.sendTon", JSONObject().put("to", addr).put("amountNanotons", 0L)).isFailure)
                // Invalid address rejected
                assertTrue(host.requestResult("wallet.sendTon", JSONObject().put("to", "not-a-ton-address!!!").put("amountNanotons", 100_000_000L)).isFailure)
                // Insufficient balance rejected
                assertTrue(host.requestResult("wallet.sendTon", JSONObject().put("to", addr).put("amountNanotons", 9_000_000_000L)).isFailure)
                // Balance untouched
                assertEquals(500_000_000L, host.request("wallet.status").getLong("balanceNanotons"))

                // tonConnectApprove unconditionally fails closed with TONCONNECT_UNAVAILABLE
                val tcErr = host.requestResult(
                    "wallet.tonConnectApprove",
                    JSONObject().put("recipient", addr).put("amountNanotons", 100_000_000L).put("dappName", "TestDApp"),
                )
                assertTrue(tcErr.isFailure && tcErr.exceptionOrNull()?.message.orEmpty().contains("TONCONNECT_UNAVAILABLE"))
                assertEquals(500_000_000L, host.request("wallet.status").getLong("balanceNanotons"))
            }

            // ViewModel swap & stake must reject honestly without fake 5.42 / STON.fi fallback
            fix.vm.clearToastBanner()
            fix.vm.swapTonToUsdt(1.0)
            val afterSwap = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
            assertFalse(afterSwap.toastBanner.orEmpty().contains("5.42"))
            assertFalse(afterSwap.toastBanner.orEmpty().contains("STON.fi"))

            fix.vm.clearToastBanner()
            fix.vm.stakeTon(1.0)
            val afterStake = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
            assertFalse(afterStake.toastBanner.orEmpty().contains("APY"))
        }
    }

    // =========================================================================
    // Area F: Apple iOS UI/UX & Contrast Boundaries (5 Tests: F1..F5)
    // =========================================================================

    @Test
    fun e2e_t2_f1_appleDayWhiteOnWhiteRegressionGuard() {
        val day = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_DAY))
        assertTrue("searchCursorColor on inputBg", contrastRatio(day.searchCursorColor, day.inputBg) >= 3.0)
        assertTrue("searchCursorColor on appBackground", contrastRatio(day.searchCursorColor, day.appBackground) >= 3.0)
        assertTrue("selectedPillContent on selectedPillBackground", contrastRatio(day.selectedPillContent, day.selectedPillBackground) >= 3.0)
        assertTrue("unreadBadgeContent on unreadBadgeActive", contrastRatio(day.unreadBadgeContent, day.unreadBadgeActive) >= 3.0)
        assertTrue("topBarActionColor on topBarBackground", contrastRatio(day.topBarActionColor, day.topBarBackground) >= 3.0)
        assertFalse(day.topBarActionColor == Color(0xFFFFFFFF))
        assertFalse(day.searchCursorColor == Color(0xFFFFFFFF))
    }

    @Test
    fun e2e_t2_f2_appleDayBlueOnBlueRegressionGuard() {
        val day = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_DAY))
        val outgoingControls = listOf(
            "outgoingCheckMarkColor" to day.outgoingCheckMarkColor,
            "outgoingLinkColor" to day.outgoingLinkColor,
            "outgoingReplyBarColor" to day.outgoingReplyBarColor,
            "outgoingReactionTextColor" to day.outgoingReactionTextColor,
        )
        for ((name, color) in outgoingControls) {
            assertFalse("$name must NEVER be #007AFF on blue bubble", color == Color(0xFF007AFF))
            assertTrue("$name on outgoingBubbleBottom >= 3.0", contrastRatio(color, day.outgoingBubbleBottom) >= 3.0)
            assertTrue("$name on outgoingBubbleTop >= 3.0", contrastRatio(color, day.outgoingBubbleTop) >= 3.0)
        }
    }

    @Test
    fun e2e_t2_f3_themeSettingsClampingCornerRadiusAndFontSize() {
        val tooHigh = TelegramThemeSettings(bubbleCornerRadiusDp = 999, messageFontSizeSp = 80)
        assertEquals(18.dp, tooHigh.boundedCornerDp)
        assertEquals(22.sp, tooHigh.boundedFontSp)

        val tooLow = TelegramThemeSettings(bubbleCornerRadiusDp = -5, messageFontSizeSp = 4)
        assertEquals(4.dp, tooLow.boundedCornerDp)
        assertEquals(12.sp, tooLow.boundedFontSp)
    }

    @Test
    fun e2e_t2_f4_partitionPinnedChatsGridBoundaryCounts0To5() {
        fun chat(i: Int) = TelegramChatItem(
            id = "c$i", title = "C$i", subtitle = "", preview = "", time = "10:00",
            kind = ConversationKind.DIRECT, isPinned = true,
        )

        val p0 = partitionPinnedChatsGrid(emptyList())
        assertTrue(p0.rows.isEmpty())
        assertFalse(p0.centeredSingleRow)

        val p1 = partitionPinnedChatsGrid(listOf(chat(1)))
        assertTrue(p1.centeredSingleRow)
        assertEquals(listOf(listOf("c1")), p1.rows.map { r -> r.map { it.id } })

        val p2 = partitionPinnedChatsGrid(listOf(chat(1), chat(2)))
        assertTrue(p2.centeredSingleRow)
        assertEquals(listOf(listOf("c1", "c2")), p2.rows.map { r -> r.map { it.id } })

        val p3 = partitionPinnedChatsGrid((1..3).map(::chat))
        assertFalse(p3.centeredSingleRow)
        assertEquals(listOf(3), p3.rows.map { it.size })

        val p4 = partitionPinnedChatsGrid((1..4).map(::chat))
        assertFalse(p4.centeredSingleRow)
        assertEquals(listOf(3, 1), p4.rows.map { it.size })

        val p5 = partitionPinnedChatsGrid((1..5).map(::chat))
        assertFalse(p5.centeredSingleRow)
        assertEquals(listOf(3, 2), p5.rows.map { it.size })
        assertEquals(listOf("c1", "c2", "c3", "c4", "c5"), p5.rows.flatten().map { it.id })
    }

    @Test
    fun e2e_t2_f5_deniedAudioAndGpsPermissionsBlockCleanlyAndAttachmentSheetStateHoisted() {
        // TestFabushiApplication has grantedPermissions = emptySet() by default -> PERMISSION_DENIED
        E2eViewModelFixture().use { fix ->
            assertFalse(fix.vm.hasRecordAudioPermission())
            assertFalse(fix.vm.canSendCurrentLocation())

            // 1. Denied RECORD_AUDIO refuses recording with zero synthetic waveform
            fix.vm.clearToastBanner()
            fix.vm.startVoiceOrVideoRecording()
            assertFalse(fix.vm.state.value.recordingState.isRecording)
            assertTrue(fix.vm.state.value.recordingState.liveBars.isEmpty())
            assertTrue(fix.vm.state.value.toastBanner.orEmpty().contains("RECORD_AUDIO"))

            // 2. Denied GPS refuses sendLocation (zero fake Hangzhou 30.2741, 120.1551 message)
            fix.vm.openSavedMessages()
            val savedId = awaitState(fix.vm) { it.activeChatId != null }.activeChatId!!
            val beforeMsgs = fix.vm.state.value.messagesByChat[savedId].orEmpty().size
            fix.vm.clearToastBanner()
            fix.vm.sendLocation(savedId, 30.2741, 120.1551, "Hangzhou")
            assertEquals(beforeMsgs, fix.vm.state.value.messagesByChat[savedId].orEmpty().size)
            assertTrue(fix.vm.state.value.toastBanner.orEmpty().contains("位置权限"))

            // 3. Attachment sheet overlay state is hoisted in TelegramUiState (outside TelegramComposer)
            assertFalse(fix.vm.state.value.composerAttachmentSheetOpen)
            fix.vm.setComposerAttachmentSheetOpen(true)
            assertTrue(fix.vm.state.value.composerAttachmentSheetOpen)
            fix.vm.setComposerAttachmentSheetOpen(false)
            assertFalse(fix.vm.state.value.composerAttachmentSheetOpen)
        }
    }

    // =========================================================================
    // Area G: MiniApp Security Boundaries (4 Kotlin Tests: G1, G2, G4, G5)
    // =========================================================================

    @Test
    fun e2e_t2_g1_emptyToolListStillAnswersOfflineCapabilitiesWithEmptyArray() {
        val ans = miniAppCapabilityAnswer(emptyList())
        assertEquals("fabushi.miniapp.tools.v1", ans.getString("contract"))
        assertEquals("android", ans.getString("platform"))
        assertEquals(0, ans.getJSONArray("capabilities").length())
    }

    @Test
    fun e2e_t2_g2_isAllowedMiniAppUrlRejectsUntrustedUrlsAndLifecycleGateFlipsInactive() {
        assertFalse(isAllowedMiniAppUrl("http://evil.example"))
        assertFalse(isAllowedMiniAppUrl("https://evil.example/app"))
        assertFalse(isAllowedMiniAppUrl("file:///etc/passwd"))
        assertFalse(isAllowedMiniAppUrl("javascript:alert(1)"))
        assertFalse(isAllowedMiniAppUrl("https://fabushi.ombhrum.com@evil.example/"))
        assertTrue(isAllowedMiniAppUrl("https://fabushi.ombhrum.com/miniapps/global-dharma/"))
        assertTrue(isAllowedMiniAppUrl("https://miniapp.local.fabushi.invalid/miniapps/global-dharma/"))

        val gate = MiniAppLifecycleGate()
        gate.onLocalHtmlLoaded()
        assertTrue(gate.localDocumentActive)
        gate.onPageStarted("http://evil.example")
        assertFalse("Navigating to non-local/untrusted URL must flip localDocumentActive=false", gate.localDocumentActive)
    }

    @Test
    fun e2e_t2_g4_declinedApprovalPromptFailsImmediatelyWithoutExecutingNativeTool() {
        val app = TestFabushiApplication()
        try {
            val plugin = MarketplacePlugin(
                pluginId = "global-dharma",
                displayName = "Global Dharma",
                description = "Test",
                latestVersion = "2.3.0",
                tools = listOf(MiniAppToolContract("transfer.send", "Mutate", "confirm")),
            )
            var executed = 0
            val bridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { true },
                callRuntimeToolJson = { _, _, _ ->
                    executed++
                    """{"ok":true}"""
                },
                approvalPrompt = { false },
            )
            val resp = JSONObject(bridge.callTool("transfer.send", "{}"))
            assertFalse(resp.getBoolean("ok"))
            assertFalse(resp.optString("error").isBlank())
            assertEquals("Tool MUST NOT execute when approvalPrompt declines", 0, executed)
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t2_g5_lateHostEventAfterOnStopOrOnDisposeIsSuppressedSilently() {
        val gate = MiniAppLifecycleGate()
        gate.onLocalHtmlLoaded()
        assertTrue(gate.localDocumentActive)

        var delivered = 0
        gate.onLifecycleEvent(Lifecycle.Event.ON_STOP)
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "late.stop")) { delivered++ })
        assertEquals(0, delivered)

        gate.onDispose()
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "late.dispose")) { delivered++ })
        assertEquals(0, delivered)
    }
}
