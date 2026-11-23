package com.ombhrum.fabushi

import androidx.lifecycle.Lifecycle
import com.ombhrum.fabushi.core.MahayanaHost
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Iteration 2 regression tests for:
 * 1. MahayanaHost concurrent double-close & post-close listener/publish safety
 * 2. FabushiRemoteDeviceGateway logout-vs-connect/registered race
 * 3. MiniAppWebMcpSurface nested credential stripping, post-approval foreground re-check,
 *    and atomic MiniAppLifecycleGate invariants (leaving MiniAppWebMcpSurfaceTest.kt untouched)
 */
class M3Iteration2RegressionTest {

    @Test
    fun mahayanaHostConcurrentDoubleClosePreservesSiblingLiveHandleAndRejectsPostCloseListeners() {
        val app = M3TestApplication()
        try {
            val hostA = MahayanaHost(app, featureHostTest = false)
            val hostB = MahayanaHost(app, featureHostTest = false)
            try {
                // 16 threads simultaneously close hostA: SharedHost.refCount must decrement ONLY ONCE (2 -> 1).
                val threadCount = 16
                val ready = CountDownLatch(threadCount)
                val go = CountDownLatch(1)
                val done = CountDownLatch(threadCount)
                val closeError = AtomicReference<Throwable?>(null)

                val threads = (0 until threadCount).map { idx ->
                    Thread({
                        ready.countDown()
                        go.await(5, TimeUnit.SECONDS)
                        runCatching { hostA.close() }.onFailure { closeError.compareAndSet(null, it) }
                        done.countDown()
                    }, "mahayana-close-$idx").apply { isDaemon = true; start() }
                }

                assertTrue(ready.await(5, TimeUnit.SECONDS))
                go.countDown()
                assertTrue(done.await(5, TimeUnit.SECONDS))
                threads.forEach { it.join(1_000L) }
                assertEquals(null, closeError.get())

                // Sibling hostB sharing the same appDataDir MUST remain fully operational!
                val infoB = hostB.requestResult("feature.info")
                assertTrue("Sibling live hostB must stay usable after concurrent hostA.close(): $infoB", infoB.isSuccess)

                // hostA must reject requests, listener registration, and event publication cleanly after close()
                assertTrue(hostA.requestResult("feature.info").isFailure)
                val addListenerErr = runCatching { hostA.addFeatureEventListener { } }.exceptionOrNull()
                assertTrue(addListenerErr is IllegalStateException)
                val publishErr = runCatching { hostA.publishFeatureEvent(JSONObject().put("type", "ping")) }.exceptionOrNull()
                assertTrue(publishErr is IllegalStateException)
            } finally {
                hostB.close()
            }
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun remoteDeviceGatewayLogoutRacingWithDelayedRegistrationNeverLeavesIsRegisteredTrue() {
        val app = M3TestApplication()
        val registerReceived = CountDownLatch(1)
        val allowServerRegisteredReply = CountDownLatch(1)
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":89,"username":"gw_race","first_name":"Race","session_id":"gw-race-89","token":"tok_gw_race_12345678901234567890"}""",
                    )
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
            wsOnClientMessage = { msg, reply ->
                val obj = JSONObject(msg)
                if (obj.optString("type") == "register") {
                    registerReceived.countDown()
                    // Hold server's "registered" frame until test calls setLoggedIn(false)
                    allowServerRegisteredReply.await(3, TimeUnit.SECONDS)
                    reply("""{"type":"registered","deviceId":"${obj.optString("deviceId")}"}""")
                }
            },
        )
        try {
            MahayanaHost(app, featureHostTest = false).use { host ->
                host.request(
                    "telegram.server.login",
                    JSONObject()
                        .put("endpoint", server.baseUrl)
                        .put("username", "gw_race")
                        .put("password", "pw"),
                )

                val gateway = FabushiRemoteDeviceGateway(
                    context = app,
                    surface = FabushiAppAgentSurface(),
                    metadata = mapOf("env" to "race-test"),
                    gatewayUrl = "${server.wsBaseUrl}/agent",
                    retryDelayMs = 100L,
                )
                try {
                    gateway.setLoggedIn(true)
                    assertTrue("Expected register frame from gateway", registerReceived.await(5, TimeUnit.SECONDS))

                    // Logout while "registered" response is still in flight
                    gateway.setLoggedIn(false)
                    allowServerRegisteredReply.countDown()
                    Thread.sleep(200L)

                    assertFalse("Gateway must NEVER flip isRegistered()=true after setLoggedIn(false)", gateway.isRegistered())
                } finally {
                    gateway.close()
                }
            }
        } finally {
            server.close()
            app.cleanup()
        }
    }

    @Test
    fun miniAppAccountProjectionStripsNestedCredentialsInMembershipAndAvatar() {
        val rawAuth = JSONObject()
            .put("loggedIn", true)
            .put(
                "user",
                JSONObject()
                    .put("id", "u-99")
                    .put("username", "lotus")
                    .put("role", "member")
                    .put(
                        "membership",
                        JSONObject()
                            .put("tier", "pro")
                            .put("accessToken", "nested-secret-token-should-never-leak"),
                    )
                    .put(
                        "avatar",
                        JSONObject()
                            .put("url", "https://example.com/a.png")
                            .put("sessionCookie", "cookie-secret"),
                    ),
            )

        val projected = miniAppAccountProjection(rawAuth)
        val serialized = projected.toString()
        assertFalse("Nested accessToken must be stripped", serialized.contains("nested-secret-token-should-never-leak"))
        assertFalse("Nested sessionCookie must be stripped", serialized.contains("cookie-secret"))
        val user = projected.getJSONObject("user")
        assertEquals("u-99", user.getString("id"))
        assertEquals("lotus", user.getString("username"))
        assertEquals("member", user.getString("role"))
        assertFalse("Nested JSONObject membership must not be copied verbatim", user.has("membership"))
        assertFalse("Nested JSONObject avatar must not be copied verbatim", user.has("avatar"))
    }

    @Test
    fun miniAppNativeBridgeRejectsExecutionIfDocumentBecomesInactiveDuringApprovalPrompt() {
        val app = M3TestApplication()
        try {
            val plugin = MarketplacePlugin(
                pluginId = "global-dharma",
                displayName = "全球法布施",
                description = "Post-approval foreground guard test",
                latestVersion = "2.3.0",
                tools = listOf(
                    MiniAppToolContract(
                        name = "transfer.send",
                        description = "Mutating transfer",
                        approval = "confirm",
                        title = "Send",
                    ),
                ),
            )
            val active = AtomicBoolean(true)
            val executed = AtomicInteger(0)

            val bridge = MiniAppNativeWebMcpBridge(
                plugin = plugin,
                context = app,
                localDocumentActive = { active.get() },
                callRuntimeToolJson = { _, _, _ ->
                    executed.incrementAndGet()
                    """{"done":true}"""
                },
                approvalPrompt = {
                    // Simulate ON_PAUSE/ON_STOP/onDispose happening while approval dialog is open
                    active.set(false)
                    true
                },
            )

            val response = JSONObject(bridge.callTool("transfer.send", """{"amount":1}"""))
            assertFalse("Tool call must be rejected when document becomes inactive during approval", response.getBoolean("ok"))
            assertEquals("Runtime tool must NOT execute after backgrounding/disposal", 0, executed.get())
        } finally {
            app.cleanup()
        }
    }

    @Test
    fun miniAppLifecycleGateMaintainsAtomicStopAndDisposeInvariantsUnderConcurrency() {
        repeat(20) {
            val gate = MiniAppLifecycleGate()
            val start = CountDownLatch(1)
            val done = CountDownLatch(2)

            val loader = Thread {
                start.await(2, TimeUnit.SECONDS)
                repeat(200) { gate.onLocalHtmlLoaded() }
                done.countDown()
            }.apply { isDaemon = true; start() }

            val disposer = Thread {
                start.await(2, TimeUnit.SECONDS)
                gate.onLifecycleEvent(Lifecycle.Event.ON_STOP)
                gate.onDispose()
                done.countDown()
            }.apply { isDaemon = true; start() }

            start.countDown()
            assertTrue(done.await(5, TimeUnit.SECONDS))
            loader.join(1_000L)
            disposer.join(1_000L)

            assertTrue(gate.disposed)
            assertFalse("Disposed gate must NEVER report localDocumentActive == true", gate.localDocumentActive)
            assertFalse("Disposed gate must NEVER allow host event dispatch", gate.canDispatchHostEvent())
            gate.onLocalHtmlLoaded()
            gate.onLifecycleEvent(Lifecycle.Event.ON_RESUME)
            assertFalse(gate.localDocumentActive)
        }
    }
}
