package com.ombhrum.fabushi

import androidx.annotation.MainThread
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Native App MCP semantic surface for the Android shell.
 *
 * The surface deliberately exposes stable semantic identifiers and a bounded
 * action allowlist rather than screenshots, arbitrary Kotlin/Java reflection,
 * JavaScript, or shell execution. A future/native device transport can publish
 * these same tools without changing the UI contract.
 */
@MainThread
class FabushiAppAgentSurface(
    private val appId: String = "fabushi.android",
) {
    companion object {
        const val Version = 1
        const val StatusTool = "fabushi.app.status"
        const val SnapshotTool = "fabushi.app.snapshot"
        const val FindTool = "fabushi.app.find"
        const val ActionTool = "fabushi.app.action"
        const val WaitTool = "fabushi.app.wait"
        const val AssertTool = "fabushi.app.assert"
        val ToolNames = listOf(StatusTool, SnapshotTool, FindTool, ActionTool, WaitTool, AssertTool)
    }

    data class Element(
        val agentId: String,
        val role: String,
        val name: String,
        val visible: Boolean = true,
        val enabled: Boolean = true,
        val sensitive: Boolean = false,
        val valuePresent: Boolean? = null,
        val valueLength: Int? = null,
    )

    data class Snapshot(
        val version: Int = Version,
        val appId: String,
        val platform: String = "android",
        val screen: String,
        val generation: Long,
        val elements: List<Element>,
    )

    data class Status(
        val version: Int = Version,
        val appId: String,
        val platform: String = "android",
        val available: Boolean,
        val screen: String,
        val generation: Long,
    )

    data class Assertion(
        val passed: Boolean,
        val screen: String,
        val generation: Long,
        val matches: List<Element>,
        val failures: List<String>,
    )

    class Action(
        val allowed: Set<String>,
        val invoke: (String?) -> Unit,
    )

    private data class Overlay(
        val elements: List<Element>,
        val actions: Map<String, Action>,
    )

    private val lock = Any()
    private var generation = 0L
    private var screen = "unavailable"
    private var baseElements = emptyList<Element>()
    private var baseActions = emptyMap<String, Action>()
    private val overlays = linkedMapOf<String, Overlay>()
    private var elements = emptyList<Element>()
    private var actions = emptyMap<String, Action>()

    fun publish(
        screen: String,
        elements: List<Element>,
        actions: Map<String, Action> = emptyMap(),
    ): Snapshot = synchronized(lock) {
        require(screen.isNotBlank() && screen.length <= 160) { "invalid_app_surface_screen" }
        validateElements(elements, actions)
        this.screen = screen
        this.baseElements = elements.toList()
        this.baseActions = actions.toMap()
        rebuildLocked()
        advanceGenerationLocked()
        snapshotLocked()
    }

    /**
     * Adds a small app-shell-owned semantic overlay without replacing the active screen contract.
     * This is used for cross-surface controls such as a Mini App Bot's canonical "打开应用" menu
     * button while preserving the Bot chat's draft/send/stop actions.
     */
    fun setOverlay(
        key: String,
        elements: List<Element>,
        actions: Map<String, Action> = emptyMap(),
    ): Snapshot = synchronized(lock) {
        require(key.matches(Regex("[A-Za-z0-9._:-]{1,80}"))) { "invalid_app_surface_overlay_key" }
        validateElements(elements, actions)
        overlays[key] = Overlay(elements.toList(), actions.toMap())
        rebuildLocked()
        advanceGenerationLocked()
        snapshotLocked()
    }

    fun clearOverlay(key: String): Snapshot = synchronized(lock) {
        if (overlays.remove(key) != null) {
            rebuildLocked()
            advanceGenerationLocked()
        }
        snapshotLocked()
    }

    fun clear() = synchronized(lock) {
        advanceGenerationLocked()
        screen = "unavailable"
        baseElements = emptyList()
        baseActions = emptyMap()
        overlays.clear()
        elements = emptyList()
        actions = emptyMap()
    }

    fun status(): Status = synchronized(lock) {
        Status(
            appId = appId,
            available = screen != "unavailable",
            screen = screen,
            generation = generation,
        )
    }

    fun snapshot(): Snapshot = synchronized(lock) { snapshotLocked() }

    fun find(
        agentId: String? = null,
        role: String? = null,
        name: String? = null,
        limit: Int = 25,
    ): List<Element> = synchronized(lock) {
        elements.asSequence()
            .filter { agentId.isNullOrBlank() || it.agentId == agentId }
            .filter { role.isNullOrBlank() || it.role.equals(role, ignoreCase = true) }
            .filter { name.isNullOrBlank() || it.name.contains(name, ignoreCase = true) }
            .take(limit.coerceIn(1, 100))
            .toList()
    }

    fun assertState(
        expectedScreen: String? = null,
        agentId: String? = null,
        role: String? = null,
        name: String? = null,
        state: String = "present",
    ): Assertion = synchronized(lock) {
        val matches = elements.asSequence()
            .filter { agentId.isNullOrBlank() || it.agentId == agentId }
            .filter { role.isNullOrBlank() || it.role.equals(role, ignoreCase = true) }
            .filter { name.isNullOrBlank() || it.name.contains(name, ignoreCase = true) }
            .take(100)
            .toList()
        val failures = mutableListOf<String>()
        if (!expectedScreen.isNullOrBlank() && screen != expectedScreen) {
            failures += "screen expected $expectedScreen, actual $screen"
        }
        val statePassed = when (state) {
            "absent" -> matches.isEmpty()
            "enabled" -> matches.any { it.enabled }
            "disabled" -> matches.any { !it.enabled }
            "visible" -> matches.any { it.visible }
            "hidden" -> matches.any { !it.visible }
            else -> if (agentId == null && role == null && name == null) true else matches.isNotEmpty()
        }
        if (!statePassed) failures += "element state $state was not satisfied"
        Assertion(
            passed = failures.isEmpty(),
            screen = screen,
            generation = generation,
            matches = matches,
            failures = failures,
        )
    }

    suspend fun waitFor(
        expectedScreen: String? = null,
        agentId: String? = null,
        role: String? = null,
        name: String? = null,
        state: String = "present",
        timeoutMilliseconds: Long = 10_000,
    ): Assertion {
        val bounded = timeoutMilliseconds.coerceIn(100, 30_000)
        val satisfied = withTimeoutOrNull(bounded) {
            var result = assertState(expectedScreen, agentId, role, name, state)
            while (!result.passed) {
                delay(100)
                result = assertState(expectedScreen, agentId, role, name, state)
            }
            result
        }
        return satisfied ?: assertState(expectedScreen, agentId, role, name, state)
    }

    fun action(
        expectedGeneration: Long,
        agentId: String,
        action: String,
        value: String? = null,
    ): Snapshot {
        val callback = synchronized(lock) {
            require(expectedGeneration == generation) { "stale_app_surface_generation" }
            val element = elements.firstOrNull { it.agentId == agentId }
                ?: error("app_surface_element_not_found")
            require(element.visible) { "app_surface_target_hidden" }
            require(element.enabled) { "app_surface_target_disabled" }
            require(!(element.sensitive && value != null)) { "sensitive_app_surface_input_requires_secure_input" }
            require(value == null || value.length <= 20_000) { "app_surface_value_too_large" }
            val binding = actions[agentId] ?: error("app_surface_action_unavailable")
            require(action in binding.allowed) { "unsupported_app_surface_action" }
            binding.invoke
        }
        callback(value)
        return synchronized(lock) {
            advanceGenerationLocked()
            snapshotLocked()
        }
    }

    private fun validateElements(elements: List<Element>, actions: Map<String, Action>) {
        require(elements.size <= 500) { "app_surface_element_limit" }
        val ids = HashSet<String>()
        elements.forEach { element ->
            require(element.agentId.matches(Regex("[A-Za-z0-9._:/@-]{1,200}"))) { "invalid_app_surface_agent_id" }
            require(ids.add(element.agentId)) { "duplicate_app_surface_agent_id" }
            require(element.role.length <= 80 && element.name.length <= 240) { "invalid_app_surface_element" }
        }
        require(actions.keys.all(ids::contains)) { "app_surface_action_target_missing" }
    }

    private fun rebuildLocked() {
        val mergedElements = ArrayList<Element>(baseElements.size + overlays.values.sumOf { it.elements.size })
        val mergedActions = linkedMapOf<String, Action>()
        val ids = HashSet<String>()
        fun add(rows: List<Element>, rowActions: Map<String, Action>) {
            rows.forEach { element ->
                require(ids.add(element.agentId)) { "duplicate_app_surface_agent_id" }
                mergedElements += element
            }
            rowActions.forEach { (id, action) ->
                require(id in ids) { "app_surface_action_target_missing" }
                mergedActions[id] = action
            }
        }
        add(baseElements, baseActions)
        overlays.values.forEach { overlay -> add(overlay.elements, overlay.actions) }
        require(mergedElements.size <= 500) { "app_surface_element_limit" }
        elements = mergedElements
        actions = mergedActions
    }

    private fun advanceGenerationLocked() {
        generation = if (generation == Long.MAX_VALUE) 1 else generation + 1
    }

    private fun snapshotLocked() = Snapshot(
        appId = appId,
        screen = screen,
        generation = generation,
        elements = elements.toList(),
    )
}
