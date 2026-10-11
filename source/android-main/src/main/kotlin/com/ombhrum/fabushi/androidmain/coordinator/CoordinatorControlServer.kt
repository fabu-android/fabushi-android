package com.ombhrum.fabushi.androidmain.coordinator
class CoordinatorControlServer {
    private val executors = linkedMapOf<String, (String) -> String>()
    private val eventFamilies = linkedSetOf<String>()
    fun registerCommand(name: String, executor: (String) -> String) {
        require(name.isNotBlank())
        check(executors.put(name, executor) == null) { "duplicate coordinator command: " + name }
    }
    fun allowEventFamily(family: String) { require(family.isNotBlank()); eventFamilies += family }
    fun dispatch(name: String, payload: String): Result<String> =
        executors[name]?.let { runCatching { it(payload) } } ?: Result.failure(IllegalArgumentException("unknown coordinator command: " + name))
    fun acceptsEventFamily(family: String): Boolean = family in eventFamilies
}
