package com.ombhrum.fabushi.androidmain.coordinator

/**
 * One-shot account-transition cleanup coordinator.
 *
 * Canonical account truth remains owned by the auth/account owners. This helper only guarantees
 * that every old-account adapter is fenced even when an earlier cleanup action fails.
 */
class AccountTransitionCleanup {
    private val actions = ArrayDeque<() -> Unit>()
    private var completed = false

    @Synchronized
    fun add(action: () -> Unit) {
        check(!completed) { "account transition cleanup already completed" }
        actions.addLast(action)
    }

    @Synchronized
    fun runAll(): List<Throwable> {
        if (completed) return emptyList()
        completed = true

        val failures = mutableListOf<Throwable>()
        while (actions.isNotEmpty()) {
            val action = actions.removeLast()
            try {
                action()
            } catch (failure: Throwable) {
                failures += failure
            }
        }
        return failures
    }
}
