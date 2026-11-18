package com.wangzan101.mika

import com.wangzan101.mika.core.access.AppAccessPolicy
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.agent.AgentLoop
import com.wangzan101.mika.core.agent.SystemPrompt
import com.wangzan101.mika.core.agent.TaskState
import com.wangzan101.mika.core.assembly.toolDispatcher
import com.wangzan101.mika.core.gate.ConfirmationGate
import com.wangzan101.mika.core.model.Capabilities
import com.wangzan101.mika.core.model.ChatModel
import com.wangzan101.mika.core.model.Message
import com.wangzan101.mika.core.model.ModelReply
import com.wangzan101.mika.core.model.Part
import com.wangzan101.mika.core.model.StopReason
import com.wangzan101.mika.core.model.ToolSpec
import com.wangzan101.mika.core.model.Usage
import com.wangzan101.mika.core.run.Attendance
import com.wangzan101.mika.core.run.Channel
import com.wangzan101.mika.core.run.ChannelGateway
import com.wangzan101.mika.core.run.ChannelInput
import com.wangzan101.mika.core.run.ChannelInputSink
import com.wangzan101.mika.core.run.ChannelInteraction
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOutboxStore
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.core.run.Outbox
import com.wangzan101.mika.core.run.OwnerIdentity
import com.wangzan101.mika.core.run.RunId
import com.wangzan101.mika.core.run.RunInbox
import com.wangzan101.mika.core.run.RunOrigin
import com.wangzan101.mika.core.run.RunProvenance
import com.wangzan101.mika.core.run.RunSpec
import com.wangzan101.mika.core.run.SendOutcome
import com.wangzan101.mika.core.uistate.ScreenPort
import com.wangzan101.mika.core.uistate.ScreenSnapshot
import com.wangzan101.mika.task.TaskRun
import com.wangzan101.mika.task.confirmViaInbox
import java.lang.reflect.Modifier
import java.lang.reflect.Proxy
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.coroutines.Continuation
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class HostChannelRegistrationTest {

    @Test
    fun mikaAppAndTaskRunExposePublicHostChannelHooks() {
        val registerMethod = MikaApp::class.java.getMethod(
            "registerHostChannel",
            Channel::class.java,
            OwnerIdentity::class.java,
        )
        assertTrue(Modifier.isPublic(registerMethod.modifiers))
        assertEquals(AutoCloseable::class.java, registerMethod.returnType)

        val resolveMethod = MikaApp::class.java.getMethod("resolveChannel", String::class.java)
        assertTrue(Modifier.isPublic(resolveMethod.modifiers))
        assertEquals(Channel::class.java, resolveMethod.returnType)

        val readCopiesMethod = MikaApp::class.java.getMethod("readAppCopies", List::class.java)
        assertTrue(Modifier.isPublic(readCopiesMethod.modifiers))
        assertEquals(List::class.java, readCopiesMethod.returnType)

        val confirmMethod = TaskRun::class.java.getMethod(
            "confirmFromChannel",
            ConfirmChoice::class.java,
            String::class.java,
            Continuation::class.java,
        )
        assertTrue(Modifier.isPublic(confirmMethod.modifiers))
    }

    @Test
    fun hostChannelRegistryResolvesUnregistersAndPreservesFallback() = runBlocking {
        val fallback = TestHostChannel(id = "wechat", confirms = false)
        val telegram = TestHostChannel(id = "telegram", confirms = true)
        val startedTasks = CopyOnWriteArrayList<Triple<ChannelPeer, String, String>>()
        val stoppedRuns = CopyOnWriteArrayList<String>()

        val registry = HostChannelRegistry(
            activeAsk = { null },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) {
                    startedTasks += Triple(peer, messageId, text)
                }

                override suspend fun answer(threadId: String, askId: String, text: String): Boolean = false
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean = false
                override suspend fun stop(runId: RunId) {
                    stoppedRuns += runId.value
                }

                override suspend fun cancelAsk(threadId: String) = Unit
            },
            fallbackChannel = { id -> fallback.takeIf { id == "wechat" } },
        )

        assertNull(registry.resolve("telegram"))
        assertSame(fallback, registry.resolve("wechat"))
        assertNull(registry.resolve("unknown"))

        val ownerPeer = ChannelPeer("telegram", "user-42", "chat-100")
        val intruderPeer = ChannelPeer("telegram", "intruder-99", "chat-100")
        val owners = MemoryOwnerIdentity().apply { bind(ownerPeer) }

        val handle = registry.register(telegram, owners)
        assertSame(telegram, registry.resolve("telegram"))
        assertSame(fallback, registry.resolve("wechat"))

        // Non-owner messages must be dropped by ChannelGateway
        telegram.emit(ChannelInput.Text(intruderPeer, "m-bad", "steal session"))
        assertTrue(startedTasks.isEmpty())

        // Bound owner messages and stop signals route cleanly
        telegram.emit(ChannelInput.Text(ownerPeer, "m-1", "打开微信发消息"))
        telegram.emit(ChannelInput.Stop(ownerPeer, RunId("run-77")))
        assertEquals(1, startedTasks.size)
        assertEquals(Triple(ownerPeer, "m-1", "打开微信发消息"), startedTasks.single())
        assertEquals(listOf("run-77"), stoppedRuns)

        // Closing handle unregisters channel and detaches input sink
        handle.close()
        assertNull(registry.resolve("telegram"))
        assertFalse(telegram.hasSink)
    }

    @Test
    fun hostChannelRoutesInlinePaymentConfirmationThroughRunInboxEndToEnd() = runBlocking {
        val peer = ChannelPeer("telegram", "owner-1", "mika-bot-thread")
        val owners = MemoryOwnerIdentity().apply { bind(peer) }
        val inbox = RunInbox()
        val outbox = Outbox(MemoryOutboxStore())
        val firstConfirmDelivered = CompletableDeferred<Delivery.Ask>()
        val channel = TestHostChannel(id = "telegram", confirms = true) { delivery ->
            if (delivery is Delivery.Ask && delivery.kind == Delivery.Ask.Kind.Confirm) {
                firstConfirmDelivered.complete(delivery)
            }
        }

        val screen = Proxy.newProxyInstance(
            ScreenPort::class.java.classLoader,
            arrayOf(ScreenPort::class.java),
        ) { _, method, _ ->
            when (method.name) {
                "getAppAccess" -> AppAccessPolicy.ALLOW
                "snapshot", "peek" -> ScreenSnapshot("snap-1", "com.sankuai.meituan", emptyList(), 1080, 2400)
                else -> when (method.returnType) {
                    Boolean::class.javaPrimitiveType -> false
                    Int::class.javaPrimitiveType -> 0
                    Long::class.javaPrimitiveType -> 0L
                    List::class.java -> emptyList<Any>()
                    else -> null
                }
            }
        } as ScreenPort

        var step = 0
        val scriptedModel = object : ChatModel {
            override val capabilities = Capabilities(
                toolCalls = true,
                vision = false,
                contextTokens = 64_000,
                reasoning = false,
            )
            override val name = "test-main"

            override suspend fun complete(
                system: String,
                messages: List<Message>,
                tools: List<ToolSpec>,
            ): ModelReply {
                step++
                return when (step) {
                    1 -> ModelReply(
                        parts = listOf(
                            Part.ToolCall(
                                id = "call-1",
                                name = "request_confirmation",
                                argsJson = """{"action":"order","target":"美团外卖","content":"生椰拿铁 1 杯","amount":"¥18.50","app":"com.sankuai.meituan"}""",
                            ),
                        ),
                        stopReason = StopReason.ToolUse,
                        usage = Usage(100, 40),
                    )
                    else -> ModelReply(
                        parts = listOf(Part.Text("订单已按您的批准提交。")),
                        stopReason = StopReason.EndTurn,
                        usage = Usage(120, 20),
                    )
                }
            }
        }

        val loop = AgentLoop(
            model = scriptedModel,
            tools = toolDispatcher(screen = screen),
            basePrompt = SystemPrompt.identity(),
            interaction = ChannelInteraction(outbox, channel, peer),
            confirmations = ConfirmationGate(),
            reviewFinalAnswer = false,
        )

        var confirmReceipts = CopyOnWriteArrayList<Boolean>()
        val registry = HostChannelRegistry(
            activeAsk = { threadId -> loop.activeAsk?.takeIf { it.threadId == threadId } },
            route = object : ChannelGateway.Route {
                override suspend fun newTask(peer: ChannelPeer, messageId: String, text: String) = Unit
                override suspend fun answer(threadId: String, askId: String, text: String): Boolean = false
                override suspend fun confirm(threadId: String, askId: String, choice: ConfirmChoice): Boolean =
                    confirmViaInbox(inbox, choice, askId).also { confirmReceipts += it }
                override suspend fun stop(runId: RunId) = Unit
                override suspend fun cancelAsk(threadId: String) = Unit
            },
        )

        registry.register(channel, owners).use {
            val runDeferred = async {
                loop.run(
                    spec = RunSpec(
                        id = RunId("run-confirm-1"),
                        goal = "帮我在美团点一杯生椰拿铁",
                        threadId = peer.threadId,
                        provenance = RunProvenance(RunOrigin.Ui, Attendance.Present),
                    ),
                    inbox = inbox,
                )
            }

            val ask = withTimeout(5_000) { firstConfirmDelivered.await() }
            assertEquals(Delivery.Ask.Kind.Confirm, ask.kind)
            assertEquals(peer.threadId, ask.threadId)

            // Mismatched askId directly against confirmViaInbox must fail closed
            assertFalse(confirmViaInbox(inbox, ConfirmChoice.Once, "wrong-ask-id"))

            // Mismatched askId over ChannelGateway is ignored before touching RunInbox
            channel.emit(ChannelInput.Confirm(peer, "stale-ask-id", ConfirmChoice.Once))
            assertTrue(confirmReceipts.isEmpty())

            // Matching askId from bound owner over confirms=true host channel unblocks AgentLoop
            channel.emit(ChannelInput.Confirm(peer, ask.askId, ConfirmChoice.Once))
            val endState = withTimeout(5_000) { runDeferred.await() }
            assertEquals(listOf(true), confirmReceipts)
            assertTrue("Expected TaskState.Succeeded, got $endState", endState is TaskState.Succeeded)
            assertEquals("订单已按您的批准提交。", (endState as TaskState.Succeeded).answer)
        }
    }

    private class TestHostChannel(
        override val id: String,
        override val confirms: Boolean,
        private val onDelivery: (Delivery) -> Unit = {},
    ) : Channel {
        @Volatile private var currentSink: ChannelInputSink? = null
        val deliveries = CopyOnWriteArrayList<Pair<ChannelPeer, Delivery>>()

        val hasSink: Boolean get() = currentSink != null

        override fun start(sink: ChannelInputSink): AutoCloseable {
            currentSink = sink
            return AutoCloseable {
                if (currentSink === sink) currentSink = null
            }
        }

        suspend fun emit(input: ChannelInput) {
            checkNotNull(currentSink) { "channel $id not started" }.receive(input)
        }

        override suspend fun send(peer: ChannelPeer, delivery: Delivery): SendOutcome {
            deliveries += peer to delivery
            onDelivery(delivery)
            return SendOutcome.Delivered
        }
    }
}
