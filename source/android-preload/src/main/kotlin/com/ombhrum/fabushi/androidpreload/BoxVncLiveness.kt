package com.ombhrum.fabushi.androidpreload

private const val DEFAULT_VNC_LIVENESS_WINDOW_MS = 10_000L
private const val DEFAULT_VNC_LIVENESS_MIN_IMPACTFUL_INPUTS = 3L

data class VncLivenessCounters(
    val frames: Long,
    val inputEvents: Long,
    val observedAtMs: Long,
    val inBytes: Long = 0L,
    val impactfulInputEvents: Long = inputEvents,
)

data class VncLivenessStall(
    val stallMs: Long,
    val impactfulInputs: Long,
    val inBytes: Long,
)

/**
 * Detects the Desktop VNC failure mode where user input continues but the remote display stops
 * making progress. Input is evidence of demand, never evidence that the remote session is alive.
 *
 * The detector fires once per stall episode after a full observation window contains enough
 * impactful inputs and no frame/inbound-byte progress. A frame or inbound-byte delta rearms the
 * detector for a future episode. Counter reset/reconnect rebaselines instead of reporting a stall.
 */
class VncLivenessDetector(
    private val staleAfterMs: Long = DEFAULT_VNC_LIVENESS_WINDOW_MS,
    private val minimumImpactfulInputs: Long = DEFAULT_VNC_LIVENESS_MIN_IMPACTFUL_INPUTS,
) {
    init {
        require(staleAfterMs > 0L) { "VNC liveness window must be positive" }
        require(minimumImpactfulInputs > 0L) { "VNC impactful input threshold must be positive" }
    }

    private data class Delta(
        val atMs: Long,
        val frameDelta: Long,
        val inputDelta: Long,
        val inboundByteDelta: Long,
    )

    private var last: VncLivenessCounters? = null
    private val deltas = ArrayDeque<Delta>()
    private var coveredSinceMs: Long? = null
    private var episodeFired = false
    private var stalled = false

    /**
     * Compatibility surface for the existing preload call sites: true means the session is not
     * currently classified as stalled. A stall remains false until actual remote progress or a
     * counter reset/reconnect rearms the detector.
     */
    fun observe(next: VncLivenessCounters): Boolean {
        sample(next)
        return !stalled
    }

    fun sample(next: VncLivenessCounters): VncLivenessStall? {
        require(next.frames >= 0L) { "VNC frame counter must be non-negative" }
        require(next.inputEvents >= 0L) { "VNC input counter must be non-negative" }
        require(next.inBytes >= 0L) { "VNC inbound byte counter must be non-negative" }
        require(next.impactfulInputEvents >= 0L) { "VNC impactful input counter must be non-negative" }
        require(next.observedAtMs >= 0L) { "VNC observation time must be non-negative" }

        val previous = last
        if (previous == null) {
            rebaseline(next)
            return null
        }

        val frameDelta = next.frames - previous.frames
        val inputDelta = next.impactfulInputEvents - previous.impactfulInputEvents
        val inboundByteDelta = next.inBytes - previous.inBytes
        val totalInputDelta = next.inputEvents - previous.inputEvents
        if (
            frameDelta < 0L ||
            inputDelta < 0L ||
            inboundByteDelta < 0L ||
            totalInputDelta < 0L ||
            next.observedAtMs < previous.observedAtMs
        ) {
            rebaseline(next)
            return null
        }
        last = next

        deltas.addLast(
            Delta(
                atMs = next.observedAtMs,
                frameDelta = frameDelta,
                inputDelta = inputDelta,
                inboundByteDelta = inboundByteDelta,
            ),
        )
        val windowStartMs = next.observedAtMs - staleAfterMs
        while (deltas.firstOrNull()?.atMs?.let { it <= windowStartMs } == true) {
            deltas.removeFirst()
        }

        if (frameDelta > 0L || inboundByteDelta > 0L) {
            episodeFired = false
            stalled = false
        }
        if (episodeFired) return null

        val coveredSince = coveredSinceMs ?: next.observedAtMs.also { coveredSinceMs = it }
        if (next.observedAtMs - coveredSince < staleAfterMs) return null

        val impactfulInputs = deltas.sumOf(Delta::inputDelta)
        if (impactfulInputs < minimumImpactfulInputs) return null

        val frameProgress = deltas.sumOf(Delta::frameDelta)
        val inboundBytes = deltas.sumOf(Delta::inboundByteDelta)
        if (frameProgress > 0L || inboundBytes > 0L) return null

        episodeFired = true
        stalled = true
        val oldestInputAtMs = deltas.firstOrNull { it.inputDelta > 0L }?.atMs ?: next.observedAtMs
        return VncLivenessStall(
            stallMs = (next.observedAtMs - oldestInputAtMs).coerceAtLeast(0L),
            impactfulInputs = impactfulInputs,
            inBytes = inboundBytes,
        )
    }

    fun reset() {
        last = null
        deltas.clear()
        coveredSinceMs = null
        episodeFired = false
        stalled = false
    }

    private fun rebaseline(next: VncLivenessCounters) {
        reset()
        last = next
        coveredSinceMs = next.observedAtMs
    }
}
