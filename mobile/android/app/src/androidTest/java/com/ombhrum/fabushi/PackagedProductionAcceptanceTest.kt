package com.ombhrum.fabushi

import android.content.Intent
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ombhrum.fabushi.androidmain.coordinator.AndroidCoordinatorPorts
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class PackagedProductionAcceptanceTest {
    @Test
    fun productionCoordinatorOwnsAuthenticatedStreamCancelMcpWebauthnAndLogout() {
        assertTrue(
            "Packaged production acceptance must run only in the CI acceptance build",
            BuildConfig.CI_ACCOUNT_SESSION_IMPORT_ENABLED,
        )

        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val application = context.applicationContext as FabushiApplication
        val gatewayTrace = File(
            context.getExternalFilesDir(null) ?: context.filesDir,
            "device-gateway-trace.jsonl",
        )
        gatewayTrace.delete()
        val intent = Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)

        ActivityScenario.launch<MainActivity>(intent).use {
            val coordinator = AndroidCoordinatorPorts.presentation(application)
            val status = coordinator.coordinatorStatus()
            val generation = status.getLong("generation")
            assertTrue(generation > 0L)

            val auth = coordinator.authStatus()
            assertTrue(
                "A real short-lived CI account session is required for packaged acceptance",
                auth.optBoolean("loggedIn", false),
            )
            val deviceSession = coordinator.authDeviceAgentSession()
            val expectedDeviceId = deviceSession.optString("deviceId")
            assertTrue(expectedDeviceId.startsWith("gha-"))
            assertTrue(deviceSession.optString("sessionId").startsWith("ci-runner:"))
            assertTrue(
                "Authenticated packaged acceptance requires backend-confirmed remote-device registration",
                waitForGatewayRegistration(gatewayTrace, expectedDeviceId),
            )

            val events = CopyOnWriteArrayList<JSONObject>()
            val deltaSeen = CountDownLatch(1)
            val completedSeen = CountDownLatch(1)
            val subscription = coordinator.addFeatureEventListener { event ->
                events += JSONObject(event.toString())
                when (event.optString("type")) {
                    "chat.delta" -> deltaSeen.countDown()
                    "operation.completed" -> completedSeen.countDown()
                }
            }

            try {
                val requestId = "packaged-chat-${System.nanoTime()}"
                val accepted = coordinator.featureExecute(
                    JSONObject().put(
                        "command",
                        JSONObject()
                            .put("type", "chat.send")
                            .put("requestId", requestId)
                            .put("agentId", "mahayana-assistant")
                            .put("mode", "agent")
                            .put("text", "Packaged acceptance: stream one response."),
                    ),
                )
                val operationId = accepted.getString("operationId")
                assertTrue(accepted.optBoolean("accepted", false))
                assertTrue(operationId.isNotBlank())
                assertTrue(
                    "Production chat must emit at least one streamed delta",
                    deltaSeen.await(20, TimeUnit.SECONDS),
                )
                assertTrue(
                    "Production chat must settle with a terminal event",
                    completedSeen.await(45, TimeUnit.SECONDS),
                )
                val streamedText = events
                    .filter { it.optString("type") == "chat.delta" }
                    .joinToString(separator = "") { it.optString("delta") }
                assertTrue(
                    "Production inference must return real streamed model output",
                    streamedText.isNotBlank(),
                )
                assertFalse(
                    "The production Host must not pass acceptance with the placeholder provider",
                    streamedText == "Fabushi Android Host accepted the message.",
                )

                val streamEvents = events.filter {
                    it.optString("operationId") == operationId &&
                        it.optJSONObject("_coordinator") != null
                }
                assertTrue(streamEvents.isNotEmpty())
                val sequences = streamEvents.map {
                    val metadata = it.getJSONObject("_coordinator")
                    assertEquals(generation, metadata.getLong("generation"))
                    metadata.getLong("sequence")
                }
                assertTrue(sequences.all { it > 0L })
                assertEquals(sequences.sorted(), sequences.distinct())

                val longTask = coordinator.featureExecute(
                    JSONObject().put(
                        "command",
                        JSONObject()
                            .put("type", "runtime.longTask")
                            .put("requestId", "packaged-stop-${System.nanoTime()}"),
                    ),
                )
                val longOperationId = longTask.getString("operationId")
                val interrupted = coordinator.featureInterrupt(
                    JSONObject().put("operationId", longOperationId),
                )
                assertEquals("interrupted", interrupted.getString("status"))
                assertEquals(0, coordinator.coordinatorStatus().getInt("activeRequestCount"))

                val state = "packaged-oauth-${System.nanoTime()}"
                assertTrue(coordinator.mcpOAuthRegister(state, "packaged-test-provider"))
                val oauth = coordinator.mcpOAuthComplete(
                    state = state,
                    code = null,
                    error = "access_denied",
                )
                assertEquals("packaged-test-provider", oauth.provider)
                assertEquals("failed", oauth.outcome)

                val provider = coordinator.webAuthnRegisterProvider()
                val providerId = provider.getString("providerId")
                assertTrue(providerId.isNotBlank())
                val welcome = coordinator.webAuthnPollRequest(
                    JSONObject().put("providerId", providerId),
                )
                assertEquals("welcome", welcome.getJSONObject("frame").getString("kind"))
                coordinator.webAuthnSubmitResponses(
                    JSONObject()
                        .put("providerId", providerId)
                        .put(
                            "frames",
                            org.json.JSONArray()
                                .put(JSONObject().put("kind", "ping")),
                        ),
                )
                coordinator.webAuthnUnregisterProvider(
                    JSONObject().put("providerId", providerId),
                )

                val loggedOut = coordinator.authLogout()
                assertFalse(loggedOut.optBoolean("loggedIn", true))
                assertFalse(coordinator.authStatus().optBoolean("loggedIn", true))
            } finally {
                subscription.close()
            }
        }
    }

    private fun waitForGatewayRegistration(trace: File, expectedDeviceId: String): Boolean {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(45)
        while (System.nanoTime() < deadline) {
            val registered = runCatching {
                trace.takeIf(File::isFile)
                    ?.readLines(Charsets.UTF_8)
                    .orEmpty()
                    .any { line ->
                        val record = runCatching { JSONObject(line) }.getOrNull()
                        record?.optString("phase") == "registered" &&
                            record.optString("deviceId") == expectedDeviceId
                    }
            }.getOrDefault(false)
            if (registered) return true
            Thread.sleep(250)
        }
        return false
    }

}
