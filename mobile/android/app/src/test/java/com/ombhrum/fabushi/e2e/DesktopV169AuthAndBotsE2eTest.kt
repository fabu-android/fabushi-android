package com.ombhrum.fabushi.e2e

import com.ombhrum.fabushi.e2e.harness.E2eViewModelFixture
import com.ombhrum.fabushi.e2e.harness.MainDispatcherRule
import com.ombhrum.fabushi.e2e.harness.MockGpuiTelegramServer
import com.ombhrum.fabushi.e2e.harness.awaitCondition
import com.ombhrum.fabushi.e2e.harness.awaitState
import com.ombhrum.fabushi.telegram.TelegramSubScreen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * End-to-end JNI + ViewModel + stateful mock server regression tests for
 * `docs/specs/desktop-v1-6-9-auth-bots-mobile-uiux.md`:
 * - Split-Key V2 registration -> sign-out -> re-login roundtrip (`R1`, `R2`, `R3`, `R5`, `AC-1`, `AC-2`)
 * - Leading `@username` normalization & wrong-password rejection
 * - 2FA cloud password challenge (`requires_2fa` -> `/api/v1/auth/2fa/verify`) (`R4`, `AC-4`)
 * - Clicking official bots with `companionChatId == null` & `@BotFather` opens chat immediately,
 *   materializes DM via `POST /api/v1/contacts/add` (`is_bot = true`), and starts via `/start` (`R6`, `AC-5`, `AC-6`).
 */
class DesktopV169AuthAndBotsE2eTest {

    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    @Test
    fun registerSplitKeyV2_signOut_andRelogin_succeedsWithZeroPlaintextPassword() {
        MockGpuiTelegramServer().use { server ->
            E2eViewModelFixture(preseedEndpoint = server.baseUrl).use { fix ->
                // 1. Register a fresh Split-Key V2 user
                fix.vm.registerWithPassword("new_v2_user", "StrongPass!2026", "V2 Tester")
                awaitState(fix.vm, timeoutMs = 8_000L, description = "Split-Key V2 register") { snap ->
                    snap.isAuthenticated && snap.subScreen == TelegramSubScreen.HOME
                }

                // Verify wire payload sent Split-Key V2 verifier & salt with ZERO plaintext password
                val regReq = server.recordedRequests.last { it.method == "POST" && it.path == "/api/v1/auth/register" }
                val regJson = regReq.bodyJsonOrNull()
                assertNotNull(regJson)
                assertEquals(2, regJson!!.optInt("auth_version"))
                assertTrue(regJson.optString("auth_salt_b64").isNotBlank())
                assertTrue(regJson.optString("auth_verifier_b64").isNotBlank())
                assertFalse("Plaintext password must NEVER be sent in V2 register", regJson.has("password"))

                // 2. Sign out and verify clean immediate return to Auth Wizard + best-effort remote logout
                fix.vm.signOutToAuthWizard()
                awaitState(fix.vm, timeoutMs = 4_000L, description = "signOutToAuthWizard") { snap ->
                    !snap.isAuthenticated && !snap.offlineLocalMode
                }
                awaitCondition(timeoutMs = 4_000L, description = "best-effort POST /api/v1/auth/logout") {
                    server.recordedRequests.any { it.method == "POST" && it.path == "/api/v1/auth/logout" }
                }

                // 3. Re-login using leading '@' username with the SAME password -> MUST SUCCEED via kdf-params + V2 verifier
                server.clearRecordedRequests()
                fix.vm.loginWithPassword("@new_v2_user", "StrongPass!2026")
                awaitState(fix.vm, timeoutMs = 8_000L, description = "Split-Key V2 re-login") { snap ->
                    snap.isAuthenticated && snap.subScreen == TelegramSubScreen.HOME
                }

                assertTrue(
                    "Must query GET /api/v1/auth/kdf-params before V2 login",
                    server.recordedRequests.any {
                        it.method == "GET" && it.path == "/api/v1/auth/kdf-params" && it.queryParam("username") == "new_v2_user"
                    },
                )
                val loginReq = server.recordedRequests.last { it.method == "POST" && it.path == "/api/v1/auth/login" }
                val loginJson = loginReq.bodyJsonOrNull()!!
                assertEquals("new_v2_user", loginJson.optString("username"))
                assertTrue(loginJson.optString("auth_verifier_b64").isNotBlank())
                assertEquals("Plaintext password must NEVER be sent in V2 login", "", loginJson.optString("password", ""))

                // 4. Sign out and verify wrong password is rejected cleanly
                fix.vm.signOutToAuthWizard()
                awaitState(fix.vm, timeoutMs = 4_000L, description = "second signOut") { !it.isAuthenticated }

                fix.vm.loginWithPassword("new_v2_user", "WrongPassword!999")
                awaitState(fix.vm, timeoutMs = 8_000L, description = "wrong password error") { snap ->
                    !snap.isAuthenticating && !snap.isAuthenticated && !snap.authError.isNullOrBlank()
                }
            }
        }
    }

    @Test
    fun twoFactorChallenge_andVerify_completesSignIn() {
        MockGpuiTelegramServer().use { server ->
            server.seedUser(
                MockGpuiTelegramServer.MockUser(
                    userId = 77L,
                    username = "twofa_alice",
                    firstName = "TwoFA Alice",
                    password = "pw1",
                    twoFactorPassword = "cloud-secret-42",
                    twoFactorHint = "favorite number",
                ),
            )
            E2eViewModelFixture(preseedEndpoint = server.baseUrl).use { fix ->
                fix.vm.loginWithPassword("twofa_alice", "pw1")
                awaitState(fix.vm, timeoutMs = 8_000L, description = "2FA challenge presented") { snap ->
                    !snap.isAuthenticating && !snap.isAuthenticated && !snap.pendingTwoFactorChallengeToken.isNullOrBlank()
                }
                assertEquals("favorite number", fix.vm.state.value.pendingTwoFactorHint)

                // Wrong cloud password shows error and retains challenge
                fix.vm.submitTwoFactorCloudPassword("wrong-cloud")
                awaitState(fix.vm, timeoutMs = 6_000L, description = "wrong 2FA password error") { snap ->
                    !snap.isAuthenticating && !snap.isAuthenticated && !snap.authError.isNullOrBlank()
                }
                assertNotNull(fix.vm.state.value.pendingTwoFactorChallengeToken)

                // Correct cloud password completes authentication
                fix.vm.submitTwoFactorCloudPassword("cloud-secret-42")
                awaitState(fix.vm, timeoutMs = 8_000L, description = "2FA verified login") { snap ->
                    snap.isAuthenticated && snap.subScreen == TelegramSubScreen.HOME && snap.pendingTwoFactorChallengeToken == null
                }
                assertNull(fix.vm.state.value.authError)
            }
        }
    }

    @Test
    fun clickingOfficialBotWithNullCompanionChatId_andBotFather_opensImmediatelyMaterializesDmAndStarts() {
        MockGpuiTelegramServer().use { server ->
            server.seedBot(
                MockGpuiTelegramServer.MockBot(
                    botUserId = 9001L,
                    ownerUserId = 1L,
                    agentId = "official-helper",
                    username = "official_helper_bot",
                    displayName = "Official Helper",
                    description = "Helps with daily tasks",
                    botToken = "tok_official",
                    avatarColor = "purple",
                    companionChatId = null, // Official bot has NO pre-existing DM chat!
                    isFabushi = true,
                ),
            )

            E2eViewModelFixture(preseedEndpoint = server.baseUrl).use { fix ->
                fix.loginAndConnect(server, awaitWs = false)

                awaitState(fix.vm, timeoutMs = 6_000L, description = "official bot loaded") { snap ->
                    snap.fabushiBots.any { it.username == "official_helper_bot" }
                }
                val officialBot = fix.vm.state.value.fabushiBots.first { it.username == "official_helper_bot" }
                assertNull(officialBot.companionChatId)

                // 1. Clicking the bot MUST immediately open CHAT screen with non-null activeChat (zero silent no-op!)
                fix.vm.openBotChat(officialBot)
                assertEquals(TelegramSubScreen.CHAT, fix.vm.state.value.subScreen)
                assertNotNull(fix.vm.state.value.activeChat)
                assertTrue(fix.vm.state.value.activeChat!!.isBot)

                // 2. Background POST /api/v1/contacts/add (is_bot=true) materializes real numeric DM chat_id
                awaitCondition(timeoutMs = 6_000L, description = "contacts/add is_bot=true for official bot") {
                    server.recordedRequests.any { req ->
                        req.method == "POST" &&
                            req.path == "/api/v1/contacts/add" &&
                            req.bodyJsonOrNull()?.optBoolean("is_bot") == true &&
                            req.bodyJsonOrNull()?.optString("username") == "official_helper_bot"
                    }
                }
                awaitState(fix.vm, timeoutMs = 6_000L, description = "migrated to numeric companion chat id") { snap ->
                    val activeId = snap.activeChatId
                    snap.subScreen == TelegramSubScreen.CHAT && activeId != null && activeId.toLongOrNull() != null
                }

                // 3. Tapping START bar sends /start and marks chat started
                val materializedChatId = fix.vm.state.value.activeChatId!!
                fix.vm.startBotChat(materializedChatId)
                awaitState(fix.vm, timeoutMs = 6_000L, description = "/start message sent") { snap ->
                    materializedChatId in snap.startedBotChatIds &&
                        snap.messagesByChat[materializedChatId].orEmpty().any { it.text == "/start" }
                }

                // 4. Opening @BotFather directly by username also opens immediately and materializes DM
                fix.vm.openBotByUsername("@BotFather", displayName = "BotFather", description = "Create and manage bots")
                assertEquals(TelegramSubScreen.CHAT, fix.vm.state.value.subScreen)
                assertNotNull(fix.vm.state.value.activeChat)
                assertTrue(fix.vm.state.value.activeChat!!.isBot)
                awaitCondition(timeoutMs = 6_000L, description = "contacts/add is_bot=true for BotFather") {
                    server.recordedRequests.any { req ->
                        req.method == "POST" &&
                            req.path == "/api/v1/contacts/add" &&
                            req.bodyJsonOrNull()?.optBoolean("is_bot") == true &&
                            req.bodyJsonOrNull()?.optString("username")?.equals("BotFather", ignoreCase = true) == true
                    }
                }
            }
        }
    }
}
