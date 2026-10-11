package com.ombhrum.fabushi.androidmain.coordinator
class AccountTransitionCleanup {
    private val actions = ArrayDeque<() -> Unit>()
    fun add(action: () -> Unit) { actions.addFirst(action) }
    fun runAll(): List<Throwable> {
        val failures = mutableListOf<Throwable>()
        while (actions.isNotEmpty()) runCatching { actions.removeFirst().invoke() }.exceptionOrNull()?.let(failures::add)
        return failures
    }
}
