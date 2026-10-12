package com.ombhrum.fabushi.androidmain.coordinator
data class ResyncCursor(val generation: Long, val sequence: Long)
class CoordinatorResyncTracker(initial: ResyncCursor = ResyncCursor(0, 0)) {
    var cursor: ResyncCursor = initial
        private set
    fun accept(generation: Long, sequence: Long): Boolean {
        if (generation < cursor.generation) return false
        if (generation == cursor.generation && sequence <= cursor.sequence) return false
        cursor = ResyncCursor(generation, sequence)
        return true
    }
    fun resetForGeneration(generation: Long) {
        require(generation >= cursor.generation)
        cursor = ResyncCursor(generation, 0)
    }
}
fun unionDisabledTools(vararg maps: Map<String, Set<String>>): Map<String, Set<String>> {
    val out = linkedMapOf<String, MutableSet<String>>()
    maps.forEach { map -> map.forEach { (server, tools) -> out.getOrPut(server) { linkedSetOf() }.addAll(tools) } }
    return out.mapValues { it.value.toSet() }
}
