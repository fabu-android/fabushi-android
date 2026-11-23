package com.ombhrum.fabushi

import com.ombhrum.fabushi.core.MahayanaHost
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Verifies M3 Feature F7:
 * 1. Lock-free concurrent JNI dispatch (ReentrantReadWriteLock + Rust lock-free HTTP I/O)
 * 2. Fail-closed MahayanaHost.requestResult()
 * 3. All 19 Kotlin <-> Rust JNI contracts against real libmahayana_app_host.so
 */
class MahayanaHostConcurrencyAndContractsTest {

    @Test
    fun concurrentJniDispatchDoesNotBlockFastRpcsDuringSlowNetworkIo() {
        val app = M3TestApplication()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":42,"username":"alice","first_name":"Alice","session_id":"s42","token":"tok_concurrent_test_1234567890"}""",
                    )
                    "/api/v1/chats" -> MockHttpResponse(
                        body = """{"chats":[]}""",
                        delayMs = 600L,
                    )
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                host.request(
                    "telegram.server.login",
                    JSONObject()
                        .put("endpoint", server.baseUrl)
                        .put("username", "alice")
                        .put("password", "pass123"),
                )

                val slowStarted = CountDownLatch(1)
                val slowDone = CountDownLatch(1)
                val slowErr = AtomicReference<Throwable?>(null)
                val slowThread = Thread {
                    slowStarted.countDown()
                    runCatching { host.request("telegram.server.listChats") }
                        .onFailure { slowErr.set(it) }
                    slowDone.countDown()
                }.apply { isDaemon = true; start() }

                assertTrue(slowStarted.await(2, TimeUnit.SECONDS))
                Thread.sleep(60L) // Ensure slow HTTP call is inside Rust ureq I/O

                val t0 = System.nanoTime()
                val info = host.requestResult("feature.info")
                val sess = host.requestResult("telegram.server.session")
                val persisted = host.requestResult("feature.persistence.set", JSONObject().put("key", "k").put("value", "v"))
                val fastElapsedMs = (System.nanoTime() - t0) / 1_000_000L

                assertTrue(info.isSuccess)
                assertTrue(sess.isSuccess)
                assertTrue(persisted.isSuccess)
                assertTrue(
                    "Fast JNI calls should complete concurrently while slow HTTP RPC is in-flight (< 300ms), took ${fastElapsedMs}ms",
                    fastElapsedMs < 300L,
                )
                assertTrue(slowDone.await(5, TimeUnit.SECONDS))
                assertEquals(null, slowErr.get())
                slowThread.join(1000)
            }
        } finally {
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun requestResultFailsClosedOnErrorsAndClosedHost() {
        val app = M3TestApplication()
        try {
            val host = MahayanaHost(app, featureHostTest = false)
            val unknown = host.requestResult("unknown.nonexistent.method")
            assertTrue(unknown.isFailure)
            assertNotNull(unknown.exceptionOrNull()?.message)

            val callFail = host.requestResult("telegram.calls.startCall", JSONObject().put("conversationId", "1"))
            assertTrue(callFail.isFailure)
            assertTrue(callFail.exceptionOrNull()?.message.orEmpty().contains("CALL_TRANSPORT_UNAVAILABLE"))

            val tcFail = host.requestResult("wallet.tonConnectApprove", JSONObject())
            assertTrue(tcFail.isFailure)
            assertTrue(tcFail.exceptionOrNull()?.message.orEmpty().contains("TONCONNECT_UNAVAILABLE"))

            host.close()
            val afterClose = host.requestResult("feature.info")
            assertTrue(afterClose.isFailure)
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun allNineteenJniContractsRoundTripAccuratelyAgainstRealHost() {
        val app = M3TestApplication()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":10,"username":"tester","first_name":"Tester","session_id":"s10","token":"tok_19_contracts_1234567890"}""",
                    )
                    "/api/v1/messages/send" -> MockHttpResponse(
                        body = """{"message_id":777,"pts":5,"date_ms":1700000000000,"status":"sent"}""",
                    )
                    "/api/v1/computers" -> MockHttpResponse(
                        body = """{"computers":[{"node_id":"cloud:10:gpu-1","owner_user_id":10,"kind":"cloud","display_name":"GPU Box","os_name":"Ubuntu 24.04","resolution":"1920x1080","online":true,"run_policy":"auto","has_saved_credential":true}]}""",
                    )
                    "/api/v1/computers/exec" -> MockHttpResponse(
                        body = """{"command_id":"cmd-1","node_id":"cloud:10:gpu-1","ok":true,"exit_code":0,"stdout":"Linux gpu-1","stderr":"","entries":[],"executed_by":"ssh","duration_ms":12}""",
                    )
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
        )
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                // Contract #1: Default endpoint & configureEndpoint + setEndpoint alias + flushOutbox -> /api/v1/messages/send
                val initialStatus = host.request("telegram.server.status")
                assertEquals("http://34.46.77.133:8080", initialStatus.getString("endpoint"))
                val cfg1 = host.request("telegram.server.configureEndpoint", JSONObject().put("endpoint", server.baseUrl))
                assertEquals(server.baseUrl, cfg1.getString("endpoint"))
                host.request("telegram.server.login", JSONObject().put("endpoint", server.baseUrl).put("username", "tester").put("password", "pw"))
                host.request("telegram.server.enqueueOutbox", JSONObject().put("payload", JSONObject().put("chat_id", 101).put("text", "queued")))
                val flush = host.request("telegram.server.flushOutbox")
                assertEquals(1, flush.getInt("flushedCount"))
                assertTrue(server.recordedRequests.any { it.path == "/api/v1/messages/send" })

                // Contract #2 & #3: Magpie upsertProvider (presetId + apiKey) & importUri / importEnv
                val upserted = host.request(
                    "magpie.upsertProvider",
                    JSONObject().put("presetId", "openai").put("apiKey", "sk-real-contract-key-01").put("enabled", true),
                )
                assertTrue(upserted.getBoolean("configuredForLive"))
                val providers = host.request("magpie.providers")
                assertTrue(providers.getBoolean("liveReady"))
                val envImport = host.request("magpie.importEnv", JSONObject().put("envText", "ANTHROPIC_API_KEY=sk-ant-test-key-02"))
                assertTrue(envImport.getInt("importedCount") >= 1)

                // Contract #4, #5, #6, #7: TON Wallet createWallet, status, sendTon, tonConnectApprove
                val walletCreated = host.request("wallet.createWallet", JSONObject().put("label", "Test"))
                val addr = walletCreated.getString("address")
                assertFalse(addr.startsWith("UQBk9x_mahayana_v4r2_"))
                val walletStatus = host.request("wallet.status")
                assertTrue(walletStatus.getBoolean("hasWallet"))
                assertEquals(addr, walletStatus.getString("address"))
                assertTrue(walletStatus.has("balanceTon"))
                val sendZeroFail = host.requestResult(
                    "wallet.sendTon",
                    JSONObject().put("to", addr).put("amountNanotons", 0L).put("comment", "").put("broadcast", true),
                )
                assertTrue(sendZeroFail.isFailure)
                assertTrue(host.requestResult("wallet.tonConnectApprove").isFailure)

                // Contract #8: Passcode Argon2id ({passcode} -> {valid})
                val setPass = host.request("telegram.auth.passcodeSet", JSONObject().put("passcode", "2468"))
                assertEquals("Argon2id", setPass.getString("kdf"))
                assertTrue(host.request("telegram.auth.passcodeVerify", JSONObject().put("passcode", "2468")).getBoolean("valid"))
                assertFalse(host.request("telegram.auth.passcodeVerify", JSONObject().put("passcode", "0000")).getBoolean("valid"))

                // Contract #9: Account switch fence ({accountId, expectedGeneration})
                val addedAcc = host.request(
                    "telegram.auth.accountAdd",
                    JSONObject().put("displayName", "Second").put("username", "@second").put("phone", ""),
                )
                val accId = addedAcc.getString("accountId")
                val wrongGen = host.requestResult(
                    "telegram.auth.accountSwitch",
                    JSONObject().put("accountId", accId).put("expectedGeneration", 9999L),
                )
                assertTrue(wrongGen.isFailure)

                // Contract #10: Forum topic ({conversationId, title, iconEmoji})
                val topic = host.request(
                    "telegram.messaging.createTopic",
                    JSONObject().put("conversationId", "conv-1").put("title", "Dev").put("iconEmoji", "🦀"),
                )
                assertEquals("🦀", topic.getString("iconEmoji"))

                // Contract #11: E2EE ({peerId, chatId, plaintext})
                host.request("telegram.e2ee.initDevice", JSONObject().put("deviceId", "dev-1"))
                val sec = host.request(
                    "telegram.e2ee.createSecretChat",
                    JSONObject().put("peerId", "bob").put("chatId", "sec-bob"),
                )
                assertEquals("sec-bob", sec.getString("chatId"))
                val enc = host.request(
                    "telegram.e2ee.sendEncrypted",
                    JSONObject().put("chatId", "sec-bob").put("plaintext", "secret hello").put("mode", "secret_chat"),
                )
                assertTrue(enc.getBoolean("verifiedRoundtrip"))

                // Contract #12: Global Dharma ({rotations}, {durationSeconds}, {prompt}, countries, bootstrap)
                val boot = host.request("fabushiMiniapp.bootstrap")
                assertTrue(boot.getJSONObject("profile").has("total_transfer_count"))
                val countries = host.request("fabushiMiniapp.countries")
                assertTrue(countries.getJSONArray("countries").length() >= 240)
                val spin = host.request("fabushiMiniapp.spinPrayerWheel", JSONObject().put("rotations", 3))
                assertEquals(3, spin.getInt("rotationsAdded"))
                val prac = host.request("fabushiMiniapp.recordPractice", JSONObject().put("durationSeconds", 300L))
                assertEquals(300L, prac.getLong("durationSeconds"))

                // Contract #13, #14, #15: Hermes ({target, content}, {input}, {approvalId, decision})
                val soul = host.request(
                    "hermes.memoryUpdate",
                    JSONObject().put("target", "SOUL.md").put("content", "# Truthful Soul"),
                )
                assertEquals("SOUL.md", soul.getString("target"))
                val slash = host.request("hermes.runSlashCommand", JSONObject().put("input", "/help"))
                assertTrue(slash.getString("summary").isNotBlank())
                val badApproval = host.requestResult(
                    "hermes.approvalsDecide",
                    JSONObject().put("approvalId", "missing-id").put("decision", "approved"),
                )
                assertTrue(badApproval.isFailure)

                // Contract #16 & #17: Computers (listNodes platform/viewport + server listComputers + execComputer)
                val localNodes = host.request("computer.listNodes").getJSONArray("nodes")
                assertEquals(1, localNodes.length())
                assertEquals("local-android-mika", localNodes.getJSONObject(0).getString("nodeId"))
                assertTrue(localNodes.getJSONObject(0).has("platform"))
                assertTrue(localNodes.getJSONObject(0).getJSONObject("viewport").has("width"))
                val srvComps = host.request("telegram.server.listComputers").getJSONArray("computers")
                assertEquals("cloud:10:gpu-1", srvComps.getJSONObject(0).getString("node_id"))
                val execRes = host.request(
                    "telegram.server.execComputer",
                    JSONObject()
                        .put("nodeId", "cloud:10:gpu-1")
                        .put("operation", JSONObject().put("op", "shell_exec").put("command", "uname -a")),
                )
                assertTrue(execRes.getBoolean("ok"))
                assertEquals("Linux gpu-1", execRes.getString("stdout"))

                // Contract #18: Mika task event ({taskId, stepIndex, phase, summary})
                val mikaEv = host.request(
                    "mika.registerTaskEvent",
                    JSONObject().put("taskId", "t1").put("stepIndex", 2).put("phase", "running").put("summary", "step 2"),
                )
                assertTrue(mikaEv.getBoolean("recorded"))
                assertEquals(2, mikaEv.getInt("stepIndex"))

                // Contract #19: VAD PCM ({pcmSamples}) & Call fail-closed
                val pcm = JSONArray(List(288) { 5000 })
                val vad = host.request("telegram.calls.vadFeedPcm", JSONObject().put("pcmSamples", pcm).put("ttsPlaying", true))
                assertTrue(vad.has("framesEvaluated"))
                assertTrue(host.requestResult("telegram.calls.startCall", JSONObject().put("conversationId", "1")).isFailure)
            }
        } finally {
            server.close()
            app.cleanup()
        }
    }
}
