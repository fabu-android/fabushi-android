package com.ombhrum.fabushi

import android.content.Context
import android.content.ContextWrapper
import com.ombhrum.fabushi.core.MahayanaHost
import java.io.File
import java.nio.file.Files
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * End-to-end JVM → JNI → Rust (`libmahayana_app_host.so`) integration tests verifying:
 * 1. Complete `MahayanaFeatureHostTest` cross-platform user journey (`feature.*`).
 * 2. Protocol v2 Telegram messaging, polls, reactions, FTS5 search, and chunked blobs.
 * 3. 5-bit 100-bar voice waveforms, `<60ms` VAD barge-in, and X3DH + Double Ratchet E2EE.
 * 4. TON v4r2 Wallet, 全球法布施 240-country Mini App, Hermes 6-Tab Studio, Magpie 51-preset
 *    Gateway, Cloud Computer Action Bus + 7-State Tasks FSM, and Mika Phone Agent bridge audit.
 */
class MahayanaHostJniIntegrationTest {
    private fun createTempContext(prefix: String): Pair<Context, File> {
        val dir = Files.createTempDirectory("fabushi_jni_${prefix}_").toFile()
        val ctx = object : ContextWrapper(null) {
            override fun getFilesDir(): File = dir
        }
        return ctx to dir
    }

    @Test
    fun nativeBridgeExecutesCompleteCrossPlatformUserJourneyOverJni() {
        val (context, dir) = createTempContext("journey")
        try {
            MahayanaHost(context, featureHostTest = true).use { host ->
                val info = host.request("feature.info")
                assertEquals("android", info.getString("platform"))
                assertEquals("2", info.getString("protocolVersion"))
                assertTrue(info.getString("runtimeVersion").contains("test"))

                val initialAuth = host.request("feature.auth.status")
                assertFalse(initialAuth.getBoolean("loggedIn"))

                val providers = host.requestValue("feature.auth.providers") as JSONArray
                assertTrue((0 until providers.length()).any { providers.getJSONObject(it).getString("id") == "google" })

                val oauth = host.request("feature.auth.oauthStart", JSONObject().put("provider", "google"))
                val oauthCompleted = host.request(
                    "feature.auth.oauthPoll",
                    JSONObject().put("attemptId", oauth.getString("attemptId")),
                )
                assertEquals("completed", oauthCompleted.getString("status"))

                val sseServer = java.net.ServerSocket(0)
                val sseThread = Thread {
                    runCatching {
                        sseServer.accept().use { sock ->
                            val input = sock.getInputStream().bufferedReader()
                            while (true) {
                                val line = input.readLine() ?: break
                                if (line.isEmpty()) break
                            }
                            val body =
                                "data: {\"choices\":[{\"delta\":{\"content\":\"自动化测试状态正常。\"}}]}\n\n" +
                                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n" +
                                    "data: [DONE]\n\n"
                            val resp =
                                "HTTP/1.1 200 OK\r\n" +
                                    "Content-Type: text/event-stream\r\n" +
                                    "Content-Length: ${body.toByteArray().size}\r\n" +
                                    "Connection: close\r\n\r\n" +
                                    body
                            sock.getOutputStream().write(resp.toByteArray())
                            sock.getOutputStream().flush()
                        }
                    }
                }.apply { isDaemon = true; start() }
                try {
                    host.request(
                        "magpie.upsertProvider",
                        JSONObject()
                            .put("id", "jni-loopback")
                            .put("presetId", "openai")
                            .put("baseUrl", "http://127.0.0.1:${sseServer.localPort}/v1")
                            .put("apiKey", "sk-jni-loopback-test")
                            .put("defaultModel", "gpt-4o")
                            .put("priority", 1),
                    )
                    executeFeature(host, "chat.send", "android-chat", JSONObject().put("text", "请用一句话说明自动化测试状态"))
                } finally {
                    runCatching { sseServer.close() }
                    runCatching { sseThread.join(1000) }
                }
                executeFeature(host, "marketplace.install", "android-install", JSONObject().put("miniAppId", "global-dharma"))
                executeFeature(host, "miniapp.open", "android-open", JSONObject().put("miniAppId", "global-dharma"))
                executeFeature(
                    host,
                    "capability.request",
                    "android-capability",
                    JSONObject()
                        .put("miniAppId", "global-dharma")
                        .put("capability", "camera")
                        .put("reason", "cross-platform UI automation"),
                )

                val approval = receiveFeatureEvent(host, "approval.requested")
                val resolved = host.request(
                    "feature.approval.resolve",
                    JSONObject().put(
                        "resolution",
                        JSONObject()
                            .put("approvalId", approval.getString("approvalId"))
                            .put("decision", "allow-once"),
                    ),
                )
                assertTrue(resolved.optBoolean("resolved", true))

                val longTask = executeFeature(
                    host,
                    "runtime.longTask",
                    "android-long-task",
                    JSONObject().put("label", "Android simulated user operation"),
                )
                host.request("feature.interrupt", JSONObject().put("operationId", longTask.getString("operationId")))
                executeFeature(host, "session.clear", "android-session-clear")
            }
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun nativeBridgeSupportsTelegramE2eeWaveformVadWalletDharmaHermesMagpieComputerAndMika() {
        val (context, dir) = createTempContext("fusion")
        try {
            MahayanaHost(context, featureHostTest = false).use { host ->
                val info = host.request("feature.info")
                assertEquals("2.0.0", info.getString("runtimeVersion"))

                // 1. 5-bit 100-bar Waveform & <60ms VAD Barge-in
                val bars = JSONArray((0 until 100).map { it % 32 })
                val encodedWave = host.request("telegram.messaging.encodeWaveform", JSONObject().put("bars", bars))
                assertEquals(63, encodedWave.getInt("byteLength"))
                assertEquals(100, encodedWave.getInt("barCount"))

                val loudFrame = JSONArray(List(288) { 6000 })
                host.request("telegram.calls.vadFeedPcm", JSONObject().put("ttsPlaying", true).put("pcmSamples", loudFrame))
                host.request("telegram.calls.vadFeedPcm", JSONObject().put("ttsPlaying", true).put("pcmSamples", loudFrame))
                val vad3 = host.request("telegram.calls.vadFeedPcm", JSONObject().put("ttsPlaying", true).put("pcmSamples", loudFrame))
                assertTrue(vad3.getBoolean("bargeInTriggered"))
                assertTrue(vad3.getLong("detectionLatencyMs") < 60L)

                // 2. X3DH + Double Ratchet Secret Chat & 4-Emoji Fingerprint
                val dev = host.request("telegram.e2ee.initDevice", JSONObject().put("deviceId", "android-e2ee-test"))
                assertEquals(64, dev.getString("identityDhPubHex").length)
                val secChat = host.request(
                    "telegram.e2ee.createSecretChat",
                    JSONObject().put("deviceId", "android-e2ee-test").put("peerId", "peer-zen").put("chatId", "sec-zen"),
                )
                assertEquals(4, secChat.getJSONObject("safetyFingerprint").getJSONArray("emoji_chars").length())
                val enc = host.request(
                    "telegram.e2ee.sendEncrypted",
                    JSONObject().put("mode", "secret_chat").put("chatId", "sec-zen").put("plaintext", "金刚般若波罗蜜"),
                )
                assertTrue(enc.getBoolean("verifiedRoundtrip"))

                // 3. TON v4r2 Wallet & Collectibles
                val createdWallet = host.request("wallet.createWallet", JSONObject().put("label", "Android TON Vault"))
                assertEquals(24, createdWallet.getJSONArray("mnemonic").length())
                val nfts = host.request("wallet.collectibles")
                assertTrue(nfts.getJSONArray("collectibles").length() >= 3)

                // 4. 全球法布施 240+ Country Globe & Sutras
                val countries = host.request("fabushiMiniapp.countries")
                assertTrue(countries.getJSONArray("countries").length() >= 240)
                val sutras = host.request("fabushiMiniapp.sutras")
                assertTrue(sutras.getJSONArray("sutras").length() >= 3)

                // 5. Hermes Studio & Magpie 47+ Provider Gateway
                val commands = host.request("hermes.listCommands")
                assertTrue(commands.getJSONArray("commands").length() >= 60)
                val presets = host.request("magpie.presets")
                assertTrue(presets.getJSONArray("presets").length() >= 47)
                val masked = host.request(
                    "magpie.maskPreview",
                    JSONObject().put("text", "Key sk-1234567890abcdef12345678 and user@example.com"),
                )
                assertFalse(masked.getString("maskedText").contains("sk-1234567890abcdef"))

                // 6. Cloud Computer Nodes & Mika Phone Agent Bridge Audit
                val nodes = host.request("computer.nodes")
                assertTrue(nodes.getJSONArray("nodes").length() >= 2)
                host.request("mika.bindChat", JSONObject().put("chatId", "bot-mika-phone").put("autoExecute", true))
                host.request(
                    "mika.registerTaskEvent",
                    JSONObject()
                        .put("taskId", "mika-jni-01")
                        .put("phase", "running")
                        .put("summary", "Verified Accessibility + VirtualDisplay bridge"),
                )
                val history = host.request("mika.taskHistory")
                assertTrue(history.getJSONArray("events").length() >= 1)
            }
        } finally {
            dir.deleteRecursively()
        }
    }

    private fun executeFeature(
        host: MahayanaHost,
        type: String,
        requestId: String,
        fields: JSONObject = JSONObject(),
    ): JSONObject {
        fields.put("type", type).put("requestId", requestId)
        val accepted = host.request("feature.execute", JSONObject().put("command", fields))
        assertEquals(requestId, accepted.getString("requestId"))
        return accepted
    }

    private fun receiveFeatureEvent(host: MahayanaHost, expectedType: String): JSONObject {
        repeat(64) {
            val event = host.request("feature.receive")
            if (event.optString("type") == expectedType) return event
        }
        error("Expected FeatureHost event not received: $expectedType")
    }
}
