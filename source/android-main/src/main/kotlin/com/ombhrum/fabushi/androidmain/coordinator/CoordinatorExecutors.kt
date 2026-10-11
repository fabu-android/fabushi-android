package com.ombhrum.fabushi.androidmain.coordinator
class CoordinatorExecutors {
    private val executors = linkedMapOf<String, (String) -> String>()
    fun register(name: String, executor: (String) -> String) {
        require(name.isNotBlank())
        check(executors.put(name, executor) == null) { "executor already registered: " + name }
    }
    fun execute(name: String, payload: String): Result<String> =
        executors[name]?.let { runCatching { it(payload) } } ?: Result.failure(IllegalArgumentException("missing executor: " + name))
}
