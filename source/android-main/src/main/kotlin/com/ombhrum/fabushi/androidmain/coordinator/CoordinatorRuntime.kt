package com.ombhrum.fabushi.androidmain.coordinator
interface CoordinatorEpochStore { fun read(): Long; fun write(value: Long) }
class InMemoryCoordinatorEpochStore(initial: Long = 0) : CoordinatorEpochStore {
    private var value = initial
    override fun read(): Long = value
    override fun write(value: Long) { this.value = value }
}
sealed interface CoordinatorProcessState {
    data object Stopped : CoordinatorProcessState
    data class Running(val generation: Long) : CoordinatorProcessState
    data class Crashed(val generation: Long, val reason: String) : CoordinatorProcessState
}
class CoordinatorProcessRuntime(private val epochs: CoordinatorEpochStore) {
    var state: CoordinatorProcessState = CoordinatorProcessState.Stopped
        private set
    fun start(): Long {
        val next = epochs.read().coerceAtLeast(0) + 1
        epochs.write(next)
        state = CoordinatorProcessState.Running(next)
        return next
    }
    fun crash(reason: String) {
        val generation = (state as? CoordinatorProcessState.Running)?.generation ?: epochs.read()
        state = CoordinatorProcessState.Crashed(generation, reason)
    }
    fun currentGeneration(): Long = when (val current = state) {
        is CoordinatorProcessState.Running -> current.generation
        is CoordinatorProcessState.Crashed -> current.generation
        CoordinatorProcessState.Stopped -> epochs.read()
    }
    fun isStale(generation: Long): Boolean = generation < currentGeneration()
}
