package com.ombhrum.fabushi.androidmain.coordinator
data class CoordinatorHandoffSample(val generation: Long, val stage: String, val elapsedMs: Long)
class CoordinatorHandoffTelemetry(private val limit: Int = 128) {
    private val samples = ArrayDeque<CoordinatorHandoffSample>()
    fun record(sample: CoordinatorHandoffSample) {
        if (samples.size >= limit.coerceAtLeast(1)) samples.removeFirst()
        samples.addLast(sample)
    }
    fun snapshot(): List<CoordinatorHandoffSample> = samples.toList()
}
