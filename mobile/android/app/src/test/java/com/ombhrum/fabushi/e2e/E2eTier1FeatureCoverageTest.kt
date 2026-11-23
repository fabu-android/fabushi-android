package com.ombhrum.fabushi.e2e

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.lifecycle.Lifecycle
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.FabushiRemoteDeviceGateway
import com.ombhrum.fabushi.MarketplacePlugin
import com.ombhrum.fabushi.MiniAppLifecycleGate
import com.ombhrum.fabushi.MiniAppNativeWebMcpBridge
import com.ombhrum.fabushi.MiniAppToolContract
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.e2e.harness.E2eViewModelFixture
import com.ombhrum.fabushi.e2e.harness.MainDispatcherRule
import com.ombhrum.fabushi.e2e.harness.MockGpuiTelegramServer
import com.ombhrum.fabushi.e2e.harness.TestFabushiApplication
import com.ombhrum.fabushi.e2e.harness.awaitCondition
import com.ombhrum.fabushi.e2e.harness.awaitState
import com.ombhrum.fabushi.mika.MikaChannelCardEvent
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import com.ombhrum.fabushi.miniAppAccountProjection
import com.ombhrum.fabushi.miniAppCapabilityAnswer
import com.ombhrum.fabushi.miniAppCapabilityDomains
import com.ombhrum.fabushi.miniAppEventProjection
import com.ombhrum.fabushi.telegram.AppleShellTab
import com.ombhrum.fabushi.telegram.CallPhase
import com.ombhrum.fabushi.telegram.ChatTimelineItem
import com.ombhrum.fabushi.telegram.CloudComputerUiState
import com.ombhrum.fabushi.telegram.CodingProviderKindUi
import com.ombhrum.fabushi.telegram.DEFAULT_DESKTOP_SERVER_URL
import com.ombhrum.fabushi.telegram.GlobalDharmaCatalog
import com.ombhrum.fabushi.telegram.GlobalDharmaUiState
import com.ombhrum.fabushi.telegram.HermesStudioUiState
import com.ombhrum.fabushi.telegram.MagpieGatewayUiState
import com.ombhrum.fabushi.telegram.MagpiePresetCatalog
import com.ombhrum.fabushi.telegram.MessageContentSegment
import com.ombhrum.fabushi.telegram.MikaBridgeUiSnapshot
import com.ombhrum.fabushi.telegram.TelegramAccount
import com.ombhrum.fabushi.telegram.TelegramChatItem
import com.ombhrum.fabushi.telegram.TelegramMessageItem
import com.ombhrum.fabushi.telegram.TelegramSubScreen
import com.ombhrum.fabushi.telegram.TelegramThemePreset
import com.ombhrum.fabushi.telegram.TelegramThemeSettings
import com.ombhrum.fabushi.telegram.TelegramUiState
import com.ombhrum.fabushi.telegram.TelegramWallpaperStyle
import com.ombhrum.fabushi.telegram.TonWalletUiState
import com.ombhrum.fabushi.telegram.buildTimelineItems
import com.ombhrum.fabushi.telegram.canSubmitComposer
import com.ombhrum.fabushi.telegram.parseMessageCodeSegments
import com.ombhrum.fabushi.telegram.partitionPinnedChatsGrid
import com.ombhrum.fabushi.telegram.publishTelegramSemanticSurface
import com.ombhrum.fabushi.telegram.resolveTelegramPalette
import com.ombhrum.fabushi.telegram.resolveUiStrings
import com.ombhrum.fabushi.toolContractJson
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import com.wangzan101.mika.core.run.SendOutcome
import java.io.File
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Tier 1 — Feature Coverage Opaque-Box E2E Suite (43 Kotlin Tests: Areas A–G).
 * Exercises real Rust JNI (`MahayanaHost`), real SQLite WAL cold-restarts, real `TelegramViewModel`,
 * real OkHttp RFC 6455 WebSockets, real `:mika-core` `ChannelGateway`, `FabushiRemoteDeviceGateway`,
 * `TelegramTheme` WCAG 2.1 tokens, and `MiniAppWebMcpSurface` (`AND-GD-01..05`).
 */
class E2eTier1FeatureCoverageTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    private fun contrastRatio(a: Color, b: Color): Double {
        val la = a.luminance().toDouble()
        val lb = b.luminance().toDouble()
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    // =========================================================================
    // Area A: Zero-Fake-Data, Parity Metadata & Real Auth (5 Kotlin Tests: A2..A6)
    // =========================================================================

    @Test
    fun e2e_t1_a2_freshKotlinUiAndModelDefaultsContainZeroFakeData() {
        val ui = TelegramUiState()
        assertFalse(ui.isAuthenticated)
        assertTrue(ui.chats.isEmpty())
        assertTrue(ui.messagesByChat.isEmpty())
        assertTrue(ui.fabushiBots.isEmpty())
        assertTrue(ui.serverSessions.isEmpty())
        assertTrue(ui.serverContacts.isEmpty())
        assertTrue(ui.stories.isEmpty())
        assertEquals(DEFAULT_DESKTOP_SERVER_URL, ui.serverEndpoint)

        val magpie = MagpieGatewayUiState()
        assertFalse(magpie.liveReady)
        assertEquals(48, MagpiePresetCatalog.PRESETS_48.size)
        assertTrue(MagpiePresetCatalog.PRESETS_48.all { !it.configured && !it.healthy })

        val hermes = HermesStudioUiState()
        assertTrue(hermes.memoryBlocks.isEmpty())
        assertTrue(hermes.terminalLines.isEmpty())
        assertTrue(hermes.sessions.isEmpty())
        assertTrue(hermes.approvals.isEmpty())
        assertTrue(hermes.skills.isEmpty())
        assertTrue(hermes.kanbanCards.isEmpty())

        val cloud = CloudComputerUiState()
        assertTrue(cloud.nodes.isEmpty())
        assertTrue(cloud.remoteSessions.isEmpty())
        assertTrue(cloud.mcpGrants.isEmpty())

        val wallet = TonWalletUiState()
        assertFalse(wallet.initialized)
        assertEquals("", wallet.addressFriendly)
        assertTrue(wallet.collectibles.isEmpty())

        val dharma = GlobalDharmaUiState()
        assertTrue(dharma.countries.isEmpty())
        assertTrue(dharma.aiDharmaMessages.isEmpty())
        val catalog = GlobalDharmaCatalog.build240Countries()
        assertTrue(catalog.none { it.nameZh.contains("十方洲岛") || it.nameEn.contains("十方洲岛") })
        assertTrue(catalog.all { it.offeringsCount == 0L })

        val account = TelegramAccount(id = "u1", slotIndex = 0, displayName = "Test", username = "test", phone = "", bio = "")
        assertFalse(account.isPremium)

        val mika = MikaBridgeUiSnapshot()
        assertFalse(mika.available)
        assertFalse(mika.sandboxReady)
        assertFalse(ui.toString().contains("bot-mika-phone"))
    }

    @Test
    fun e2e_t1_a3_configureEndpointLoginMeAndColdRestartPersistSession() {
        MockGpuiTelegramServer().use { server ->
            val rootApp = TestFabushiApplication()
            try {
                // Phase 1: Configure endpoint, login, verify /api/v1/auth/me receives Bearer header
                E2eViewModelFixture(app = rootApp, deleteRootDirOnClose = false).use { fix ->
                    fix.loginAndConnect(server, "alice", "password123")
                    assertTrue(fix.vm.state.value.isAuthenticated)
                    assertTrue(
                        server.recordedRequests.any {
                            it.method == "GET" && it.path == "/api/v1/auth/me" && it.authorization?.startsWith("Bearer ") == true
                        },
                    )
                }

                // Phase 2: Cold restart fresh ViewModel + JNI host on SAME SQLite WAL directory
                val restartedApp = rootApp.coldRestartApp(preservePrefs = false)
                E2eViewModelFixture(
                    app = restartedApp,
                    preseedEndpoint = server.baseUrl,
                    expectAuthenticated = true,
                    deleteRootDirOnClose = false,
                ).use { fix2 ->
                    assertTrue(fix2.vm.state.value.isAuthenticated)
                    MahayanaHost(restartedApp, featureHostTest = false).use { host ->
                        val sess = host.request("telegram.server.session")
                        assertTrue(sess.getBoolean("authenticated"))
                        assertTrue(sess.optString("token").length >= 24)
                        assertEquals("alice", sess.optString("username"))
                    }
                }
            } finally {
                rootApp.cleanup()
            }
        }
    }

    @Test
    fun e2e_t1_a4_registerEmailCodeEmailRegisterAndAlipayPendingToCompleted() {
        MockGpuiTelegramServer().use { server ->
            // 1. Password register
            E2eViewModelFixture().use { fix ->
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.registerWithPassword("new_reg_user", "pw123", "New Reg")
                awaitState(fix.vm) { it.isAuthenticated }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/auth/register" })
            }

            // 2. Email send-code & email register
            E2eViewModelFixture().use { fix ->
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.sendEmailVerificationCode("dharma@example.com")
                awaitState(fix.vm) { !it.emailCodeSentNotice.isNullOrBlank() }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/auth/email/send-code" })

                fix.vm.registerWithEmail("dharma@example.com", "654321", "email_user", "pw123", "Email User")
                awaitState(fix.vm) { it.isAuthenticated }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/auth/email/register" })
            }

            // 3. Alipay start -> poll pending (MUST NOT authenticate) -> poll completed (authenticates & persists)
            E2eViewModelFixture().use { fix ->
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.startAlipayLogin()
                val started = awaitState(fix.vm) { !it.alipayAttemptId.isNullOrBlank() && !it.alipayLoginUrl.isNullOrBlank() }
                val aid = started.alipayAttemptId!!
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/auth/alipay/start" })

                // Pending poll must NOT authenticate or persist SQLite session
                fix.vm.pollAlipayLogin()
                awaitState(fix.vm) { it.alipayStatusText?.contains("pending", ignoreCase = true) == true }
                assertFalse(fix.vm.state.value.isAuthenticated)
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    assertFalse(host.request("telegram.server.session").getBoolean("authenticated"))
                }

                // Completed poll authenticates & persists SQLite session
                server.setAlipayAttemptStatus(aid, "completed")
                fix.vm.pollAlipayLogin()
                awaitState(fix.vm) { it.isAuthenticated }
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/auth/alipay/poll/$aid" })
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    assertTrue(host.request("telegram.server.session").getBoolean("authenticated"))
                }
            }
        }
    }

    @Test
    fun e2e_t1_a5_listSessionsRevokeSessionAndUpdateProfileUseRealServerRoutes() {
        MockGpuiTelegramServer().use { server ->
            server.seedSession(
                MockGpuiTelegramServer.MockSession(
                    sessionId = "sess-macbook-99",
                    userId = 1L,
                    token = "tok_secondary_macbook_0123456789abcdef",
                    deviceName = "MacBook Pro Desktop",
                    platform = "macos",
                    isCurrent = false,
                ),
            )
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server, "alice", "password123")
                val sessReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/auth/sessions" }
                fix.vm.refreshSessions()
                awaitCondition(description = "explicit GET /api/v1/auth/sessions") {
                    server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/auth/sessions" } > sessReqsBefore
                }
                val listed = awaitState(fix.vm) { it.serverSessions.any { s -> s.sessionId == "sess-macbook-99" } }
                assertTrue(listed.serverSessions.none { it.deviceName.contains("iPhone 15 Pro") })
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/auth/sessions" })

                fix.vm.revokeSession("sess-macbook-99")
                awaitState(fix.vm) { it.serverSessions.none { s -> s.sessionId == "sess-macbook-99" } }
                awaitCondition(description = "DELETE /api/v1/auth/sessions/sess-macbook-99") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/auth/sessions/sess-macbook-99" }
                }

                fix.vm.updateProfile(displayName = "Alice Updated", username = "alice", bio = "Real server bio")
                awaitCondition(description = "PUT /api/v1/auth/profile") {
                    server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/auth/profile" }
                }
                val putReq = server.recordedRequests.last { it.method == "PUT" && it.path == "/api/v1/auth/profile" }
                assertEquals("Alice Updated", putReq.bodyJsonOrNull()?.optString("first_name"))
                assertEquals("Real server bio", putReq.bodyJsonOrNull()?.optString("bio"))
            }
        }
    }

    @Test
    fun e2e_t1_a6_logoutClearsSqliteSessionClosesWsAndEmptiesChats() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Alpha Chat", lastMessageDateMs = 5000L))
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server, "alice", "password123")
                awaitState(fix.vm) { it.chats.size == 1 && it.serverWsConnected }

                fix.vm.signOutToAuthWizard()
                val signedOut = awaitState(fix.vm) { !it.isAuthenticated && !it.serverWsConnected && it.chats.isEmpty() }
                assertTrue(signedOut.messagesByChat.isEmpty())

                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val sess = host.request("telegram.server.session")
                    assertFalse(sess.getBoolean("authenticated"))
                    assertEquals("", sess.optString("token"))
                }
            }
        }
    }

    // =========================================================================
    // Area B: Pinning, Chat Lifecycle & WebSocket Engine (7 Kotlin Tests: B1..B7)
    // =========================================================================

    @Test
    fun e2e_t1_b1_pinChatSortingAndColdRestartPersistence() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Chat 101", isPinned = true, lastMessageDateMs = 1000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(102L, 1L, "Chat 102", isPinned = false, lastMessageDateMs = 2000L))
            server.seedChat(MockGpuiTelegramServer.MockChat(103L, 1L, "Chat 103", isPinned = false, lastMessageDateMs = 5000L))

            val rootApp = TestFabushiApplication()
            try {
                E2eViewModelFixture(app = rootApp, deleteRootDirOnClose = false).use { fix ->
                    fix.loginAndConnect(server)
                    val init = awaitState(fix.vm) { it.chats.size == 3 }
                    assertEquals(listOf("101", "103", "102"), init.chats.map { it.id })

                    fix.vm.togglePinChat("102")
                    awaitCondition(description = "pin 102 recorded") {
                        server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/102/pin" }
                    }
                    fix.vm.togglePinChat("101")
                    val afterToggle = awaitState(fix.vm) {
                        it.chats.firstOrNull { c -> c.id == "102" }?.isPinned == true &&
                            it.chats.firstOrNull { c -> c.id == "101" }?.isPinned == false &&
                            server.getChatSnapshot(101L)?.isPinned == false
                    }
                    assertEquals(listOf("102", "103", "101"), afterToggle.chats.map { it.id })
                }

                // Cold restart -> verify pin state & sorting survive without reverting
                val restartedApp = rootApp.coldRestartApp()
                E2eViewModelFixture(
                    app = restartedApp,
                    preseedEndpoint = server.baseUrl,
                    expectAuthenticated = true,
                    deleteRootDirOnClose = false,
                ).use { fix2 ->
                    val rehydrated = awaitState(fix2.vm) { it.chats.size == 3 }
                    assertEquals(listOf("102", "103", "101"), rehydrated.chats.map { it.id })
                    assertTrue(rehydrated.chats.first { it.id == "102" }.isPinned)
                    assertFalse(rehydrated.chats.first { it.id == "101" }.isPinned)
                }
            } finally {
                rootApp.cleanup()
            }
        }
    }

    @Test
    fun e2e_t1_b2_messagePinListUnpinAndWsPinnedMessagesChanged() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Pin Room", lastMessageDateMs = 3000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Important notice", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "501" } == true }

                fix.vm.togglePinMessage("101", "501")
                val pinned = awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds == listOf("501") }
                assertTrue(pinned.messagesByChat["101"]!!.first { it.id == "501" }.pinned)
                awaitCondition(description = "POST /pins") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/pins" }
                }

                fix.vm.fetchPinnedMessages("101", 101L)
                awaitCondition(description = "GET /pins") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats/101/pins" }
                }

                fix.vm.togglePinMessage("101", "501")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds.isEmpty() }
                awaitCondition(description = "DELETE /pins/501") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/chats/101/pins/501" }
                }

                // WS PinnedMessagesChanged pushes [501] then []
                server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessagesChanged","data":{"chat_id":101,"pinned_ids":[501]}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds == listOf("501") }
                server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessagesChanged","data":{"chat_id":101,"pinned_ids":[]}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds.isEmpty() }
            }
        }
    }

    @Test
    fun e2e_t1_b3_chatLifecycleCreateMuteArchiveReadTtlClearAndLeave() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Primary", unreadCount = 4, lastMessageDateMs = 3000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Unread msg", dateMs = 3000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.chats.any { c -> c.id == "101" } }

                fix.vm.createNewGroupOrChannel(ConversationKind.GROUP, "Design Group", "iOS polish")
                awaitCondition(description = "POST /api/v1/chats/create") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/create" }
                }

                fix.vm.toggleMuteChat("101")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.isMuted }
                awaitCondition(description = "POST /mute") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/mute" }
                }

                fix.vm.toggleArchiveChat("101")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.isArchived }
                awaitCondition(description = "POST /archive") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/archive" }
                }

                fix.vm.markChatRead("101", 501L)
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.unreadCount == 0 }
                awaitCondition(description = "POST /read") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/read" }
                }

                fix.vm.setChatTtl("101", 3600)
                awaitCondition(description = "POST /ttl") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/ttl" }
                }

                fix.vm.clearChatHistory("101")
                awaitCondition(description = "POST /clear") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/clear" }
                }

                fix.vm.leaveChat("101")
                awaitState(fix.vm) { it.chats.none { c -> c.id == "101" } }
                awaitCondition(description = "POST /leave") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/leave" }
                }
            }
        }
    }

    @Test
    fun e2e_t1_b4_cloudDraftsFoldersContactsAndPtsSyncDifferenceRoundTrip() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Work Chat", lastMessageDateMs = 3000L))
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.chats.any { c -> c.id == "101" } }

                // Drafts: PUT -> GET -> DELETE
                fix.vm.saveServerDraft("101", "Draft reply text")
                awaitCondition(description = "PUT /draft") {
                    server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/chats/101/draft" }
                }
                fix.vm.fetchServerDraft("101")
                awaitCondition(description = "GET /draft") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats/101/draft" }
                }
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.draftText == "Draft reply text" }

                fix.vm.clearServerDraft("101")
                awaitCondition(description = "DELETE /draft") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/chats/101/draft" }
                }

                // Folders: POST -> GET -> DELETE
                fix.vm.upsertCustomFolder("Engineering", includeContacts = false, includeBots = true, includeGroups = true, includeChannels = false, excludeMuted = true)
                val withFolder = awaitState(fix.vm) { it.folderTabs.any { f -> f.title == "Engineering" } }
                val folderId = withFolder.folderTabs.first { it.title == "Engineering" }.id
                awaitCondition(description = "POST /folders") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/folders" }
                }

                fix.vm.refreshFolders()
                awaitCondition(description = "GET /folders") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/folders" }
                }
                fix.vm.deleteCustomFolder(folderId)
                awaitState(fix.vm) { it.folderTabs.none { f -> f.id == folderId } }
                awaitCondition(description = "DELETE /folders/$folderId") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/folders/$folderId" }
                }

                // Contacts: POST /api/v1/contacts/add -> GET -> DELETE
                fix.vm.addContactByUsername("bob")
                awaitState(fix.vm) { it.serverContacts.any { c -> c.username == "bob" } }
                awaitCondition(description = "POST /contacts/add") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/contacts/add" }
                }

                val bobId = fix.vm.state.value.serverContacts.first { it.username == "bob" }.userId.toString()
                fix.vm.deleteContact(bobId)
                awaitState(fix.vm) { it.serverContacts.none { c -> c.userId.toString() == bobId } }
                awaitCondition(description = "DELETE /contacts/$bobId") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/contacts/$bobId" }
                }

                // PTS syncDifference via JNI hits POST /api/v1/sync/difference
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val sync = host.request("telegram.server.syncDifference", JSONObject().put("pts", 1))
                    assertTrue(sync.optLong("current_pts") >= 10L)
                }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/sync/difference" })
            }
        }
    }

    @Test
    fun e2e_t1_b5_sendReplyEditReactDeleteAndPollLifecycle() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Dev Chat", lastMessageDateMs = 3000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Original question", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                val opened = awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "501" } == true }
                val baseMsg = opened.messagesByChat["101"]!!.first { it.id == "501" }

                // Send with reply_to + client_nonce -> sent
                fix.vm.setReplyTarget(baseMsg)
                fix.vm.sendComposerMessage("101", "My real reply")
                val sentState = awaitState(fix.vm) {
                    it.messagesByChat["101"]?.any { m -> m.text == "My real reply" && m.deliveryState == "sent" } == true
                }
                val sentMsg = sentState.messagesByChat["101"]!!.first { it.text == "My real reply" }
                assertEquals("501", sentMsg.replyHeader?.replyToMessageId)
                val sendReq = server.recordedRequests.last { it.method == "POST" && it.path == "/api/v1/messages/send" }
                assertEquals(501L, sendReq.bodyJsonOrNull()?.optLong("reply_to"))
                assertFalse(sendReq.bodyJsonOrNull()?.optString("client_nonce").isNullOrBlank())

                // WS ReadHistory(is_outbox=true) flips sent -> read
                server.pushWsEvent(
                    """{"type":"Update","payload":{"type":"ReadHistory","data":{"chat_id":101,"max_id":${sentMsg.id},"is_outbox":true,"unread_count":0}}}""",
                )
                awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == sentMsg.id }?.deliveryState == "read" }

                // Edit message via setEditTarget + sendComposerMessage -> PUT
                fix.vm.setEditTarget(fix.vm.state.value.messagesByChat["101"]!!.first { it.id == sentMsg.id })
                fix.vm.sendComposerMessage("101", "My edited reply")
                awaitState(fix.vm) {
                    it.messagesByChat["101"]?.firstOrNull { m -> m.id == sentMsg.id }?.let { m -> m.text == "My edited reply" && m.edited } == true
                }
                awaitCondition(description = "PUT /messages/${sentMsg.id}") {
                    server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/chats/101/messages/${sentMsg.id}" }
                }

                // React -> POST .../reactions
                fix.vm.toggleReaction("101", sentMsg.id, "🔥")
                awaitState(fix.vm) {
                    it.messagesByChat["101"]?.firstOrNull { m -> m.id == sentMsg.id }?.reactions?.any { r -> r.reaction == "🔥" && r.chosenByMe } == true
                }
                awaitCondition(description = "POST /reactions") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/101/messages/${sentMsg.id}/reactions" }
                }

                // Create poll & vote
                fix.vm.sendPollOrQuiz("101", "Release target?", listOf("v2.3.0", "v2.4.0"), isAnonymous = true, multipleAnswers = false, isQuiz = false, correctIndex = 0, explanation = null)
                val withPoll = awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.poll?.question == "Release target?" && !m.pollId.isNullOrBlank() } == true }
                val pollMsg = withPoll.messagesByChat["101"]!!.first { it.poll?.question == "Release target?" }
                fix.vm.votePoll("101", pollMsg.id, pollMsg.poll!!.options.first().id)
                awaitCondition(description = "POST /polls/{id}/vote") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/polls/${pollMsg.pollId}/vote" }
                }

                // Delete message -> DELETE
                fix.vm.deleteMessage("101", sentMsg.id, forEveryone = true)
                awaitState(fix.vm) { it.messagesByChat["101"]?.none { m -> m.id == sentMsg.id } == true }
                awaitCondition(description = "DELETE /messages/${sentMsg.id}") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/chats/101/messages/${sentMsg.id}" }
                }
            }
        }
    }

    @Test
    fun e2e_t1_b6_paginatedHistoryAndMediaUploadDownloadRoundTrip() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Media Room", lastMessageDateMs = 50_000L))
            for (i in 1..60) {
                server.seedMessage(
                    MockGpuiTelegramServer.MockMessage(
                        id = 100L + i,
                        chatId = 101L,
                        senderId = 2L,
                        senderName = "Bob",
                        text = "Page item #$i",
                        dateMs = 1_000L * i,
                    ),
                )
            }
            val payloadBytes = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4, 5)

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                val page1 = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 50 }
                assertEquals("111", page1.messagesByChat["101"]!!.first().id)
                assertTrue(page1.chats.first { it.id == "101" }.hasMoreHistory)

                // Load older page via ?before_id=111&limit=50 -> total 60 unique messages
                fix.vm.loadOlderMessages("101", limit = 50)
                val page2 = awaitState(fix.vm) { it.messagesByChat["101"]?.size == 60 }
                assertEquals("101", page2.messagesByChat["101"]!!.first().id)
                assertEquals(60, page2.messagesByChat["101"]!!.map { it.id }.distinct().size)
                assertFalse(page2.chats.first { it.id == "101" }.hasMoreHistory)
                assertTrue(
                    server.recordedRequests.any {
                        it.method == "GET" && it.path == "/api/v1/chats/101/messages" && it.rawQuery.contains("before_id=111")
                    },
                )

                // Upload media via ViewModel -> download exact bytes via fresh JNI call
                fix.vm.sendMediaAttachment("101", "lotus.png", "image/png", payloadBytes)
                val uploadedState = awaitState(fix.vm) {
                    it.messagesByChat["101"]?.any { m -> m.mediaFileName == "lotus.png" && m.deliveryState == "sent" && m.mediaBlobId?.startsWith("med_") == true } == true
                }
                val fileId = uploadedState.messagesByChat["101"]!!.first { it.mediaFileName == "lotus.png" }.mediaBlobId!!
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/media/upload" })

                // Download from server via a fresh ViewModel (cold cache, hits GET /api/v1/media/{fileId})
                val dlResult = AtomicReference<Result<ByteArray>>()
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val dl = host.request("telegram.server.downloadMedia", JSONObject().put("fileId", fileId))
                    val raw = java.util.Base64.getDecoder().decode(dl.getString("dataBase64"))
                    dlResult.set(Result.success(raw))
                }
                assertArrayEquals(payloadBytes, dlResult.get().getOrThrow())
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/media/$fileId" })
            }
        }
    }

    @Test
    fun e2e_t1_b7_websocketAllUpdateEventsAndExponentialBackoffReconnect() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Live WS Chat", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 1L, "Alice", "Sent text", dateMs = 2000L))

            E2eViewModelFixture(wsBaseBackoffMs = 150L).use { fix ->
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.size == 1 && it.serverWsConnected }

                // 1. NewMessage
                server.pushWsEvent("""{"type":"Update","payload":{"type":"NewMessage","data":{"id":502,"chat_id":101,"sender_id":2,"sender_name":"Bob","text":"Incoming live","date_ms":3000}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "502" && m.text == "Incoming live" } == true }

                // 2. EditMessage
                server.pushWsEvent("""{"type":"Update","payload":{"type":"EditMessage","data":{"chat_id":101,"id":502,"text":"Edited live"}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == "502" }?.text == "Edited live" }

                // 3. ReadHistory
                server.pushWsEvent("""{"type":"Update","payload":{"type":"ReadHistory","data":{"chat_id":101,"max_id":501,"is_outbox":true,"unread_count":0}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == "501" }?.deliveryState == "read" }

                // 4. MessageReactions
                server.pushWsEvent("""{"type":"Update","payload":{"type":"MessageReactions","data":{"chat_id":101,"message_id":502,"reactions":[{"emoji":"❤️","count":3,"is_chosen_by_me":false}]}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == "502" }?.reactions?.any { r -> r.reaction == "❤️" && r.count == 3 } == true }

                // 5. PinnedMessagesChanged
                server.pushWsEvent("""{"type":"Update","payload":{"type":"PinnedMessagesChanged","data":{"chat_id":101,"pinned_ids":[502]}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.pinnedMessageIds == listOf("502") }

                // 6. DraftUpdated
                server.pushWsEvent("""{"type":"Update","payload":{"type":"DraftUpdated","data":{"chat_id":101,"draft":{"text":"Synced cloud draft"}}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.draftText == "Synced cloud draft" }

                // 7. FolderChanged
                server.pushWsEvent("""{"type":"Update","payload":{"type":"FolderChanged","data":{"folder":{"id":77,"title":"Live Folder","include_bots":true}}}}""")
                awaitState(fix.vm) { it.folderTabs.any { f -> f.id == "77" && f.title == "Live Folder" } }

                // 8. PollUpdated
                server.pushWsEvent("""{"type":"Update","payload":{"type":"NewMessage","data":{"id":503,"chat_id":101,"sender_id":2,"sender_name":"Bob","text":"Poll","content_type":"poll","date_ms":4000,"poll":{"id":"p-ws","question":"Coffee?","options":[{"id":"opt-1","text":"Yes","voter_count":0},{"id":"opt-2","text":"No","voter_count":0}]}}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.any { m -> m.id == "503" } == true }
                server.pushWsEvent("""{"type":"Update","payload":{"type":"PollUpdated","data":{"chat_id":101,"message_id":503,"poll":{"id":"p-ws","question":"Coffee?","options":[{"id":"opt-1","text":"Yes","voter_count":5,"chosen":true},{"id":"opt-2","text":"No","voter_count":1,"chosen":false}]}}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.firstOrNull { m -> m.id == "503" }?.poll?.options?.first()?.voterCount == 5 }

                // 9. UserTyping
                server.pushWsEvent("""{"type":"Update","payload":{"type":"UserTyping","data":{"chat_id":101,"user_id":2,"action":"typing"}}}""")
                awaitState(fix.vm) { it.chats.first { c -> c.id == "101" }.typingLabel?.contains("正在输入") == true }

                // 10. DeleteMessages
                server.pushWsEvent("""{"type":"Update","payload":{"type":"DeleteMessages","data":{"chat_id":101,"message_ids":[502]}}}""")
                awaitState(fix.vm) { it.messagesByChat["101"]?.none { m -> m.id == "502" } == true }

                // 11. Forced TCP drop triggers exponential-backoff reconnect (wsConnectCount 1 -> 2+)
                val before = server.wsConnectCount.get()
                assertTrue(before >= 1)
                server.dropAllWebSockets()
                awaitCondition(timeoutMs = 5_000L, description = "WS automatic reconnect") {
                    server.wsConnectCount.get() > before
                }
                awaitState(fix.vm) { it.serverWsConnected }
            }
        }
    }

    // =========================================================================
    // Area C: Real Bots, Fabushi AI Bots & Coding Agents (6 Kotlin Tests: C1..C6)
    // =========================================================================

    @Test
    fun e2e_t1_c1_listMyBotsAndListFabushiBotsHydrateRealServerBotsOnly() {
        MockGpuiTelegramServer().use { server ->
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9001L,
                    ownerUserId = 1L,
                    agentId = "ag-coder",
                    username = "coder_bot",
                    displayName = "Coder Bot",
                    botToken = "srv_tok_9001",
                    executionState = "idle",
                    avatarShape = "hexagon",
                    avatarColor = "#34C759",
                    companionChatId = 301L,
                    isFabushi = true,
                ),
            )
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9002L,
                    ownerUserId = 1L,
                    agentId = "bot-classic",
                    username = "classic_bot",
                    displayName = "Classic Bot",
                    botToken = "srv_tok_9002",
                    executionState = "idle",
                    avatarShape = "circle",
                    avatarColor = "#007AFF",
                    companionChatId = 302L,
                    isFabushi = false,
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.refreshServerBots()
                val hydrated = awaitState(fix.vm) { it.fabushiBots.size == 2 }
                val coder = hydrated.fabushiBots.first { it.botUserId == 9001L }
                assertEquals("coder_bot", coder.username)
                assertEquals("idle", coder.executionState)
                assertEquals("hexagon", coder.avatarShape)
                assertEquals("#34C759", coder.avatarColor)
                assertEquals(301L, coder.companionChatId)
                assertTrue(hydrated.fabushiBots.any { it.botUserId == 9002L && it.username == "classic_bot" })
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/bots/my" })
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/fabushi-bots" })
            }
        }
    }

    @Test
    fun e2e_t1_c2_createBotCreateFabushiBotAndBotfatherNlCreateUseServerTokens() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)

                val grantsBefore = server.recordedRequests.count { it.path == "/api/v1/mcp/grants" }
                fix.vm.createServerBot("Ops Bot", "ops_helper_bot", "Operations")
                val s1 = awaitState(fix.vm) { it.fabushiBots.any { b -> b.username == "ops_helper_bot" } }
                assertTrue(s1.lastCreatedBotToken.orEmpty().startsWith("srv_bot_"))
                assertFalse(s1.lastCreatedBotToken.orEmpty().startsWith("tg_bot_"))
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/bots" })
                // createServerBot triggers a trailing refreshAllFromHost() — wait for it to finish before next mutation
                awaitCondition(description = "createServerBot trailing refreshAllFromHost") {
                    server.recordedRequests.count { it.path == "/api/v1/mcp/grants" } > grantsBefore
                }
                awaitState(fix.vm) { it.syncStatusLabel != "同步中…" }

                fix.vm.createFabushiBot("Zen AI", "zen_ai_bot", "Meditation guide", avatarShape = "star", avatarColor = "#AF52DE")
                val s2 = awaitState(fix.vm) { it.fabushiBots.any { b -> b.username == "zen_ai_bot" } }
                assertTrue(s2.lastCreatedBotToken.orEmpty().startsWith("srv_fbot_"))
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/fabushi-bots" })

                fix.vm.botfatherNlCreate("Create a release engineering assistant bot", avatarShape = "diamond", avatarColor = "#FF9500")
                val s3 = awaitState(fix.vm) { it.fabushiBots.any { b -> b.username.startsWith("nl_") } }
                assertTrue(s3.lastCreatedBotToken.orEmpty().startsWith("srv_nlbot_"))
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/botfather/nl-create" })
            }
        }
    }

    @Test
    fun e2e_t1_c3_patchFabushiBotSendsHttpPatchAndPersistsOnRefetch() {
        MockGpuiTelegramServer().use { server ->
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9010L,
                    ownerUserId = 1L,
                    agentId = "ag-9010",
                    username = "muse_bot",
                    displayName = "Muse Bot",
                    botToken = "srv_tok_9010",
                    avatarShape = "circle",
                    avatarColor = "#007AFF",
                    companionChatId = 310L,
                ),
            )
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.fabushiBots.any { b -> b.botUserId == 9010L } }

                fix.vm.patchFabushiBot(
                    botId = "9010",
                    avatarShape = "diamond",
                    avatarColor = "#FF9500",
                    preferredModel = "gpt-5.4",
                    approvalMode = "auto",
                )
                awaitState(fix.vm) {
                    val b = it.fabushiBots.firstOrNull { bot -> bot.botUserId == 9010L }
                    b?.avatarShape == "diamond" && b.avatarColor == "#FF9500" && b.preferredModel == "gpt-5.4"
                }
                assertTrue(server.recordedRequests.any { it.method == "PATCH" && it.path == "/api/v1/fabushi-bots/9010" })

                // Re-fetch from server and verify persisted values
                fix.vm.refreshServerBots()
                awaitState(fix.vm) {
                    val b = it.fabushiBots.firstOrNull { bot -> bot.botUserId == 9010L }
                    b?.avatarShape == "diamond" && b.approvalMode == "auto"
                }
            }
        }
    }

    @Test
    fun e2e_t1_c4_getBotProfileAndSendCallbackQueryApplyPeerBotActionResult() {
        MockGpuiTelegramServer().use { server ->
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9020L,
                    ownerUserId = 1L,
                    agentId = "ag-9020",
                    username = "action_bot",
                    displayName = "Action Bot",
                    aiPersona = "Owner secret persona",
                    botToken = "srv_tok_9020",
                    companionChatId = 320L,
                ),
            )
            server.seedChat(MockGpuiTelegramServer.MockChat(320L, 1L, "Action Bot", isBot = true, peerUserId = 9020L, lastMessageDateMs = 2000L))
            server.seedMessage(
                MockGpuiTelegramServer.MockMessage(
                    id = 801L,
                    chatId = 320L,
                    senderId = 9020L,
                    senderName = "Action Bot",
                    text = "Confirm deployment?",
                    dateMs = 2000L,
                    inlineKeyboard = JSONArray().put(
                        JSONArray().put(JSONObject().put("text", "Deploy Now").put("callback_data", "deploy:prod")),
                    ),
                ),
            )
            server.callbackActionToast = "Deployment triggered"
            server.callbackUpdatedText = "Deployed to prod"
            server.callbackUpdatedKeyboard = JSONArray().put(
                JSONArray().put(JSONObject().put("text", "View Logs").put("callback_data", "logs:prod")),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.getBotProfile("action_bot")
                val withProfile = awaitState(fix.vm) { !it.lastBotProfileJson.isNullOrBlank() }
                assertTrue(withProfile.lastBotProfileJson!!.contains("Owner secret persona"))
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/bots/profile/action_bot" })

                fix.vm.openChat("320")
                val chatState = awaitState(fix.vm) { it.messagesByChat["320"]?.any { m -> m.id == "801" } == true }
                val btn = chatState.messagesByChat["320"]!!.first { it.id == "801" }.inlineKeyboard.first().buttons.first()
                fix.vm.clickInlineButton("320", btn, messageId = "801")

                val updated = awaitState(fix.vm) {
                    it.messagesByChat["320"]?.firstOrNull { m -> m.id == "801" }?.text == "Deployed to prod"
                }
                val msg = updated.messagesByChat["320"]!!.first { it.id == "801" }
                assertEquals("View Logs", msg.inlineKeyboard.first().buttons.first().text)
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/chats/320/callback" })
            }
        }
    }

    @Test
    fun e2e_t1_c5_probeCodingAgentsAndGetSetBotComputerBinding() {
        MockGpuiTelegramServer().use { server ->
            server.codingAgentProbes = JSONArray()
                .put(JSONObject().put("provider_kind", "claude_code").put("installed", false))
                .put(JSONObject().put("provider_kind", "codex").put("installed", true).put("version", "0.42.0").put("executable_path", "/usr/bin/codex"))
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9030L,
                    ownerUserId = 1L,
                    agentId = "ag-9030",
                    username = "dev_bot",
                    displayName = "Dev Bot",
                    botToken = "srv_tok_9030",
                    companionChatId = 330L,
                ),
            )

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.probeCodingAgents()
                val probed = awaitState(fix.vm) { it.codingAgentProbes.size == 2 }
                assertFalse(probed.codingAgentProbes.first { it.providerKind == CodingProviderKindUi.CLAUDE_CODE }.installed)
                assertTrue(probed.codingAgentProbes.first { it.providerKind == CodingProviderKindUi.CODEX }.installed)
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/coding-agents/probe" })

                fix.vm.setBotComputer("9030", "cloud:1:prod-1")
                awaitState(fix.vm) { it.botComputerBindings["9030"] == "cloud:1:prod-1" }
                assertTrue(server.recordedRequests.any { it.method == "PUT" && it.path == "/api/v1/bots/9030/computer" })

                fix.vm.getBotComputer("9030")
                awaitCondition(description = "GET /bots/9030/computer") {
                    server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/bots/9030/computer" }
                }
                assertEquals("cloud:1:prod-1", fix.vm.state.value.botComputerBindings["9030"])
            }
        }
    }

    @Test
    fun e2e_t1_c6_liveWsBotActivityChangedTransitionsThinkingToRunningToolToIdle() {
        MockGpuiTelegramServer().use { server ->
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9040L,
                    ownerUserId = 1L,
                    agentId = "ag-9040",
                    username = "stream_bot",
                    displayName = "Stream Bot",
                    botToken = "srv_tok_9040",
                    companionChatId = 340L,
                ),
            )
            server.seedChat(MockGpuiTelegramServer.MockChat(340L, 1L, "Stream Bot", isBot = true, peerUserId = 9040L, lastMessageDateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.chats.any { c -> c.id == "340" } && it.serverWsConnected }

                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":340,"bot_user_id":9040,"state":"thinking","detail":"Analyzing repo"}}}""")
                val thinking = awaitState(fix.vm) { it.chats.first { c -> c.id == "340" }.botExecutionState == "thinking" }
                assertTrue(thinking.chats.first { it.id == "340" }.typingLabel.orEmpty().contains("Analyzing repo"))

                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":340,"bot_user_id":9040,"state":"running_tool","detail":"shell_exec"}}}""")
                val running = awaitState(fix.vm) { it.chats.first { c -> c.id == "340" }.botExecutionState == "running_tool" }
                assertTrue(running.chats.first { it.id == "340" }.typingLabel.orEmpty().contains("shell_exec"))

                server.pushWsEvent("""{"type":"Update","payload":{"type":"BotActivityChanged","data":{"chat_id":340,"bot_user_id":9040,"state":"idle"}}}""")
                val idle = awaitState(fix.vm) { it.chats.first { c -> c.id == "340" }.botExecutionState == "idle" }
                assertTrue(idle.chats.first { it.id == "340" }.typingLabel.isNullOrBlank())
            }
        }
    }

    // =========================================================================
    // Area D: Real Remote Control, Assist, MCP Grants & Mika (7 Kotlin Tests: D1..D7)
    // =========================================================================

    @Test
    fun e2e_t1_d1_computersCrudPolicyTestAndNoSecretEcho() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.upsertCloudComputer(
                    displayName = "Build Server",
                    host = "10.0.0.42",
                    port = 22,
                    username = "ubuntu",
                    authMethod = "ssh_password",
                    secret = "super_secret_ssh_pw",
                )
                val created = awaitState(fix.vm) { it.cloudComputer.nodes.any { n -> n.name == "Build Server" } }
                val node = created.cloudComputer.nodes.first { it.name == "Build Server" }
                assertTrue(node.hasSavedCredential)
                assertEquals("ask_every_time", node.runPolicy)
                assertFalse(created.cloudComputer.toString().contains("super_secret_ssh_pw"))

                fix.vm.patchComputerPolicy(node.id, runPolicy = "auto")
                awaitState(fix.vm) { it.cloudComputer.nodes.first { n -> n.id == node.id }.runPolicy == "auto" }
                assertTrue(server.recordedRequests.any { it.method == "PATCH" && it.path == "/api/v1/computers/${node.id}/policy" })

                fix.vm.testComputer(node.id)
                awaitState(fix.vm) { it.cloudComputer.nodes.first { n -> n.id == node.id }.online }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/computers/${node.id}/test" })

                fix.vm.deleteComputer(node.id)
                awaitState(fix.vm) { it.cloudComputer.nodes.none { n -> n.id == node.id } }
                awaitCondition(description = "DELETE /computers/${node.id}") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/computers/${node.id}" }
                }
            }
        }
    }

    @Test
    fun e2e_t1_d2_execComputerAndLocalRegisterPollGetApproveComplete() {
        MockGpuiTelegramServer().use { server ->
            server.seedComputer(
                MockGpuiTelegramServer.MockComputer(
                    nodeId = "cloud:1:exec-1",
                    ownerUserId = 1L,
                    displayName = "Exec Box",
                    host = "10.0.0.50",
                    online = true,
                    runPolicy = "auto",
                ),
            )
            server.queueLocalCommand(JSONObject().put("command_id", "lcmd-1").put("op", "capture_screen"))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                awaitState(fix.vm) { it.cloudComputer.nodes.any { n -> n.id == "cloud:1:exec-1" } }

                fix.vm.execComputer("cloud:1:exec-1", JSONObject().put("op", "shell_exec").put("command", "uname -a"))
                val afterExec = awaitState(fix.vm) { it.cloudComputer.lastOpOutput.contains("server-exec-stdout: uname -a") }
                assertFalse(afterExec.cloudComputer.lastOpOutput.contains("执行完成 (0)"))
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/computers/exec" })

                // Local computer register (POST), poll (GET!), approve, complete via real JNI
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    host.request(
                        "telegram.server.registerLocalComputer",
                        JSONObject().put("deviceId", "android-e2e-1").put("displayName", "Pixel E2E"),
                    )
                    val polled = host.request("telegram.server.pollLocalCommands", JSONObject().put("deviceId", "android-e2e-1"))
                    assertEquals(1, polled.getJSONArray("commands").length())
                    host.request("telegram.server.approveComputerCommand", JSONObject().put("commandId", "lcmd-1").put("approved", true))
                    host.request("telegram.server.completeComputerCommand", JSONObject().put("commandId", "lcmd-1").put("ok", true).put("stdout", "done"))
                }
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/computers/local/register" })
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/computers/local/poll" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/computers/commands/lcmd-1/approve" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/computers/commands/lcmd-1/complete" })
            }
        }
    }

    @Test
    fun e2e_t1_d3_remoteAssistInfoAllowResetCodeSetCodeAndStart() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                fix.vm.refreshRemoteAssist()
                awaitState(fix.vm) { it.cloudComputer.assistConfig.assistId == "987654321" }

                fix.vm.toggleRemoteAssist(true)
                awaitState(fix.vm) { it.cloudComputer.assistConfig.allowAssist }

                val codeBeforeReset = fix.vm.state.value.cloudComputer.assistConfig.currentCode
                fix.vm.resetRemoteAssistCode()
                awaitState(fix.vm) { it.cloudComputer.assistConfig.currentCode != codeBeforeReset }

                fix.vm.updateRemoteAssistCode("Abc12345")
                awaitState(fix.vm) { it.cloudComputer.assistConfig.currentCode == "Abc12345" }

                fix.vm.startRemoteAssist("987654321", "Abc12345", mode = "desktop")
                awaitState(fix.vm) { it.cloudComputer.remoteSessions.isNotEmpty() }

                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/remote/assist/info" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/assist/allow" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/assist/reset-code" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/assist/set-code" })
                assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/assist/start" })
            }
        }
    }

    @Test
    fun e2e_t1_d4_remoteSessionsDisconnectAndMcpGrantsRevokeAndOauthFlowRequired() {
        MockGpuiTelegramServer().use { server ->
            server.seedRemoteSession(MockGpuiTelegramServer.MockRemoteSession("rs-1", "cloud:1:a"))
            server.seedRemoteSession(MockGpuiTelegramServer.MockRemoteSession("rs-2", "cloud:1:b"))
            server.seedMcpGrant(MockGpuiTelegramServer.MockMcpGrant("fam-1", "Claude Desktop", listOf("remote.read"), listOf("cloud:1:a")))

            E2eViewModelFixture().use { fix ->
                fix.loginAndConnect(server)
                val sessReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/remote/sessions" }
                val grantReqsBefore = server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/mcp/grants" }
                fix.vm.refreshRemoteSessions()
                fix.vm.refreshMcpGrants()
                awaitCondition(description = "explicit refreshRemoteSessions & refreshMcpGrants HTTP") {
                    server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/remote/sessions" } > sessReqsBefore &&
                        server.recordedRequests.count { it.method == "GET" && it.path == "/api/v1/mcp/grants" } > grantReqsBefore
                }
                awaitState(fix.vm) { it.cloudComputer.remoteSessions.size == 2 && it.cloudComputer.mcpGrants.size == 1 }

                fix.vm.disconnectRemoteSession("rs-1")
                awaitState(fix.vm) { it.cloudComputer.remoteSessions.size == 1 && it.cloudComputer.remoteSessions.none { s -> s.sessionId == "rs-1" } }
                awaitCondition(description = "POST /remote/sessions/disconnect") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/sessions/disconnect" }
                }

                fix.vm.disconnectAllRemoteSessions()
                awaitState(fix.vm) { it.cloudComputer.remoteSessions.isEmpty() }
                awaitCondition(description = "POST /remote/sessions/disconnect-all") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/remote/sessions/disconnect-all" }
                }

                // Local fake MCP grant minting MUST fail closed with OAUTH_FLOW_REQUIRED
                fix.vm.clearToastBanner()
                fix.vm.issueRemoteMcpGrant("Fake Local Client")
                awaitState(fix.vm) { it.toastBanner?.contains("OAUTH_FLOW_REQUIRED") == true }
                assertEquals(1, fix.vm.state.value.cloudComputer.mcpGrants.size)

                fix.vm.revokeRemoteMcpGrant("fam-1")
                awaitState(fix.vm) { it.cloudComputer.mcpGrants.isEmpty() }
                awaitCondition(description = "DELETE /mcp/grants/fam-1") {
                    server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/mcp/grants/fam-1" }
                }
            }
        }
    }

    @Test
    fun e2e_t1_d5_telegramMikaChannelConfirmationGateAndLocalNodeHonestSnapshot(): Unit = runBlocking {
        val channel = TelegramMikaChannel()
        val owners = MemoryOwnerIdentity()
        val ownerPeer = ChannelPeer(TelegramMikaChannel.ID, "user:1", "chat-101")
        owners.bind(ownerPeer)

        var activeAsk: Delivery.Ask? = null
        val decisions = CopyOnWriteArrayList<Pair<String, ConfirmChoice>>()
        val gateway = ChannelGateway(
            owners = owners,
            activeAsk = { tid -> activeAsk?.takeIf { it.threadId == tid } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean = true
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                    decisions.add(askId to choice)
                    return true
                }
                override suspend fun cancelAsk(threadId: String) { activeAsk = null }
                override suspend fun stop(runId: RunId) = Unit
            },
        )
        val reg = gateway.start(channel)
        try {
            val ask1 = Delivery.Ask("d1", "chat-101", "ask-1", Delivery.Ask.Kind.Confirm, "Approve ADB tap?")
            activeAsk = ask1
            assertEquals(SendOutcome.Delivered, channel.send(ownerPeer, ask1))
            val card = channel.deliveredEvents().last()
            assertEquals(MikaChannelCardEvent.Kind.ASK_CONFIRM, card.kind)
            assertEquals(2, card.inlineButtons.size)
            assertTrue(card.inlineButtons[0].payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_APPROVE_PREFIX))
            assertTrue(card.inlineButtons[1].payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_DECLINE_PREFIX))

            assertTrue(channel.resolveConfirmationForThread("chat-101", approved = true, askId = "ask-1"))
            assertEquals("ask-1" to ConfirmChoice.Once, decisions.last())

            val ask2 = Delivery.Ask("d2", "chat-101", "ask-2", Delivery.Ask.Kind.Confirm, "Approve risky action?")
            activeAsk = ask2
            channel.send(ownerPeer, ask2)
            assertTrue(channel.resolveConfirmationForThread("chat-101", approved = false, askId = "ask-2"))
            assertEquals("ask-2" to ConfirmChoice.Deny, decisions.last())
        } finally {
            reg.close()
            channel.close()
        }

        // Verify local-android-mika reflects honest available=false -> online=false when MikaApp is absent
        E2eViewModelFixture().use { fix ->
            val localNode = fix.vm.state.value.cloudComputer.nodes.firstOrNull { it.id == "local-android-mika" }
            assertFalse(fix.vm.state.value.mikaSnapshot.available)
            if (localNode != null) {
                assertFalse(localNode.online)
            }
        }
    }

    @Test
    fun e2e_t1_d6_fabushiAppAgentSurfaceAndGatewayInvokeCanonicalActions(): Unit = runBlocking {
        E2eViewModelFixture().use { fix ->
            fix.vm.openSavedMessages()
            val state = awaitState(fix.vm) { it.activeChat != null }

            val surface = FabushiAppAgentSurface()
            publishTelegramSemanticSurface(surface, state, fix.vm)

            val gateway = FabushiRemoteDeviceGateway(
                context = fix.app,
                surface = surface,
                metadata = mapOf("suite" to "e2e"),
                gatewayUrl = "ws://127.0.0.1:1/agent",
                retryDelayMs = 100L,
            )
            try {
                val descriptors = gateway.toolDescriptors()
                assertTrue((0 until descriptors.length()).any { descriptors.getJSONObject(it).getString("name") == FabushiAppAgentSurface.ActionTool })

                val snap = gateway.invokeTool(FabushiAppAgentSurface.SnapshotTool, JSONObject())
                val gen = snap.getLong("generation")
                val composerId = surface.snapshot().elements.first { it.agentId.startsWith("telegram.chat.composer:") }.agentId

                gateway.invokeTool(
                    FabushiAppAgentSurface.ActionTool,
                    JSONObject().put("generation", gen).put("agentId", composerId).put("action", "setValue").put("value", "MCP draft text"),
                )
                assertEquals("MCP draft text", fix.vm.state.value.activeChat?.draftText)

                val gen2 = surface.snapshot().generation
                gateway.invokeTool(
                    FabushiAppAgentSurface.ActionTool,
                    JSONObject().put("generation", gen2).put("agentId", "telegram.nav.settings").put("action", "invoke"),
                )
                assertEquals(TelegramSubScreen.SETTINGS, fix.vm.state.value.subScreen)
            } finally {
                gateway.close()
            }
        }
    }

    @Test
    fun e2e_t1_d7_remoteDeviceGatewayBindsToTelegramAuthViaBridgedSession() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture().use { fix ->
                val surface = FabushiAppAgentSurface()
                val gateway = FabushiRemoteDeviceGateway(
                    context = fix.app,
                    surface = surface,
                    metadata = mapOf("suite" to "e2e_t1_d7"),
                    gatewayUrl = "${server.wsBaseUrl}/agent",
                    retryDelayMs = 100L,
                )
                try {
                    assertFalse(gateway.isRegistered())
                    fix.loginAndConnect(server, "alice", "password123")
                    assertTrue(fix.vm.state.value.isAuthenticated)

                    // Activate gateway after Telegram auth -> bridges >=24-char token & registers on /agent WS
                    gateway.setLoggedIn(fix.vm.state.value.isAuthenticated)
                    awaitCondition(timeoutMs = 5_000L, description = "gateway registered via bridged deviceAgentSession") {
                        gateway.isRegistered() && server.agentRegisterFrames.isNotEmpty()
                    }

                    // Sign out -> deactivates gateway & tears down cleanly
                    fix.vm.signOutToAuthWizard()
                    awaitState(fix.vm) { !it.isAuthenticated }
                    gateway.setLoggedIn(fix.vm.state.value.isAuthenticated)
                    assertFalse(gateway.isRegistered())
                } finally {
                    gateway.close()
                }
            }
        }
    }

    // =========================================================================
    // Area E: All 19 JNI Contracts & Rust Host Bugfixes (7 Kotlin Tests: E1..E7)
    // =========================================================================

    @Test
    fun e2e_t1_e1_defaultEndpointAliasesAndFlushOutboxV1Path() {
        MockGpuiTelegramServer().use { server ->
            val app = TestFabushiApplication()
            try {
                MahayanaHost(app, featureHostTest = false).use { host ->
                    val status0 = host.request("telegram.server.status")
                    assertEquals(DEFAULT_DESKTOP_SERVER_URL, status0.getString("endpoint"))

                    host.request("telegram.server.setEndpoint", JSONObject().put("endpoint", server.baseUrl))
                    assertEquals(server.baseUrl, host.request("telegram.server.status").getString("endpoint"))

                    host.request("telegram.server.configureEndpoint", JSONObject().put("endpoint", server.baseUrl))
                    host.request(
                        "telegram.server.login",
                        JSONObject().put("endpoint", server.baseUrl).put("username", "alice").put("password", "pw"),
                    )
                    host.request(
                        "telegram.server.enqueueOutbox",
                        JSONObject().put("payload", JSONObject().put("chat_id", 101).put("text", "Queued outbox msg")),
                    )
                    val flushed = host.request("telegram.server.flushOutbox")
                    assertEquals(1, flushed.getInt("flushedCount"))
                    assertTrue(server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/messages/send" })
                    assertTrue(server.recordedRequests.none { it.path == "/v1/messages/send" })
                }
            } finally {
                app.cleanup()
            }
        }
    }

    @Test
    fun e2e_t1_e2_channelStatsZeroSqliteCrashAndNoHardcoded12840() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(201L, 1L, "Announcements", kind = "channel", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(1L, 201L, 1L, "Alice", "Post 1", dateMs = 1000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(2L, 201L, 1L, "Alice", "Post 2", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                // 1. Direct local SQLite channelStats JNI contract (zero "no such column" crash)
                MahayanaHost(fix.app, featureHostTest = false).use { host ->
                    val localStats = host.request("telegram.messaging.channelStats", JSONObject().put("conversationId", "conv-missing"))
                    assertEquals(0, localStats.getInt("subscribers"))
                    assertEquals(0, localStats.getInt("totalMessages"))
                }

                // 2. Authenticated server channelStats via ViewModel (never hardcoded 12,840!)
                fix.loginAndConnect(server)
                fix.vm.openChannelStats("201")
                val withStats = awaitState(fix.vm) { it.channelStatsSnapshot?.conversationId == "201" }
                val snap = withStats.channelStatsSnapshot!!
                assertEquals(42, snap.subscribers)
                assertEquals(2, snap.avgViewsPerPost)
                assertFalse(snap.subscribers == 12840)
                assertTrue(server.recordedRequests.any { it.method == "GET" && it.path == "/api/v1/chats/201/statistics" })
            }
        }
    }

    @Test
    fun e2e_t1_e3_magpieUpsertProviderImportUriImportEnvAndColdRestart() {
        val rootApp = TestFabushiApplication()
        try {
            MahayanaHost(rootApp, featureHostTest = false).use { host ->
                val up = host.request(
                    "magpie.upsertProvider",
                    JSONObject().put("presetId", "openai").put("apiKey", "sk-real-123").put("enabled", true),
                )
                assertEquals("openai", up.getString("presetId"))
                assertTrue(up.getBoolean("configuredForLive"))

                val list = host.request("magpie.providers").getJSONArray("providers")
                val configured = (0 until list.length()).map { list.getJSONObject(it) }.filter { it.optBoolean("configuredForLive") }
                assertEquals(listOf("openai"), configured.map { it.getString("presetId") })

                val uriRes = host.request("magpie.importUri", JSONObject().put("uri", "magpie://import?preset=deepseek&key=sk-ds-999"))
                val importedProvider = uriRes.getJSONObject("provider")
                assertEquals("deepseek", importedProvider.getString("presetId"))
                assertTrue("magpie.importUri must configure deepseek for live use", importedProvider.getBoolean("configuredForLive"))

                val envRes = host.request("magpie.importEnv", JSONObject().put("envText", "ANTHROPIC_API_KEY=sk-ant-777"))
                assertTrue(envRes.getInt("importedCount") >= 1)
            }

            // Cold restart on SAME SQLite directory -> openai, deepseek, and anthropic ALL persist configuredForLive && hasApiKey while all other 45 presets remain unconfigured
            val restarted = rootApp.coldRestartApp()
            val expectedConfigured = setOf("openai", "deepseek", "anthropic")
            MahayanaHost(restarted, featureHostTest = false).use { host2 ->
                val resp2 = host2.request("magpie.providers")
                assertTrue(resp2.getBoolean("liveReady"))
                assertTrue("Expected presetCount >= 48", resp2.getInt("presetCount") >= 48)
                val providers = resp2.getJSONArray("providers")
                val storedRows = (0 until providers.length()).map { providers.getJSONObject(it) }

                for (presetId in expectedConfigured) {
                    val preset = storedRows.first { it.getString("presetId") == presetId }
                    assertTrue("Preset '$presetId' must remain configuredForLive after cold restart", preset.getBoolean("configuredForLive"))
                    assertTrue("Preset '$presetId' must remain hasApiKey after cold restart", preset.getBoolean("hasApiKey"))
                }
                for (preset in storedRows.filterNot { it.getString("presetId") in expectedConfigured }) {
                    val pid = preset.getString("presetId")
                    assertFalse("Unconfigured preset '$pid' must stay configuredForLive == false", preset.getBoolean("configuredForLive"))
                    assertFalse("Unconfigured preset '$pid' must stay hasApiKey == false", preset.getBoolean("hasApiKey"))
                }
            }

            E2eViewModelFixture(app = restarted, deleteRootDirOnClose = false).use { fix2 ->
                val rehydrated = awaitState(fix2.vm) {
                    it.magpieGateway.presets.count { p -> p.configured } == 3
                }
                assertEquals(48, rehydrated.magpieGateway.presets.size)
                assertEquals(
                    expectedConfigured,
                    rehydrated.magpieGateway.presets.filter { it.configured }.map { it.id }.toSet(),
                )
                val unconfiguredUi = rehydrated.magpieGateway.presets.filterNot { it.id in expectedConfigured }
                assertEquals(45, unconfiguredUi.size)
                assertTrue(unconfiguredUi.all { !it.configured })
            }
        } finally {
            rootApp.cleanup()
        }
    }

    @Test
    fun e2e_t1_e4_walletCreateStatusAndSendTonNanotonsContract() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                val created = host.request("wallet.createWallet", JSONObject().put("label", "E2E Wallet").put("initialNanotons", 5_000_000_000L))
                val addr = created.getString("address")
                assertTrue(addr.isNotBlank())
                assertFalse("Must be real Ed25519 v4r2 address, never fake prefix", addr.startsWith("UQBk9x_mahayana_v4r2_"))
                assertEquals("v4r2", created.getString("walletVersion"))

                val status = host.request("wallet.status")
                assertTrue(status.getBoolean("hasWallet"))
                assertEquals(addr, status.getString("address"))
                assertEquals("5.0000", status.getString("balanceTon"))

                // Send TON using canonical amountNanotons parameter
                val sent = host.request(
                    "wallet.sendTon",
                    JSONObject()
                        .put("to", addr)
                        .put("amountNanotons", 1_000_000_000L)
                        .put("comment", "E2E transfer")
                        .put("broadcast", false),
                )
                assertEquals(1_000_000_000L, sent.getLong("amountNanotons"))
                assertTrue(sent.getString("bocBase64").isNotBlank())
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t1_e5_passcodeArgon2idVerifyAndZeroPlaintextInSharedPreferences() {
        E2eViewModelFixture().use { fix ->
            fix.vm.configurePasscode("2468")
            awaitState(fix.vm) { it.passcodeConfigured }

            assertTrue(fix.vm.verifyPasscode("2468"))
            assertFalse(fix.vm.verifyPasscode("0000"))

            // Audit EVERY SharedPreferences value -> zero plaintext "2468" or legacy "passcode_pin" key
            val allPrefs = fix.app.telegramPrefs().all
            assertFalse(allPrefs.containsKey("passcode_pin"))
            assertTrue(allPrefs.values.none { it.toString().contains("2468") })
        }
    }

    @Test
    fun e2e_t1_e6_hermesAndFabushiMiniappCanonicalJniContracts() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                val mem = host.request("hermes.memoryUpdate", JSONObject().put("target", "SOUL.md").put("content", "# Pure E2E Soul"))
                assertEquals("SOUL.md", mem.getString("target"))
                assertTrue(mem.getInt("charCount") > 0)

                val slash = host.request("hermes.runSlashCommand", JSONObject().put("input", "/help"))
                assertEquals("/help", slash.getString("command"))

                // Approvals decide rejects unknown approvalId cleanly
                val badApproval = host.requestResult("hermes.approvalsDecide", JSONObject().put("approvalId", "missing-id").put("decision", "denied"))
                assertTrue(badApproval.isFailure)

                // fabushiMiniapp canonical contracts
                val boot = host.request("fabushiMiniapp.bootstrap")
                assertTrue(boot.getJSONObject("profile").has("total_transfer_count"))

                val wheel = host.request("fabushiMiniapp.spinPrayerWheel", JSONObject().put("rotations", 7))
                assertEquals(7, wheel.getInt("rotationsAdded"))

                val practice = host.request("fabushiMiniapp.recordPractice", JSONObject().put("durationSeconds", 180).put("chantCount", 108))
                assertEquals(180, practice.getInt("durationSeconds"))

                // aiDharmaChat fails closed with PROVIDER_NOT_CONFIGURED when 0 Magpie keys exist
                val aiRes = host.requestResult("fabushiMiniapp.aiDharmaChat", JSONObject().put("prompt", "什么是正念？"))
                assertTrue(aiRes.isFailure)
                assertTrue(aiRes.exceptionOrNull()?.message.orEmpty().contains("PROVIDER_NOT_CONFIGURED"))
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t1_e7_e2eeAccountSwitchTopicMikaTaskEventAndCallsContracts() {
        val app = TestFabushiApplication()
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                // 1. E2EE createSecretChat & sendEncrypted
                val sc = host.request("telegram.e2ee.createSecretChat", JSONObject().put("peerId", "peer-42"))
                val chatId = sc.getString("chatId")
                val enc = host.request("telegram.e2ee.sendEncrypted", JSONObject().put("chatId", chatId).put("plaintext", "Confidential E2EE"))
                assertTrue(enc.getBoolean("verifiedRoundtrip"))
                assertTrue(enc.getString("ciphertextBase64").isNotBlank())

                // 2. Account switch generation fence
                val acc1 = host.request("telegram.auth.accountAdd", JSONObject().put("displayName", "Acc 1").put("username", "acc1"))
                val accId = acc1.getString("accountId")
                val list0 = host.request("telegram.auth.accountsList")
                val gen0 = list0.getLong("generation")
                val stale = host.requestResult("telegram.auth.accountSwitch", JSONObject().put("accountId", accId).put("expectedGeneration", gen0 + 99))
                assertTrue(stale.isFailure)
                val sw = host.request("telegram.auth.accountSwitch", JSONObject().put("accountId", accId).put("expectedGeneration", gen0))
                assertEquals(accId, sw.getString("activeAccountId"))
                assertEquals(gen0 + 1L, sw.getLong("generation"))

                // 3. Forum topic createTopic with iconEmoji
                val topic = host.request(
                    "telegram.messaging.createTopic",
                    JSONObject().put("conversationId", "group-1").put("title", "Releases").put("iconEmoji", "🚀"),
                )
                assertEquals("🚀", topic.getString("iconEmoji"))

                // 4. Mika task event registration & history
                val reg = host.request(
                    "mika.registerTaskEvent",
                    JSONObject().put("taskId", "t-1").put("stepIndex", 1).put("phase", "started").put("summary", "Launch settings"),
                )
                assertTrue(reg.getBoolean("recorded"))
                val hist = host.request("mika.taskHistory")
                assertTrue(hist.getJSONArray("events").length() >= 1)

                // 5. Calls: startCall/endCall fail closed with CALL_TRANSPORT_UNAVAILABLE; vadFeedPcm processes real PCM
                val startErr = host.requestResult("telegram.calls.startCall", JSONObject().put("conversationId", "101"))
                assertTrue(startErr.isFailure && startErr.exceptionOrNull()?.message.orEmpty().contains("CALL_TRANSPORT_UNAVAILABLE"))
                val endErr = host.requestResult("telegram.calls.endCall", JSONObject().put("callId", "c1"))
                assertTrue(endErr.isFailure && endErr.exceptionOrNull()?.message.orEmpty().contains("CALL_TRANSPORT_UNAVAILABLE"))

                val samples = JSONArray()
                repeat(320) { i -> samples.put(if (i % 2 == 0) 6000 else -6000) }
                val vad = host.request("telegram.calls.vadFeedPcm", JSONObject().put("pcmSamples", samples).put("ttsPlaying", false))
                assertTrue(vad.getInt("framesEvaluated") >= 1)
            }
        } finally {
            app.cleanup()
        }
    }

    // =========================================================================
    // Area F: Apple iOS Design System, Contrast & Navigation (6 Kotlin Tests: F1..F6)
    // =========================================================================

    @Test
    fun e2e_t1_f1_resolveTelegramPaletteExactDesktopV169HexTokensAndZeroDim() {
        val defaults = TelegramThemeSettings()
        assertEquals(0, defaults.wallpaperDimPercent)

        val day = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_DAY))
        assertEquals(Color(0xFFF5F5F5), day.groupedBackground)
        assertEquals(Color(0xFFFCFCFC), day.topBarBackground)
        assertEquals(Color(0xFFFFFFFF), day.appBackground)
        assertEquals(Color(0xFF126FD4), day.primaryAccent)
        assertEquals(Color(0xFF1E86FC), day.outgoingBubbleTop)
        assertEquals(Color(0xFF126FD4), day.outgoingBubbleBottom)
        assertEquals(Color(0xFFFFFFFF), day.outgoingText)
        assertEquals(Color(0xFFF1F1F1), day.incomingBubble)
        assertEquals(Color(0xFF1C1C1C), day.incomingText)
        assertEquals(Color(0xFF636366), day.secondaryText)
        assertEquals(Color(0xFFE72E3B), day.destructiveRed)
        assertEquals(Color(0xFF10AB61), day.secretGreen)

        val night = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_NIGHT))
        assertEquals(Color(0xFF0E0E0E), night.groupedBackground)
        assertEquals(Color(0xFF121212), night.topBarBackground)
        assertEquals(Color(0xFF161616), night.appBackground)
        assertEquals(Color(0xFF0A84FF), night.primaryAccent)
        assertEquals(Color(0xFF1884FC), night.outgoingBubbleTop)
        assertEquals(Color(0xFF116DD2), night.outgoingBubbleBottom)
        assertEquals(Color(0xFF1F1F1F), night.incomingBubble)
        assertEquals(Color(0xFFF5F5F5), night.incomingText)
        assertEquals(Color(0xFFA3A3A8), night.secondaryText)

        val tinted = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_TINTED))
        assertEquals(Color(0xFF181C22), tinted.appBackground)
        assertEquals(Color(0xFF0A84FF), tinted.primaryAccent)
        assertEquals(Color(0xFF242A33), tinted.incomingBubble)
        assertEquals(Color(0xFFF5F7FA), tinted.incomingText)
        assertEquals(Color(0xFF949DA8), tinted.secondaryText)
    }

    @Test
    fun e2e_t1_f2_wcag21LuminanceContrastRatiosAcrossAllThreeAppleThemes() {
        val presets = listOf(TelegramThemePreset.APPLE_DAY, TelegramThemePreset.APPLE_NIGHT, TelegramThemePreset.APPLE_TINTED)
        for (preset in presets) {
            val p = resolveTelegramPalette(TelegramThemeSettings(preset = preset))
            assertTrue("$preset primaryText/bg", contrastRatio(p.primaryText, p.appBackground) >= 4.5)
            assertTrue("$preset topBarContent/topBarBg", contrastRatio(p.topBarContent, p.topBarBackground) >= 4.5)
            assertTrue("$preset incomingText/incomingBubble", contrastRatio(p.incomingText, p.incomingBubble) >= 4.5)

            for ((name, fg) in listOf(
                "outgoingText" to p.outgoingText,
                "outgoingLinkColor" to p.outgoingLinkColor,
                "outgoingCheckMarkColor" to p.outgoingCheckMarkColor,
                "outgoingReplyBarColor" to p.outgoingReplyBarColor,
                "outgoingReactionTextColor" to p.outgoingReactionTextColor,
            )) {
                assertTrue("$preset $name/bottom", contrastRatio(fg, p.outgoingBubbleBottom) >= 3.0)
                assertTrue("$preset $name/top", contrastRatio(fg, p.outgoingBubbleTop) >= 3.0)
            }
            assertTrue("$preset outgoingMeta/bottom", contrastRatio(p.outgoingMeta, p.outgoingBubbleBottom) >= 2.95)
            assertTrue("$preset secondaryText/bg", contrastRatio(p.secondaryText, p.appBackground) >= 3.0)
        }
    }

    @Test
    fun e2e_t1_f3_freshDefaultsAndLiveLanguageSwitchZhCnToEnUs() {
        val defaults = TelegramThemeSettings()
        assertEquals(TelegramThemePreset.APPLE_DAY, defaults.preset)
        assertEquals(TelegramWallpaperStyle.SOLID_MINIMAL, defaults.wallpaperStyle)
        assertEquals(18, defaults.bubbleCornerRadiusDp)

        E2eViewModelFixture().use { fix ->
            assertEquals("zh-CN", fix.vm.state.value.uiLanguage)
            val zh = resolveUiStrings(fix.vm.state.value.uiLanguage)
            assertEquals("信息", zh.tabMessages)
            assertEquals("设置", zh.tabSettings)
            assertEquals("退出登录", zh.signOut)

            fix.vm.updateLanguageAndTranslate("en-US", showTranslateButton = true, autoTranslate = false)
            awaitState(fix.vm) { it.uiLanguage == "en-US" }
            val en = resolveUiStrings(fix.vm.state.value.uiLanguage)
            assertEquals("Messages", en.tabMessages)
            assertEquals("Settings", en.tabSettings)
            assertEquals("Sign Out", en.signOut)
            assertFalse(en.composerPlaceholder == zh.composerPlaceholder)
        }
    }

    @Test
    fun e2e_t1_f4_appleShellTabFourTabsAndPinnedChatsGridHelper() {
        assertEquals(
            listOf(AppleShellTab.MESSAGES, AppleShellTab.CONTACTS, AppleShellTab.AGENTS, AppleShellTab.SETTINGS),
            AppleShellTab.entries,
        )
        fun chat(id: String, pinned: Boolean) = TelegramChatItem(
            id = id, title = "C$id", subtitle = "", preview = "", time = "10:00",
            kind = ConversationKind.DIRECT, isPinned = pinned,
        )
        val two = partitionPinnedChatsGrid(listOf(chat("1", true), chat("2", true), chat("3", false)))
        assertTrue(two.centeredSingleRow)
        assertEquals(listOf("1", "2"), two.rows.single().map { it.id })

        val four = partitionPinnedChatsGrid((1..4).map { chat("$it", true) })
        assertFalse(four.centeredSingleRow)
        assertEquals(listOf(3, 1), four.rows.map { it.size })
    }

    @Test
    fun e2e_t1_f5_timelineDateDividersCodeSegmentsAndCanSubmitComposer() {
        val zone = ZoneOffset.UTC
        val d1 = ZonedDateTime.of(2026, 11, 20, 9, 0, 0, 0, zone).toInstant().toEpochMilli()
        val d2 = ZonedDateTime.of(2026, 11, 21, 9, 0, 0, 0, zone).toInstant().toEpochMilli()
        val items = buildTimelineItems(
            listOf(
                TelegramMessageItem("m1", "c1", "u1", "A", "", contentType = "photo", mediaFileName = "a.jpg", outgoing = false, time = "09:00", timestampMs = d1),
                TelegramMessageItem("m2", "c1", "u2", "B", "Next day", outgoing = true, time = "09:00", timestampMs = d2),
            ),
            uiLanguage = "en-US",
            zoneId = zone,
        )
        assertEquals(2, items.count { it is ChatTimelineItem.DateDivider })
        assertTrue(items.filterIsInstance<ChatTimelineItem.MessageEntry>().any { it.message.id == "m1" && it.message.text == "" })

        val segs = parseMessageCodeSegments("Before\n```kotlin\nval a = 1\n```\nAfter")
        assertEquals(1, segs.filterIsInstance<MessageContentSegment.CodeBlockSegment>().size)
        val textJoined = segs.filterIsInstance<MessageContentSegment.TextSegment>().joinToString(" ") { it.text }
        assertFalse(textJoined.contains("val a = 1"))

        assertFalse(canSubmitComposer(""))
        assertFalse(canSubmitComposer("   "))
        assertTrue(canSubmitComposer("Hi"))
        assertTrue(canSubmitComposer("", attachmentCount = 1))
    }

    @Test
    fun e2e_t1_f6_honestSettingsRealCacheBytesAlipayUrlAndVoipTranslateGuards() {
        MockGpuiTelegramServer().use { server ->
            server.seedChat(MockGpuiTelegramServer.MockChat(101L, 1L, "Chat 101", lastMessageDateMs = 2000L))
            server.seedMessage(MockGpuiTelegramServer.MockMessage(501L, 101L, 2L, "Bob", "Hello world", dateMs = 2000L))

            E2eViewModelFixture().use { fix ->
                // 1. Real cacheDir bytes measurement & clearing (never hardcoded 0.4 MB)
                val cacheFile = File(fix.app.cacheDir, "media_sample.bin")
                cacheFile.writeBytes(ByteArray(2 * 1024 * 1024) { 7 })
                val measured = fix.vm.measureLocalMediaCacheMb()
                assertTrue("Expected >= 1.9 MB, got $measured", measured >= 1.9)
                fix.vm.clearLocalMediaCache()
                awaitState(fix.vm) { it.storageCacheMb < 0.01 }
                assertFalse(cacheFile.exists())

                // 2. Clickable Alipay URL surfaced from server
                fix.vm.updateDesktopServerEndpoint(server.baseUrl)
                awaitState(fix.vm) { it.serverEndpoint == server.baseUrl }
                fix.vm.startAlipayLogin()
                val alipay = awaitState(fix.vm) { !it.alipayLoginUrl.isNullOrBlank() }
                assertTrue(alipay.alipayLoginUrl!!.startsWith("https://auth.alipay.com/"))

                // 3. Sign in -> Sign Out works cleanly
                fix.loginAndConnect(server)
                fix.vm.openChat("101")
                awaitState(fix.vm) { it.messagesByChat["101"]?.size == 1 }

                // 4. Unsupported VoIP fails honestly with CALL_TRANSPORT_UNAVAILABLE (never fake connected)
                fix.vm.clearToastBanner()
                fix.vm.startCall("101", "Chat 101", video = false)
                val afterCall = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                assertEquals(CallPhase.IDLE, afterCall.callState.phase)

                // 5. Translate fails honestly when Magpie unconfigured (never prepends "【即时中文译文】")
                fix.vm.clearToastBanner()
                fix.vm.translateMessage("101", "501")
                val afterTranslate = awaitState(fix.vm) { !it.toastBanner.isNullOrBlank() }
                val msg = afterTranslate.messagesByChat["101"]!!.first { it.id == "501" }
                assertFalse(msg.translatedText.orEmpty().contains("【即时中文译文】"))

                // 6. Sign Out from Settings returns cleanly to AuthWizard & clears chats
                fix.vm.signOutToAuthWizard()
                val signedOut = awaitState(fix.vm) { !it.isAuthenticated }
                assertFalse(signedOut.isAuthenticated)
                assertTrue(signedOut.chats.isEmpty())
            }
        }
    }

    // =========================================================================
    // Area G: MiniApp Security & Capability Contract (AND-GD-01..05) (5 Kotlin Tests: G1..G5)
    // =========================================================================

    @Test
    fun e2e_t1_g1_miniAppCapabilityDomainsAndAnswerAll11CanonicalPrefixes() {
        val tools = listOf(
            "offline.capabilities", "selection.pick", "transfer.send", "scan.meta",
            "state.get", "config.get", "platform.info", "hotspot.start",
            "update.check", "diagnostics.dump", "compat.probe", "private.secret", "shell.exec",
        ).map { MiniAppToolContract(it, it, "none") }

        val domains = miniAppCapabilityDomains(tools)
        assertEquals(
            listOf("compat", "config", "diagnostics", "hotspot", "offline", "platform", "scan", "selection", "state", "transfer", "update"),
            domains,
        )
        val answer = miniAppCapabilityAnswer(tools)
        assertEquals("fabushi.miniapp.tools.v1", answer.getString("contract"))
        assertEquals("android", answer.getString("platform"))
        assertEquals(11, answer.getJSONArray("capabilities").length())
    }

    @Test
    fun e2e_t1_g2_miniAppAccountAndEventProjectionsStripAllSecrets() {
        val projectedAcc = miniAppAccountProjection(
            JSONObject()
                .put("loggedIn", true)
                .put("token", "tok-secret")
                .put("accessToken", "acc-secret")
                .put(
                    "user",
                    JSONObject()
                        .put("id", "u1")
                        .put("username", "alice")
                        .put("password", "pw")
                        .put("privateKey", "priv")
                        .put("localPath", "/data/secret")
                        .put("sessionId", "s1")
                        .put("nestedCreds", JSONObject().put("key", "nope")),
                ),
        )
        assertFalse(projectedAcc.has("token"))
        assertFalse(projectedAcc.has("accessToken"))
        val user = projectedAcc.getJSONObject("user")
        assertEquals("alice", user.getString("username"))
        assertFalse(user.has("password"))
        assertFalse(user.has("privateKey"))
        assertFalse(user.has("localPath"))
        assertFalse(user.has("sessionId"))
        assertFalse(user.has("nestedCreds"))

        val projectedEv = miniAppEventProjection(
            JSONObject()
                .put("type", "transfer.progress")
                .put("progress", 80)
                .put("token", "leak")
                .put("privateKey", "leak")
                .put("nested", JSONObject().put("secret", "leak")),
        )
        assertEquals("transfer.progress", projectedEv.getString("type"))
        assertFalse(projectedEv.has("token"))
        assertFalse(projectedEv.has("privateKey"))
        assertFalse(projectedEv.has("nested"))
    }

    @Test
    fun e2e_t1_g3_toolContractJsonPropagatesSchemaAndReadOnlyHint() {
        val plugin = MarketplacePlugin(
            pluginId = "global-dharma",
            displayName = "Global Dharma",
            description = "Test",
            latestVersion = "2.3.0",
            tools = listOf(
                MiniAppToolContract("state.read", "Read state", "none", title = "Read State", inputSchemaJson = """{"type":"object","properties":{"k":{"type":"string"}}}"""),
                MiniAppToolContract("transfer.send", "Send data", "confirm", title = "Send Data", inputSchemaJson = """{"type":"object"}"""),
            ),
        )
        val arr = JSONArray(toolContractJson(plugin))
        assertEquals(3, arr.length())
        val readTool = arr.getJSONObject(1)
        assertEquals("state.read", readTool.getString("name"))
        assertEquals("Read State", readTool.getString("title"))
        assertTrue(readTool.getJSONObject("annotations").getBoolean("readOnlyHint"))
        assertTrue(readTool.getJSONObject("inputSchema").has("properties"))

        val mutTool = arr.getJSONObject(2)
        assertEquals("transfer.send", mutTool.getString("name"))
        assertFalse(mutTool.getJSONObject("annotations").getBoolean("readOnlyHint"))
    }

    @Test
    fun e2e_t1_g4_miniAppNativeBridgeWithInjectableApprovalPromptAndJniContract() {
        val app = TestFabushiApplication()
        try {
            val plugin = MarketplacePlugin(
                pluginId = "global-dharma",
                displayName = "Global Dharma",
                description = "Test",
                latestVersion = "2.3.0",
                tools = listOf(
                    MiniAppToolContract("state.get", "Read state", "none"),
                    MiniAppToolContract("transfer.send", "Send", "confirm"),
                ),
            )
            var active = true
            var approved = false
            val bridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { active },
                callRuntimeToolJson = { _, _, _ -> """{"executed":true}""" },
                approvalPrompt = { approved },
            )

            // Undeclared tool rejected
            assertFalse(JSONObject(bridge.callTool("shell.exec", "{}")).getBoolean("ok"))

            // Unapproved mutation rejected without blocking on Looper
            approved = false
            assertFalse(JSONObject(bridge.callTool("transfer.send", "{}")).getBoolean("ok"))

            // Non-local origin rejected
            active = false
            assertFalse(JSONObject(bridge.callTool("state.get", "{}")).getBoolean("ok"))

            // Approved on local origin succeeds
            active = true
            approved = true
            assertTrue(JSONObject(bridge.callTool("transfer.send", "{}")).getBoolean("ok"))

            // Real JNI miniapp contract verification
            MahayanaHost(app, featureHostTest = false).use { host ->
                val boot = host.request("fabushiMiniapp.bootstrap")
                assertNotNull(boot.optJSONObject("profile"))
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun e2e_t1_g5_miniAppLifecycleGateRevokesOnPauseStopAndDispose() {
        val gate = MiniAppLifecycleGate()
        assertFalse(gate.localDocumentActive)
        gate.onLocalHtmlLoaded()
        assertTrue(gate.localDocumentActive)

        gate.onLifecycleEvent(Lifecycle.Event.ON_PAUSE)
        assertFalse(gate.localDocumentActive)
        var delivered = 0
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "ping")) { delivered++ })
        assertEquals(0, delivered)

        gate.onLifecycleEvent(Lifecycle.Event.ON_RESUME)
        assertTrue(gate.localDocumentActive)

        gate.onLifecycleEvent(Lifecycle.Event.ON_STOP)
        assertFalse(gate.localDocumentActive)

        gate.onDispose()
        assertTrue(gate.disposed)
        gate.onLifecycleEvent(Lifecycle.Event.ON_RESUME)
        assertFalse(gate.localDocumentActive)
        assertFalse(gate.dispatchHostEvent(JSONObject().put("type", "late")) { delivered++ })
        assertEquals(0, delivered)
    }
}
