package com.ombhrum.fabushi.androidpreload
data class VncLivenessCounters(val frames: Long, val inputEvents: Long, val observedAtMs: Long)
class VncLivenessDetector(private val staleAfterMs: Long = 5_000L) {
    private var last: VncLivenessCounters? = null
    fun observe(next: VncLivenessCounters): Boolean {
        val previous = last
        last = next
        if (previous == null) return true
        val progressed = next.frames > previous.frames || next.inputEvents > previous.inputEvents
        return progressed || next.observedAtMs - previous.observedAtMs < staleAfterMs
    }
}
