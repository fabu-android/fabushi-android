package com.ombhrum.fabushi.androidmain.remote

import android.content.Context
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import org.json.JSONArray
import org.json.JSONObject
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpReceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoTrack
import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

internal data class RemoteComputerDataPlaneFence(
    val deviceId: String,
    val sessionId: String,
    val processGeneration: Long,
    val viewportRevision: Long,
    val humanTakeover: Boolean,
    val lifecycle: String,
)

internal object RemoteComputerDataPlanePolicy {
    fun canSendInput(
        active: RemoteComputerDataPlaneFence?,
        current: RemoteComputerDataPlaneFence,
    ): Boolean =
        active != null &&
            active.deviceId == current.deviceId &&
            active.sessionId == current.sessionId &&
            active.processGeneration == current.processGeneration &&
            active.viewportRevision == current.viewportRevision &&
            current.humanTakeover &&
            current.lifecycle !in setOf("closing", "outcome_unknown")

    fun pointerEnvelope(
        fence: RemoteComputerDataPlaneFence,
        action: String,
        normalizedX: Float,
        normalizedY: Float,
        buttonState: Int,
    ): JSONObject {
        require(action in setOf("down", "move", "up", "cancel")) { "Remote pointer action is invalid" }
        require(normalizedX in 0f..1f && normalizedY in 0f..1f) { "Remote pointer coordinate is invalid" }
        return JSONObject()
            .put("v", 1)
            .put("kind", "pointer")
            .put("eventId", UUID.randomUUID().toString())
            .put("processGeneration", fence.processGeneration)
            .put("viewportRevision", fence.viewportRevision)
            .put("action", action)
            .put("x", normalizedX.toDouble())
            .put("y", normalizedY.toDouble())
            .put("buttonState", buttonState)
    }

    fun keyEnvelope(
        fence: RemoteComputerDataPlaneFence,
        action: String,
        keyCode: Int,
        metaState: Int,
        repeatCount: Int,
    ): JSONObject {
        require(action in setOf("down", "up")) { "Remote key action is invalid" }
        require(keyCode >= 0 && repeatCount >= 0) { "Remote key event is invalid" }
        return JSONObject()
            .put("v", 1)
            .put("kind", "key")
            .put("eventId", UUID.randomUUID().toString())
            .put("processGeneration", fence.processGeneration)
            .put("viewportRevision", fence.viewportRevision)
            .put("action", action)
            .put("keyCode", keyCode)
            .put("metaState", metaState)
            .put("repeatCount", repeatCount)
    }
}

/**
 * The sole Android Remote Computer display/input data-plane owner.
 *
 * Control/session credentials never enter this class. The Coordinator supplies already-authorized
 * signaling callbacks and ICE configuration while this owner holds only one short-lived native
 * PeerConnection, one video sink and one ordered input DataChannel. Every asynchronous callback is
 * fenced by a monotonically increasing local connection generation and every input event is fenced
 * again by Coordinator-owned processGeneration/viewportRevision/humanTakeover state.
 */
internal class AndroidRemoteComputerDataPlane(
    context: Context,
    private val currentFence: () -> RemoteComputerDataPlaneFence?,
    private val sendSignal: (kind: String, payload: JSONObject) -> Unit,
    private val drainSignals: () -> JSONArray,
    private val acknowledgeSignals: (lastSignalId: Long) -> Unit,
    private val onRemoteClose: () -> Unit,
) : AutoCloseable {
    private val applicationContext = context.applicationContext
    private val eglBase = EglBase.create()
    private val factory: PeerConnectionFactory
    private val executor = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "fabushi-remote-computer-dataplane").apply { isDaemon = true }
    }
    private val connectionGeneration = AtomicLong(0L)

    @Volatile private var activeFence: RemoteComputerDataPlaneFence? = null
    @Volatile private var peerConnection: PeerConnection? = null
    @Volatile private var controlChannel: DataChannel? = null
    @Volatile private var renderer: SurfaceViewRenderer? = null
    @Volatile private var remoteVideoTrack: VideoTrack? = null
    @Volatile private var drainTask: ScheduledFuture<*>? = null
    @Volatile private var closed = false

    init {
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(applicationContext)
                .setEnableInternalTracer(false)
                .createInitializationOptions(),
        )
        factory = PeerConnectionFactory.builder()
            .setVideoEncoderFactory(
                DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true),
            )
            .setVideoDecoderFactory(
                DefaultVideoDecoderFactory(eglBase.eglBaseContext),
            )
            .createPeerConnectionFactory()
    }

    fun createViewport(context: Context): View {
        check(!closed) { "Remote Computer data plane is closed" }
        renderer?.let { return it }
        return SurfaceViewRenderer(context).also { view ->
            view.init(eglBase.eglBaseContext, null)
            view.setEnableHardwareScaler(true)
            view.setMirror(false)
            view.isFocusable = true
            view.isFocusableInTouchMode = true
            view.setOnTouchListener { _, event -> sendPointer(event) }
            view.setOnKeyListener { _, _, event -> sendKey(event) }
            remoteVideoTrack?.addSink(view)
            renderer = view
        }
    }

    @Synchronized
    fun connect(
        fence: RemoteComputerDataPlaneFence,
        iceServersJson: String,
    ) {
        check(!closed) { "Remote Computer data plane is closed" }
        require(fence.processGeneration > 0L) { "Remote Computer process generation is invalid" }
        require(fence.lifecycle !in setOf("closing", "outcome_unknown")) {
            "Remote Computer session cannot negotiate while closing"
        }
        disconnectInternal()
        activeFence = fence
        val generation = connectionGeneration.incrementAndGet()
        val rtcConfig = PeerConnection.RTCConfiguration(parseIceServers(iceServersJson)).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
        }
        val observer = object : BasePeerObserver() {
            override fun onIceCandidate(candidate: IceCandidate) {
                if (!isCurrent(generation)) return
                sendSignal(
                    "ice",
                    JSONObject()
                        .put("sdpMid", candidate.sdpMid ?: JSONObject.NULL)
                        .put("sdpMLineIndex", candidate.sdpMLineIndex)
                        .put("candidate", candidate.sdp),
                )
            }

            override fun onDataChannel(channel: DataChannel) {
                if (!isCurrent(generation)) {
                    channel.close()
                    return
                }
                // The mobile-created ordered channel is canonical. A second channel from the
                // desktop is rejected so there is never a second input owner.
                if (controlChannel == null) {
                    controlChannel = channel
                } else if (controlChannel !== channel) {
                    channel.close()
                }
            }

            override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream>) {
                if (!isCurrent(generation)) return
                (receiver.track() as? VideoTrack)?.let { track ->
                    remoteVideoTrack?.let { previous ->
                        renderer?.let(previous::removeSink)
                    }
                    remoteVideoTrack = track
                    renderer?.let(track::addSink)
                }
            }

            override fun onConnectionChange(newState: PeerConnection.PeerConnectionState) {
                if (!isCurrent(generation)) return
                if (newState == PeerConnection.PeerConnectionState.CONNECTED) {
                    val current = activeFence ?: return
                    sendSignal(
                        "ready",
                        JSONObject()
                            .put("processGeneration", current.processGeneration)
                            .put("viewportRevision", current.viewportRevision),
                    )
                }
                if (newState == PeerConnection.PeerConnectionState.FAILED ||
                    newState == PeerConnection.PeerConnectionState.CLOSED
                ) {
                    activeFence = null
                }
            }
        }
        val peer = factory.createPeerConnection(rtcConfig, observer)
            ?: error("Unable to create Remote Computer PeerConnection")
        peerConnection = peer
        controlChannel = peer.createDataChannel(
            "fabushi-control-v1",
            DataChannel.Init().apply {
                ordered = true
                negotiated = false
            },
        )
        peer.createOffer(
            object : BaseSdpObserver() {
                override fun onCreateSuccess(description: SessionDescription) {
                    if (!isCurrent(generation)) return
                    peer.setLocalDescription(
                        object : BaseSdpObserver() {
                            override fun onSetSuccess() {
                                if (!isCurrent(generation)) return
                                sendSignal(
                                    "offer",
                                    JSONObject()
                                        .put("type", "offer")
                                        .put("sdp", description.description)
                                        .put("processGeneration", fence.processGeneration)
                                        .put("viewportRevision", fence.viewportRevision),
                                )
                            }
                        },
                        description,
                    )
                }
            },
            MediaConstraints(),
        )
        drainTask = executor.scheduleWithFixedDelay(
            { runCatching { drainOnce(generation) } },
            0L,
            SIGNAL_DRAIN_PERIOD_MILLIS,
            TimeUnit.MILLISECONDS,
        )
    }

    @Synchronized
    fun updateFence(fence: RemoteComputerDataPlaneFence) {
        val active = activeFence ?: return
        if (active.deviceId == fence.deviceId &&
            active.sessionId == fence.sessionId &&
            active.processGeneration == fence.processGeneration
        ) {
            activeFence = fence
        }
    }

    fun disconnect() {
        executor.execute { disconnectInternal() }
    }

    private fun drainOnce(generation: Long) {
        if (!isCurrent(generation)) return
        val signals = drainSignals()
        var lastSignalId = 0L
        repeat(signals.length()) { index ->
            val signal = signals.getJSONObject(index)
            val signalId = signal.getLong("signalId")
            lastSignalId = maxOf(lastSignalId, signalId)
            when (signal.getString("kind")) {
                "answer" -> applyAnswer(generation, signal.getJSONObject("payload"))
                "ice" -> applyIce(generation, signal.getJSONObject("payload"))
                "ready" -> {
                    val fence = activeFence ?: return@repeat
                    sendSignal(
                        "ready",
                        JSONObject()
                            .put("processGeneration", fence.processGeneration)
                            .put("viewportRevision", fence.viewportRevision),
                    )
                }
                "close" -> {
                    disconnectInternal()
                    onRemoteClose()
                }
            }
        }
        if (lastSignalId > 0L && isCurrent(generation)) {
            acknowledgeSignals(lastSignalId)
        }
    }

    private fun applyAnswer(generation: Long, payload: JSONObject) {
        if (!isCurrent(generation)) return
        val sdp = payload.optString("sdp").trim()
        require(sdp.isNotEmpty() && sdp.length <= MAX_SDP_CHARS) { "Remote answer SDP is invalid" }
        peerConnection?.setRemoteDescription(
            BaseSdpObserver(),
            SessionDescription(SessionDescription.Type.ANSWER, sdp),
        )
    }

    private fun applyIce(generation: Long, payload: JSONObject) {
        if (!isCurrent(generation)) return
        val candidate = payload.optString("candidate").trim()
        require(candidate.isNotEmpty() && candidate.length <= MAX_ICE_CANDIDATE_CHARS) {
            "Remote ICE candidate is invalid"
        }
        val mid = payload.optString("sdpMid").takeIf(String::isNotBlank)
        val line = payload.optInt("sdpMLineIndex", 0)
        require(line >= 0) { "Remote ICE media-line index is invalid" }
        peerConnection?.addIceCandidate(IceCandidate(mid, line, candidate))
    }

    private fun sendPointer(event: MotionEvent): Boolean {
        val active = activeFence ?: return false
        val current = currentFence() ?: return false
        if (!RemoteComputerDataPlanePolicy.canSendInput(active, current)) return false
        val view = renderer ?: return false
        if (view.width <= 0 || view.height <= 0) return false
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> "down"
            MotionEvent.ACTION_MOVE -> "move"
            MotionEvent.ACTION_UP -> "up"
            MotionEvent.ACTION_CANCEL -> "cancel"
            else -> return false
        }
        val x = (event.x / view.width.toFloat()).coerceIn(0f, 1f)
        val y = (event.y / view.height.toFloat()).coerceIn(0f, 1f)
        return sendInput(
            RemoteComputerDataPlanePolicy.pointerEnvelope(
                current,
                action,
                x,
                y,
                event.buttonState,
            ),
        )
    }

    private fun sendKey(event: KeyEvent): Boolean {
        val active = activeFence ?: return false
        val current = currentFence() ?: return false
        if (!RemoteComputerDataPlanePolicy.canSendInput(active, current)) return false
        val action = when (event.action) {
            KeyEvent.ACTION_DOWN -> "down"
            KeyEvent.ACTION_UP -> "up"
            else -> return false
        }
        return sendInput(
            RemoteComputerDataPlanePolicy.keyEnvelope(
                current,
                action,
                event.keyCode,
                event.metaState,
                event.repeatCount,
            ),
        )
    }

    private fun sendInput(envelope: JSONObject): Boolean {
        val bytes = envelope.toString().toByteArray(StandardCharsets.UTF_8)
        if (bytes.size > MAX_INPUT_BYTES) return false
        val channel = controlChannel ?: return false
        if (channel.state() != DataChannel.State.OPEN) return false
        return channel.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), false))
    }

    @Synchronized
    private fun disconnectInternal() {
        connectionGeneration.incrementAndGet()
        drainTask?.cancel(false)
        drainTask = null
        controlChannel?.close()
        controlChannel?.dispose()
        controlChannel = null
        remoteVideoTrack?.let { track -> renderer?.let(track::removeSink) }
        remoteVideoTrack = null
        peerConnection?.close()
        peerConnection?.dispose()
        peerConnection = null
        activeFence = null
    }

    private fun isCurrent(generation: Long): Boolean =
        !closed && generation == connectionGeneration.get()

    override fun close() {
        if (closed) return
        closed = true
        disconnectInternal()
        renderer?.release()
        renderer = null
        factory.dispose()
        eglBase.release()
        executor.shutdownNow()
    }

    private fun parseIceServers(value: String): List<PeerConnection.IceServer> {
        val array = JSONArray(value)
        require(array.length() in 1..16) { "Remote Computer ICE server list is invalid" }
        return buildList {
            repeat(array.length()) { index ->
                val item = array.getJSONObject(index)
                val urlsJson = item.optJSONArray("urls")
                val urls = if (urlsJson != null) {
                    List(urlsJson.length()) { urlIndex -> urlsJson.getString(urlIndex) }
                } else {
                    listOf(item.getString("urls"))
                }.map(String::trim).filter(String::isNotEmpty)
                require(urls.isNotEmpty() && urls.size <= 8) { "Remote Computer ICE urls are invalid" }
                require(urls.all { url ->
                    url.length <= 512 &&
                        (url.startsWith("stun:") || url.startsWith("turn:") || url.startsWith("turns:"))
                }) { "Remote Computer ICE url is invalid" }
                val builder = PeerConnection.IceServer.builder(urls)
                item.optString("username").takeIf(String::isNotBlank)?.let(builder::setUsername)
                item.optString("credential").takeIf(String::isNotBlank)?.let(builder::setPassword)
                add(builder.createIceServer())
            }
        }
    }

    private open class BaseSdpObserver : SdpObserver {
        override fun onCreateSuccess(description: SessionDescription) {}
        override fun onSetSuccess() {}
        override fun onCreateFailure(error: String) {}
        override fun onSetFailure(error: String) {}
    }

    private open class BasePeerObserver : PeerConnection.Observer {
        override fun onSignalingChange(newState: PeerConnection.SignalingState) {}
        override fun onIceConnectionChange(newState: PeerConnection.IceConnectionState) {}
        override fun onIceConnectionReceivingChange(receiving: Boolean) {}
        override fun onIceGatheringChange(newState: PeerConnection.IceGatheringState) {}
        override fun onIceCandidate(candidate: IceCandidate) {}
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) {}
        override fun onAddStream(stream: MediaStream) {}
        override fun onRemoveStream(stream: MediaStream) {}
        override fun onDataChannel(channel: DataChannel) {}
        override fun onRenegotiationNeeded() {}
        override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream>) {}
        override fun onConnectionChange(newState: PeerConnection.PeerConnectionState) {}
    }

    private companion object {
        const val SIGNAL_DRAIN_PERIOD_MILLIS = 750L
        const val MAX_SDP_CHARS = 512 * 1024
        const val MAX_ICE_CANDIDATE_CHARS = 16 * 1024
        const val MAX_INPUT_BYTES = 16 * 1024
    }
}
