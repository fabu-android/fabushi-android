package com.ombhrum.fabushi.telegram

import android.content.Context
import android.content.ContextWrapper
import androidx.compose.ui.graphics.Color
import com.ombhrum.fabushi.core.MahayanaHost
import java.io.File
import java.nio.file.Files
import java.util.TimeZone
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * JVM unit & JNI integration tests verifying desktop `bhrumom/gpui-telegram@9542524` (`v1.6.2`)
 * parity across Android theme tokens, local timezone formatting, UI models, and Rust JNI bridge.
 */
class DesktopV162ParityTest {

    private fun createTempContext(prefix: String): Pair<Context, File> {
        val dir = Files.createTempDirectory("fabushi_v162_${prefix}_").toFile()
        val ctx = object : ContextWrapper(null) {
            override fun getFilesDir(): File = dir
        }
        return ctx to dir
    }

    @Test
    fun appleMessagesV160ThemePresetsMatchDesktopTokensAndPreserveLegacyPresets() {
        val day = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_DAY))
        assertFalse(day.isDark)
        assertTrue(day.isAppleStyle)
        assertEquals(Color(0xFF126FD4), day.primaryAccent)
        assertEquals(Color(0xFF1E86FC), day.outgoingBubbleTop)
        assertEquals(Color(0xFF126FD4), day.outgoingBubbleBottom)
        assertEquals(Color(0xFFFFFFFF), day.outgoingText)
        assertEquals(Color(0xFFF1F1F1), day.incomingBubble)

        val night = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_NIGHT))
        assertTrue(night.isDark)
        assertTrue(night.isAppleStyle)
        assertEquals(Color(0xFF0A84FF), night.primaryAccent)
        assertEquals(Color(0xFF1884FC), night.outgoingBubbleTop)
        assertEquals(Color(0xFF116DD2), night.outgoingBubbleBottom)
        assertEquals(Color(0xFF1F1F1F), night.incomingBubble)

        val tinted = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_TINTED))
        assertTrue(tinted.isDark)
        assertTrue(tinted.isAppleStyle)
        assertEquals(Color(0xFF181C22), tinted.appBackground)

        // Legacy presets remain intact and non-Apple
        val classic = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.CLASSIC_GREEN))
        assertFalse(classic.isDark)
        assertFalse(classic.isAppleStyle)
    }

    @Test
    fun localTimezoneFormatterFormatsAccuratelyAcrossTimeZones() {
        // 2026-11-22T00:15:00Z
        val epochMs = 1795306500_000L
        assertEquals(
            "00:15",
            TelegramLocalTimeFormatter.formatLocalTime(epochMs, TimeZone.getTimeZone("UTC")),
        )
        assertEquals(
            "08:15",
            TelegramLocalTimeFormatter.formatLocalTime(epochMs, TimeZone.getTimeZone("Asia/Shanghai")),
        )
        assertEquals(
            "2026-11-22",
            TelegramLocalTimeFormatter.formatLocalDateHeader(epochMs, TimeZone.getTimeZone("Asia/Shanghai")),
        )
        assertEquals(
            "2026-11-21",
            TelegramLocalTimeFormatter.formatLocalDateHeader(epochMs, TimeZone.getTimeZone("America/Los_Angeles")),
        )
    }

    @Test
    fun codingAgentAndRemoteModelsCoverAllDesktopV162ProvidersAndDefaults() {
        assertEquals(6, CodingProviderKindUi.entries.size)
        assertTrue(CodingProviderKindUi.OPEN_CODE.supportsAcp)
        assertTrue(CodingProviderKindUi.CURSOR_CLI.supportsAcp)
        assertTrue(CodingProviderKindUi.ACP_CUSTOM.supportsAcp)
        assertEquals(3, CodingPermissionModeUi.entries.size)
        assertEquals(0, CloudComputerUiState().actionBusCategoryCount)
    }

    @Test
    fun zeroFakeDataInDefaultUiModelsAndAppleMessagesDefaultTheme() {
        val ui = TelegramUiState()
        assertFalse(ui.isAuthenticated)
        assertFalse(ui.offlineLocalMode)
        assertEquals(DEFAULT_DESKTOP_SERVER_URL, ui.serverEndpoint)
        assertTrue(ui.accounts.isEmpty())
        assertTrue(ui.stories.isEmpty())
        assertTrue(ui.chats.isEmpty())
        assertTrue(ui.messagesByChat.isEmpty())
        assertTrue(ui.callHistory.isEmpty())
        assertEquals(TelegramThemePreset.APPLE_DAY, ui.themeSettings.preset)
        assertEquals(TelegramWallpaperStyle.SOLID_MINIMAL, ui.themeSettings.wallpaperStyle)

        val wallet = TonWalletUiState()
        assertFalse(wallet.initialized)
        assertEquals("", wallet.addressFriendly)
        assertTrue(wallet.mnemonicWords.isEmpty())
        assertEquals(0.0, wallet.tonBalance, 0.0)
        assertEquals(0L, wallet.starsBalance)

        val dharma = GlobalDharmaUiState()
        assertEquals(0L, dharma.totalGlobalOfferings)
        assertEquals(0L, dharma.myMeritPoints)
        assertTrue(dharma.leaderboard.isEmpty())
        assertTrue(dharma.achievements.isEmpty())
        assertTrue(dharma.countries.all { it.offeringsCount == 0L && !it.lampLitByMe })

        val magpie = MagpieGatewayUiState()
        assertFalse(magpie.liveReady)
        assertTrue(magpie.presets.all { !it.configured && !it.healthy })
        assertEquals(0L, magpie.totalRequests)
        assertEquals(0.0, magpie.totalCostUsd, 0.0)
        assertEquals(0, magpie.piiRedactedCount)

        val hermes = HermesStudioUiState()
        assertEquals("", hermes.activeSessionId)
        assertEquals("", hermes.soulMarkdown)
        assertTrue(hermes.memoryBlocks.isEmpty())
        assertTrue(hermes.terminalLines.isEmpty())

        val mika = MikaBridgeUiSnapshot()
        assertFalse(mika.available)
        assertFalse(mika.sandboxReady)

        val assist = RemoteAssistConfigUi()
        assertEquals("", assist.assistId)
        assertEquals("", assist.currentCode)
    }

    @Test
    fun jniHostExercisesDesktopV165VaultCodingRemoteMagpieWebMcpAndServerApiEndToEnd() {
        val (context, dir) = createTempContext("jni_v165")
        try {
            MahayanaHost(context, featureHostTest = true).use { host ->
                // 1. feature.info reports desktop v1.6.6 parity
                val info = host.request("feature.info")
                assertEquals("1.6.6", info.getString("desktopParityVersion"))
                assertEquals("6ca754c", info.getString("desktopParityCommit"))

                // 2. Zero-Knowledge Split-Key V2 + X25519 Device Trust Ring atomic wipe
                val env = host.request(
                    "userVault.initEnvelope",
                    JSONObject().put("userId", "u_android").put("password", "secret-passphrase-2026"),
                )
                val rootKey = env.getString("rootKeyB64")
                assertTrue(rootKey.isNotBlank())

                val syncReq = host.request(
                    "userVault.requestDeviceSync",
                    JSONObject().put("userId", "u_android").put("deviceName", "Pixel 9"),
                )
                val reqId = syncReq.getString("requestId")
                val privKey = syncReq.getString("devicePrivateKeyB64")
                assertEquals(6, syncReq.getString("verificationCode").length)

                host.request(
                    "userVault.approveDeviceSync",
                    JSONObject().put("userId", "u_android").put("requestId", reqId).put("rootKeyB64", rootKey),
                )
                val poll1 = host.request(
                    "userVault.pollDeviceSyncStatus",
                    JSONObject()
                        .put("userId", "u_android")
                        .put("requestId", reqId)
                        .put("devicePrivateKeyB64", privKey),
                )
                assertEquals("approved", poll1.getString("status"))
                assertEquals(rootKey, poll1.getString("decryptedRootKeyB64"))

                val poll2 = host.request(
                    "userVault.pollDeviceSyncStatus",
                    JSONObject().put("userId", "u_android").put("requestId", reqId),
                )
                assertEquals("consumed", poll2.getString("status"))
                assertTrue(poll2.isNull("encryptedRootKeyB64"))

                // 3. Third-Party Coding Agent Bots
                val providers = host.request("codingAgent.listProviders")
                assertEquals(6, providers.getInt("total"))
                val bot = host.request(
                    "codingAgent.createBot",
                    JSONObject()
                        .put("providerKind", "codex")
                        .put("displayName", "Codex CLI Bot")
                        .put("username", "codex_dev")
                        .put("workspacePath", "/workspace/fabushi")
                        .put("permissionMode", "auto_edit")
                        .put("useWorktree", true),
                )
                assertEquals("codex_dev_bot", bot.getString("username"))

                // 4. UU-Remote Assist, Stream Settings & OAuth 2.1 Remote MCP Grants fail closed offline
                val assistResult = host.requestResult("remote.getAssistConfig")
                assertTrue(assistResult.isFailure)
                val cats = host.request("remote.actionBusCategories")
                assertEquals(19, cats.getInt("total"))
                val grantResult = host.requestResult(
                    "remote.issueMcpGrant",
                    JSONObject().put("clientName", "Antigravity MCP"),
                )
                assertTrue(grantResult.isFailure)
                assertTrue(grantResult.exceptionOrNull()?.message.orEmpty().contains("OAUTH_FLOW_REQUIRED"))

                // 5. Magpie Vertex schema sanitization & Gemini parallel merge
                val san = host.request(
                    "magpie.sanitizeVertexSchema",
                    JSONObject().put(
                        "schema",
                        JSONObject()
                            .put("\$schema", "https://json-schema.org/draft/2020-12/schema")
                            .put("type", "object")
                            .put(
                                "properties",
                                JSONObject().put(
                                    "name",
                                    JSONObject().put("type", JSONArray().put("string").put("null")),
                                ),
                            ),
                    ),
                )
                val sanitized = san.getJSONObject("sanitized")
                assertFalse(sanitized.has("\$schema"))
                assertEquals("string", sanitized.getJSONObject("properties").getJSONObject("name").getString("type"))
                assertTrue(sanitized.getJSONObject("properties").getJSONObject("name").getBoolean("nullable"))

                // 6. Generic WebMCP Mini App PiP state
                host.request(
                    "miniapp.webmcp.setControlMode",
                    JSONObject()
                        .put("botUsername", "fabushi_bot")
                        .put("shortName", "global_dharma")
                        .put("mode", "ai_controlling")
                        .put("pipMinimized", true),
                )
                val webmcp = host.request(
                    "miniapp.webmcp.getControlStatus",
                    JSONObject().put("botUsername", "fabushi_bot").put("shortName", "global_dharma"),
                )
                assertEquals("ai_controlling", webmcp.getString("controlMode"))
                assertTrue(webmcp.getBoolean("pipMinimized"))

                // 7. Real loopback gpui_telegram_server /api/v1/* auth & dialogs
                java.net.ServerSocket(0).use { srv ->
                    val port = srv.localPort
                    val thread = Thread {
                        repeat(4) {
                            srv.accept().use { sock ->
                                val reqLine = sock.getInputStream().bufferedReader().readLine().orEmpty()
                                val body = when {
                                    reqLine.startsWith("POST /api/v1/auth/login") ->
                                        """{"user_id":77,"username":"real_user","session_id":"s77","token":"tok_77","first_name":"Real"}"""
                                    reqLine.startsWith("GET /api/v1/chats") ->
                                        """[{"id":501,"title":"Real Server Chat","kind":"direct","last_message_text":"Hi"}]"""
                                    else -> """{"message_id":999,"ok":true}"""
                                }
                                val resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${body.toByteArray().size}\r\nConnection: close\r\n\r\n$body"
                                sock.getOutputStream().write(resp.toByteArray())
                                sock.getOutputStream().flush()
                            }
                        }
                    }
                    thread.start()
                    val login = host.request(
                        "telegram.server.login",
                        JSONObject()
                            .put("endpoint", "http://127.0.0.1:$port")
                            .put("username", "real_user")
                            .put("password", "pass123"),
                    )
                    assertEquals("tok_77", login.getString("token"))
                    val chats = host.request("telegram.server.listChats")
                    assertEquals("Real Server Chat", chats.getJSONArray("chats").getJSONObject(0).getString("title"))
                    val sent = host.request(
                        "telegram.server.sendMessage",
                        JSONObject().put("chatId", 501).put("text", "Hello over JNI"),
                    )
                    assertEquals(999, sent.getInt("message_id"))
                    thread.join(3000)
                }
            }
        } finally {
            dir.deleteRecursively()
        }
    }
}
