package com.ombhrum.fabushi

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.mika.MikaChannelCardEvent
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import com.ombhrum.fabushi.telegram.TelegramUiState
import com.ombhrum.fabushi.telegram.TelegramViewModel
import com.ombhrum.fabushi.telegram.publishTelegramSemanticSurface
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.RunId
import com.wangzan101.mika.core.run.SendOutcome
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Verifies M3 Feature F11 & TEST_INFRA.md §3.3 M3 coordination contracts:
 * 1. Pure-JVM TelegramMikaChannel + :mika-core ChannelGateway + ConfirmationGate inline Approve/Decline
 * 2. FabushiRemoteDeviceGateway loopback WS registration via bridged Telegram deviceAgentSession & teardown
 * 3. FabushiAppAgentSurface + internal publishTelegramSemanticSurface canonical "invoke" / "setValue" actions
 */
class TelegramMikaAndGatewayIntegrationTest {
    @get:Rule
    val mainDispatcherRule = M3MainDispatcherRule()

    @Test
    fun telegramMikaChannelRoutesOwnerInputAndConfirmationThroughMikaCoreGatewayOnPureJvm() = runBlocking {
        val channel = TelegramMikaChannel()
        assertEquals("telegram", channel.id)
        assertTrue(channel.supportsImages)
        assertTrue(channel.confirms)

        val owners = MemoryOwnerIdentity()
        val ownerPeer = ChannelPeer(TelegramMikaChannel.ID, "user:42", "chat-101")
        val intruderPeer = ChannelPeer(TelegramMikaChannel.ID, "user:999", "chat-101")
        owners.bind(ownerPeer)

        var activeAsk: Delivery.Ask? = null
        val newTasks = CopyOnWriteArrayList<String>()
        val confirmDecisions = CopyOnWriteArrayList<Pair<String, ConfirmChoice>>()
        val answers = CopyOnWriteArrayList<Pair<String, String>>()

        val gateway = ChannelGateway(
            owners = owners,
            activeAsk = { tid -> activeAsk?.takeIf { it.threadId == tid } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) {
                    newTasks.add(text)
                }
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean {
                    answers.add(askId to text)
                    return true
                }
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean {
                    confirmDecisions.add(askId to choice)
                    return true
                }
                override suspend fun cancelAsk(threadId: String) {
                    activeAsk = null
                }
                override suspend fun stop(runId: RunId) = Unit
            },
        )
        val reg = gateway.start(channel)
        try {
            assertTrue(channel.hasActiveSink())

            // Non-owner dropped silently
            channel.submitTask(intruderPeer, "m-intruder", "steal data")
            assertTrue(newTasks.isEmpty())

            // Owner task routed
            channel.submitTask(ownerPeer, "m-1", "open settings")
            assertEquals(listOf("open settings"), newTasks.toList())

            // Deliver ConfirmationGate Ask -> emits inline Approve / Decline buttons
            val confirmAsk = Delivery.Ask("del-1", "chat-101", "ask-confirm-1", Delivery.Ask.Kind.Confirm, "Confirm sending transfer?")
            activeAsk = confirmAsk
            val outcome = channel.send(ownerPeer, confirmAsk)
            assertEquals(SendOutcome.Delivered, outcome)
            val card = channel.deliveredEvents().last()
            assertEquals(MikaChannelCardEvent.Kind.ASK_CONFIRM, card.kind)
            assertEquals(2, card.inlineButtons.size)
            assertTrue(card.inlineButtons[0].payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_APPROVE_PREFIX))
            assertTrue(card.inlineButtons[1].payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_DECLINE_PREFIX))

            // Mismatched askId rejected
            assertFalse(channel.resolveConfirmationForThread("chat-101", approved = true, askId = "wrong-ask"))
            assertTrue(confirmDecisions.isEmpty())

            // Matching Approve routes ConfirmChoice.Once
            assertTrue(channel.resolveConfirmationForThread("chat-101", approved = true, askId = "ask-confirm-1"))
            assertEquals(listOf("ask-confirm-1" to ConfirmChoice.Once), confirmDecisions.toList())

            // Second confirm Ask -> Decline routes ConfirmChoice.Deny
            val confirmAsk2 = Delivery.Ask("del-2", "chat-101", "ask-confirm-2", Delivery.Ask.Kind.Confirm, "Confirm delete?")
            activeAsk = confirmAsk2
            channel.send(ownerPeer, confirmAsk2)
            assertTrue(channel.resolveConfirmationForThread("chat-101", approved = false, askId = "ask-confirm-2"))
            assertEquals("ask-confirm-2" to ConfirmChoice.Deny, confirmDecisions.last())

            // Input Ask -> answerAsk routes text and acknowledges messageId
            val inputAsk = Delivery.Ask("del-3", "chat-101", "ask-input-1", Delivery.Ask.Kind.Input, "Which contact?")
            activeAsk = inputAsk
            channel.send(ownerPeer, inputAsk)
            assertTrue(channel.answerAsk(ownerPeer, "m-ans-1", "Alice"))
            assertEquals(listOf("ask-input-1" to "Alice"), answers.toList())
            assertTrue(channel.acknowledgedMessageIds.contains("m-ans-1"))
        } finally {
            reg.close()
            channel.close()
        }
    }

    @Test
    fun publishTelegramSemanticSurfaceUsesCanonicalInvokeAndSetValueAndRejectsLegacyActions() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.openSavedMessages()
            val curState: TelegramUiState = awaitState(vm) { it.activeChat != null }

            val surface = FabushiAppAgentSurface()
            publishTelegramSemanticSurface(surface, curState, vm)
            val snap = surface.snapshot()
            assertTrue(snap.elements.any { it.agentId == "telegram.nav.settings" })

            // Canonical "invoke" succeeds
            surface.action(snap.generation, "telegram.nav.settings", "invoke")

            // Canonical "setValue" succeeds on composer textbox
            val snap2 = surface.snapshot()
            val composerId = snap2.elements.first { it.agentId.startsWith("telegram.chat.composer:") }.agentId
            surface.action(snap2.generation, composerId, "setValue", "Hello canonical MCP")
            assertEquals("Hello canonical MCP", vm.state.value.activeChat?.draftText)

            // Legacy "tap" and "set_text" are strictly rejected with unsupported_app_surface_action
            val snap3 = surface.snapshot()
            val tapErr = runCatching { surface.action(snap3.generation, "telegram.nav.home", "tap") }.exceptionOrNull()
            assertEquals("unsupported_app_surface_action", tapErr?.message)

            val setTextErr = runCatching { surface.action(snap3.generation, composerId, "set_text", "bad") }.exceptionOrNull()
            assertEquals("unsupported_app_surface_action", setTextErr?.message)
        } finally {
            store.clear()
            app.cleanup()
        }
    }

    @Test
    fun remoteDeviceGatewayRegistersViaBridgedTelegramSessionAndTearsDownOnLogout() = runBlocking {
        val app = M3TestApplication()
        val registeredFrames = CopyOnWriteArrayList<JSONObject>()
        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when (req.path) {
                    "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":88,"username":"gw_user","first_name":"GW","session_id":"gw-sess-88","token":"tok_gateway_bridge_1234567890123456"}""",
                    )
                    else -> MockHttpResponse(body = """{"ok":true}""")
                }
            },
            wsOnClientMessage = { msg, reply ->
                val obj = JSONObject(msg)
                if (obj.optString("type") == "register") {
                    registeredFrames.add(obj)
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
                        .put("username", "gw_user")
                        .put("password", "pw"),
                )

                val surface = FabushiAppAgentSurface()
                var invoked = false
                surface.publish(
                    screen = "telegram.home",
                    elements = listOf(FabushiAppAgentSurface.Element("btn.test", "button", "Test")),
                    actions = mapOf("btn.test" to FabushiAppAgentSurface.Action(setOf("invoke")) { invoked = true }),
                )

                val gateway = FabushiRemoteDeviceGateway(
                    context = app,
                    surface = surface,
                    metadata = mapOf("env" to "test"),
                    gatewayUrl = "${server.wsBaseUrl}/agent",
                    retryDelayMs = 100L,
                )
                try {
                    assertFalse(gateway.isRegistered())
                    gateway.setLoggedIn(true)
                    awaitCondition(timeoutMs = 5_000L) { gateway.isRegistered() }
                    assertTrue(registeredFrames.isNotEmpty())

                    // Invoke semantic snapshot + action through gateway
                    val snap = gateway.invokeTool(FabushiAppAgentSurface.SnapshotTool, JSONObject())
                    val gen = snap.getLong("generation")
                    gateway.invokeTool(
                        FabushiAppAgentSurface.ActionTool,
                        JSONObject().put("generation", gen).put("agentId", "btn.test").put("action", "invoke"),
                    )
                    assertTrue(invoked)

                    // Sign out tears down gateway cleanly
                    gateway.setLoggedIn(false)
                    assertFalse(gateway.isRegistered())
                } finally {
                    gateway.close()
                }
            }
        } finally {
            server.close()
            app.cleanup()
        }
    }
}
