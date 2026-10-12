package com.ombhrum.fabushi.frontend.presentation

internal const val MOBILE_RENDER_STALL_THRESHOLD_MS = 1_000L

internal data class MobileRenderTtfrTelemetry(
    val operationId: String,
    val ttfrMs: Long,
    val chunkType: String,
    val enterEpochMs: Long,
)

internal data class MobileRenderStreamTelemetry(
    val operationId: String,
    val outcome: String,
    val chunkCount: Int,
    val maxGapMs: Long,
    val stalledMs: Long,
    val stallCount: Int,
    val streamMs: Long,
)

internal class MobileRenderTelemetryTracker(
    private val monotonicNowMs: () -> Long = { System.nanoTime() / 1_000_000L },
    private val epochNowMs: () -> Long = System::currentTimeMillis,
) {
    private data class Active(
        val operationId: String,
        val enterMonotonicMs: Long,
        val enterEpochMs: Long,
        var firstChunkMonotonicMs: Long? = null,
        var lastChunkMonotonicMs: Long? = null,
        var chunkCount: Int = 0,
        var maxGapMs: Long = 0L,
        var stalledMs: Long = 0L,
        var stallCount: Int = 0,
    )

    private var active: Active? = null

    fun begin(operationId: String) {
        val id = operationId.trim()
        if (id.isEmpty() || active?.operationId == id) return
        active = Active(
            operationId = id,
            enterMonotonicMs = monotonicNowMs(),
            enterEpochMs = epochNowMs().coerceAtLeast(0L),
        )
    }

    fun observeChunk(operationId: String, chunkType: String): MobileRenderTtfrTelemetry? {
        val current = active?.takeIf { it.operationId == operationId } ?: return null
        val now = monotonicNowMs()
        val previous = current.lastChunkMonotonicMs
        if (previous != null) {
            val gap = (now - previous).coerceAtLeast(0L)
            current.maxGapMs = maxOf(current.maxGapMs, gap)
            if (gap >= MOBILE_RENDER_STALL_THRESHOLD_MS) {
                current.stallCount += 1
                current.stalledMs += gap
            }
        }
        current.lastChunkMonotonicMs = now
        current.chunkCount += 1
        if (current.firstChunkMonotonicMs != null) return null
        current.firstChunkMonotonicMs = now
        return MobileRenderTtfrTelemetry(
            operationId = operationId,
            ttfrMs = (now - current.enterMonotonicMs).coerceAtLeast(0L),
            chunkType = chunkType,
            enterEpochMs = current.enterEpochMs,
        )
    }

    fun finish(operationId: String, outcome: String): MobileRenderStreamTelemetry? {
        val current = active?.takeIf { it.operationId == operationId } ?: return null
        active = null
        return MobileRenderStreamTelemetry(
            operationId = operationId,
            outcome = outcome,
            chunkCount = current.chunkCount,
            maxGapMs = current.maxGapMs,
            stalledMs = current.stalledMs,
            stallCount = current.stallCount,
            streamMs = (monotonicNowMs() - current.enterMonotonicMs).coerceAtLeast(0L),
        )
    }

    fun abandon(operationId: String) {
        if (active?.operationId == operationId) active = null
    }

    fun clear() {
        active = null
    }
}
