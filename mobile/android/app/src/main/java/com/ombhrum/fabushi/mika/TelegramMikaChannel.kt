package com.ombhrum.fabushi.mika

import com.ombhrum.fabushi.telegram.InlineKeyboardButtonKind
import com.ombhrum.fabushi.telegram.InlineKeyboardButtonModel
import com.wangzan101.mika.core.action.ConfirmChoice
import com.wangzan101.mika.core.run.Channel
import com.wangzan101.mika.core.run.ChannelInput
import com.wangzan101.mika.core.run.ChannelInputSink
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.DeliveryRecord
import com.wangzan101.mika.core.run.MemoryOutboxStore
import com.wangzan101.mika.core.run.Outbox
import com.wangzan101.mika.core.run.RunId
import com.wangzan101.mika.core.run.SendOutcome
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow

data class MikaChannelCardEvent(
    val deliveryId: String,
    val peer: ChannelPeer,
    val threadId: String,
    val kind: Kind,
    val text: String,
    val askId: String? = null,
    val title: String? = null,
    val imageUri: String? = null,
    val imageName: String? = null,
    val imageSize: Long? = null,
    val inlineButtons: List<InlineKeyboardButtonModel> = emptyList(),
) {
    enum class Kind {
        PROGRESS,
        ASK_INPUT,
        ASK_CONFIRM,
        ASK_TAKEOVER,
        IMAGE,
        RESULT,
        NOTICE,
    }
}

data class PendingMikaAsk(
    val peer: ChannelPeer,
    val ask: Delivery.Ask,
)

/**
 * Pure-JVM Mika HostChannel adapter for Telegram chats (`id = "telegram"`).
 *
 * Implements [Channel] directly without any Android `Context` or `MikaApp.onCreate()` dependency
 * so it can register via `MikaApp.registerHostChannel(channel, owners)` in production AND be
 * driven directly against `:mika-core` `ChannelGateway`, `Outbox`, and `ConfirmationGate` in JVM
 * unit/E2E tests.
 */
class TelegramMikaChannel(
    private val outbox: Outbox = Outbox(MemoryOutboxStore()),
    private val aliveCheck: (ChannelPeer) -> Boolean = { true },
    private val onDeliveryEvent: suspend (MikaChannelCardEvent) -> Unit = {},
    private val onAcknowledge: (String) -> Unit = {},
) : Channel {
    companion object {
        const val ID = "telegram"
        const val ACTION_CONFIRM_APPROVE_PREFIX = "mika_confirm:approve:"
        const val ACTION_CONFIRM_DECLINE_PREFIX = "mika_confirm:decline:"
    }

    override val id: String = ID
    override val supportsImages: Boolean = true
    override val confirms: Boolean = true

    @Volatile private var sink: ChannelInputSink? = null
    private val closed = AtomicBoolean(false)
    private val pendingAsks = ConcurrentHashMap<String, PendingMikaAsk>()
    private val _pendingAsksFlow = MutableStateFlow<Map<String, PendingMikaAsk>>(emptyMap())
    val pendingAsksFlow: StateFlow<Map<String, PendingMikaAsk>> = _pendingAsksFlow.asStateFlow()

    private val _events = MutableSharedFlow<MikaChannelCardEvent>(replay = 0, extraBufferCapacity = 64)
    val events: SharedFlow<MikaChannelCardEvent> = _events.asSharedFlow()

    private val _deliveredEvents = CopyOnWriteArrayList<MikaChannelCardEvent>()
    val acknowledgedMessageIds: CopyOnWriteArrayList<String> = CopyOnWriteArrayList()

    override fun isAlive(peer: ChannelPeer): Boolean =
        !closed.get() && peer.channelId == id && aliveCheck(peer)

    override fun start(sink: ChannelInputSink): AutoCloseable {
        this.sink = sink
        return AutoCloseable {
            if (this.sink === sink) {
                this.sink = null
            }
        }
    }

    override fun acknowledge(messageId: String) {
        acknowledgedMessageIds.add(messageId)
        onAcknowledge(messageId)
    }

    override suspend fun send(peer: ChannelPeer, delivery: Delivery): SendOutcome {
        if (!isAlive(peer)) return SendOutcome.Failed
        val event = when (delivery) {
            is Delivery.Ask -> when (delivery.kind) {
                Delivery.Ask.Kind.Confirm -> {
                    pendingAsks[delivery.threadId] = PendingMikaAsk(peer, delivery)
                    refreshPendingAsksFlow()
                    MikaChannelCardEvent(
                        deliveryId = delivery.id,
                        peer = peer,
                        threadId = delivery.threadId,
                        kind = MikaChannelCardEvent.Kind.ASK_CONFIRM,
                        text = delivery.text,
                        askId = delivery.askId,
                        inlineButtons = listOf(
                            InlineKeyboardButtonModel(
                                id = "mika-approve-${delivery.askId}",
                                text = "✅ 批准",
                                kind = InlineKeyboardButtonKind.MIKA_ACTION,
                                payload = "$ACTION_CONFIRM_APPROVE_PREFIX${encodeConfirmSegment(delivery.threadId)}:${encodeConfirmSegment(delivery.askId)}",
                            ),
                            InlineKeyboardButtonModel(
                                id = "mika-decline-${delivery.askId}",
                                text = "❌ 拒绝",
                                kind = InlineKeyboardButtonKind.MIKA_ACTION,
                                payload = "$ACTION_CONFIRM_DECLINE_PREFIX${encodeConfirmSegment(delivery.threadId)}:${encodeConfirmSegment(delivery.askId)}",
                            ),
                        ),
                    )
                }
                Delivery.Ask.Kind.Input -> {
                    pendingAsks[delivery.threadId] = PendingMikaAsk(peer, delivery)
                    refreshPendingAsksFlow()
                    MikaChannelCardEvent(
                        deliveryId = delivery.id,
                        peer = peer,
                        threadId = delivery.threadId,
                        kind = MikaChannelCardEvent.Kind.ASK_INPUT,
                        text = delivery.text,
                        askId = delivery.askId,
                    )
                }
                Delivery.Ask.Kind.Takeover -> {
                    pendingAsks[delivery.threadId] = PendingMikaAsk(peer, delivery)
                    refreshPendingAsksFlow()
                    MikaChannelCardEvent(
                        deliveryId = delivery.id,
                        peer = peer,
                        threadId = delivery.threadId,
                        kind = MikaChannelCardEvent.Kind.ASK_TAKEOVER,
                        text = delivery.text,
                        askId = delivery.askId,
                        inlineButtons = listOf(
                            InlineKeyboardButtonModel(
                                id = "open-mika-${delivery.askId}",
                                text = "📱 打开 Mika 操控中心",
                                kind = InlineKeyboardButtonKind.MIKA_ACTION,
                                payload = "open_workspace",
                            ),
                            InlineKeyboardButtonModel(
                                id = "stop-mika-${delivery.askId}",
                                text = "⏹ 紧急停止",
                                kind = InlineKeyboardButtonKind.MIKA_ACTION,
                                payload = "stop_task",
                            ),
                        ),
                    )
                }
            }
            is Delivery.Result -> {
                pendingAsks.remove(delivery.threadId)
                refreshPendingAsksFlow()
                MikaChannelCardEvent(
                    deliveryId = delivery.id,
                    peer = peer,
                    threadId = delivery.threadId,
                    kind = MikaChannelCardEvent.Kind.RESULT,
                    text = delivery.text,
                    title = delivery.title,
                )
            }
            is Delivery.Image -> MikaChannelCardEvent(
                deliveryId = delivery.id,
                peer = peer,
                threadId = delivery.threadId,
                kind = MikaChannelCardEvent.Kind.IMAGE,
                text = delivery.name,
                imageUri = delivery.uri,
                imageName = delivery.name,
                imageSize = delivery.size,
            )
            is Delivery.Progress -> MikaChannelCardEvent(
                deliveryId = delivery.id,
                peer = peer,
                threadId = delivery.threadId,
                kind = MikaChannelCardEvent.Kind.PROGRESS,
                text = delivery.text,
            )
            is Delivery.Notice -> MikaChannelCardEvent(
                deliveryId = delivery.id,
                peer = peer,
                threadId = delivery.threadId,
                kind = MikaChannelCardEvent.Kind.NOTICE,
                text = delivery.text,
            )
        }

        _deliveredEvents.add(event)
        _events.tryEmit(event)
        return try {
            onDeliveryEvent(event)
            SendOutcome.Delivered
        } catch (ce: CancellationException) {
            throw ce
        } catch (_: Throwable) {
            SendOutcome.Uncertain
        }
    }

    suspend fun emitInput(input: ChannelInput): Boolean {
        val activeSink = sink ?: return false
        activeSink.receive(input)
        return true
    }

    suspend fun submitTask(peer: ChannelPeer, messageId: String, text: String): Boolean =
        emitInput(ChannelInput.Text(peer = peer, messageId = messageId, text = text))

    suspend fun answerAsk(
        peer: ChannelPeer,
        messageId: String,
        text: String,
        askId: String? = null,
    ): Boolean {
        val targetAskId = askId
            ?: pendingAsk(peer.threadId)?.takeIf { it.kind == Delivery.Ask.Kind.Input }?.askId
            ?: return false
        val dispatched = emitInput(
            ChannelInput.Text(
                peer = peer,
                messageId = messageId,
                text = text,
                replyToAskId = targetAskId,
                requireMatchingAsk = true,
            ),
        )
        if (dispatched) {
            val current = pendingAsks[peer.threadId]
            if (current != null && current.peer == peer && current.ask.askId == targetAskId) {
                if (pendingAsks.remove(peer.threadId, current)) {
                    refreshPendingAsksFlow()
                }
            }
        }
        return dispatched
    }

    suspend fun resolveConfirmation(peer: ChannelPeer, askId: String, approved: Boolean): Boolean {
        val choice = if (approved) ConfirmChoice.Once else ConfirmChoice.Deny
        val dispatched = emitInput(ChannelInput.Confirm(peer = peer, askId = askId, choice = choice))
        if (dispatched) {
            val current = pendingAsks[peer.threadId]
            if (current != null && current.peer == peer && current.ask.askId == askId) {
                if (pendingAsks.remove(peer.threadId, current)) {
                    refreshPendingAsksFlow()
                }
            }
        }
        return dispatched
    }

    suspend fun resolveConfirmationForThread(
        threadId: String,
        approved: Boolean,
        askId: String? = null,
    ): Boolean {
        val pending = pendingAsks[threadId] ?: return false
        if (pending.ask.kind != Delivery.Ask.Kind.Confirm) return false
        if (askId != null && pending.ask.askId != askId) return false
        return resolveConfirmation(pending.peer, pending.ask.askId, approved)
    }

    suspend fun stopRun(peer: ChannelPeer, runId: RunId): Boolean =
        emitInput(ChannelInput.Stop(peer = peer, runId = runId))

    suspend fun deliverViaOutbox(peer: ChannelPeer, delivery: Delivery): DeliveryRecord =
        outbox.deliver(this, peer, delivery)

    fun pendingAsk(threadId: String): Delivery.Ask? = pendingAsks[threadId]?.ask

    fun deliveredEvents(): List<MikaChannelCardEvent> = _deliveredEvents.toList()

    fun hasActiveSink(): Boolean = sink != null

    fun close() {
        closed.set(true)
        sink = null
        pendingAsks.clear()
        refreshPendingAsksFlow()
    }

    private fun encodeConfirmSegment(raw: String): String =
        URLEncoder.encode(raw, StandardCharsets.UTF_8.name())

    private fun refreshPendingAsksFlow() {
        _pendingAsksFlow.value = pendingAsks.toMap()
    }
}
