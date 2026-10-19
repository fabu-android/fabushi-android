package com.ombhrum.fabushi

import androidx.test.platform.app.InstrumentationRegistry
import com.ombhrum.fabushi.core.MahayanaHost
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MahayanaFeatureHostTest {
    @Test
    fun nativeBridgeExecutesCompleteCrossPlatformUserJourney() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        MahayanaHost(context, featureHostTest = true).use { host ->
            val info = host.request("feature.info")
            assertEquals("android", info.getString("platform"))
            assertFalse(info.getString("protocolVersion").isBlank())
            assertTrue(info.getString("runtimeVersion").contains("test"))

            host.request("feature.auth.status")
            val providers = host.requestValue("feature.auth.providers") as org.json.JSONArray
            assertTrue((0 until providers.length()).any { providers.getJSONObject(it).getString("id") == "google" })

            val oauth = host.request(
                "feature.auth.oauthStart",
                JSONObject().put("provider", "google"),
            )
            val oauthCompleted = host.request(
                "feature.auth.oauthPoll",
                JSONObject().put("attemptId", oauth.getString("attemptId")),
            )
            assertEquals("completed", oauthCompleted.getString("status"))

            execute(host, "chat.send", "android-chat", JSONObject().put("text", "请用一句话说明自动化测试状态"))
            execute(
                host,
                "marketplace.install",
                "android-install",
                JSONObject().put("miniAppId", "global-dharma"),
            )
            execute(
                host,
                "miniapp.open",
                "android-open",
                JSONObject().put("miniAppId", "global-dharma"),
            )
            execute(
                host,
                "capability.request",
                "android-capability",
                JSONObject()
                    .put("miniAppId", "global-dharma")
                    .put("capability", "camera")
                    .put("reason", "cross-platform UI automation"),
            )
            val approval = receive(host, "approval.requested")
            host.request(
                "feature.approval.resolve",
                JSONObject().put(
                    "resolution",
                    JSONObject()
                        .put("approvalId", approval.getString("approvalId"))
                        .put("decision", "allow-once"),
                ),
            )

            val longTask = execute(
                host,
                "runtime.longTask",
                "android-long-task",
                JSONObject().put("label", "Android simulated user operation"),
            )
            host.request(
                "feature.interrupt",
                JSONObject().put("operationId", longTask.getString("operationId")),
            )

            execute(host, "session.clear", "android-session-clear")
        }
    }

    private fun execute(
        host: MahayanaHost,
        type: String,
        requestId: String,
        fields: JSONObject = JSONObject(),
    ): JSONObject {
        fields.put("type", type).put("requestId", requestId)
        val accepted = host.request(
            "feature.execute",
            JSONObject().put("command", fields),
        )
        assertEquals(requestId, accepted.getString("requestId"))
        return accepted
    }

    private fun receive(host: MahayanaHost, expectedType: String): JSONObject {
        repeat(64) {
            val event = host.request("feature.receive")
            if (event.optString("type") == expectedType) return event
        }
        error("FeatureHost event not received: $expectedType")
    }
}
