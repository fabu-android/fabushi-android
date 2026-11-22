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
        assertEquals(Color(0xFF007AFF), day.primaryAccent)
        assertEquals(Color(0xFF2C95FF), day.outgoingBubbleTop)
        assertEquals(Color(0xFF007AFF), day.outgoingBubbleBottom)
        assertEquals(Color(0xFFFFFFFF), day.outgoingText)
        assertEquals(Color(0xFFE9E9EB), day.incomingBubble)

        val night = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_NIGHT))
        assertTrue(night.isDark)
        assertTrue(night.isAppleStyle)
        assertEquals(Color(0xFF0A84FF), night.primaryAccent)
        assertEquals(Color(0xFF1E8FFF), night.outgoingBubbleTop)
        assertEquals(Color(0xFF0A84FF), night.outgoingBubbleBottom)
        assertEquals(Color(0xFF262629), night.incomingBubble)

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
        assertEquals(19, CloudComputerUiState().actionBusCategoryCount)
    }

    @Test
    fun jniHostExercisesDesktopV162VaultCodingRemoteMagpieAndWebMcpEndToEnd() {
        val (context, dir) = createTempContext("jni_v162")
        try {
            MahayanaHost(context, featureHostTest = true).use { host ->
                // 1. feature.info reports desktop v1.6.2 parity
                val info = host.request("feature.info")
                assertEquals("1.6.2", info.getString("desktopParityVersion"))
                assertEquals("9542524", info.getString("desktopParityCommit"))

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

                // 4. UU-Remote Assist, Stream Settings & OAuth 2.1 Remote MCP Grants
                val assist = host.request("remote.getAssistConfig")
                assertEquals(9, assist.getString("assist_id").length)
                val cats = host.request("remote.actionBusCategories")
                assertEquals(19, cats.getInt("total"))
                val grant = host.request(
                    "remote.issueMcpGrant",
                    JSONObject().put("clientName", "Antigravity MCP"),
                )
                assertNotEquals("", grant.getString("family_id"))

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
            }
        } finally {
            dir.deleteRecursively()
        }
    }
}
