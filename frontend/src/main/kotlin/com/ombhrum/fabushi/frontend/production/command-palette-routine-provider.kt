package com.ombhrum.fabushi

import org.json.JSONArray

enum class CommandPaletteRoutineStatus {
    UNAVAILABLE,
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    CANCELLED,
}

data class CommandPaletteRoutine(
    val agentId: String,
    val automationId: String,
    val name: String,
    val triggerDescription: String,
    val createdAtMs: Long,
    val lastRunAtMs: Long?,
)

data class CommandPaletteRoutineSnapshot(
    val status: CommandPaletteRoutineStatus = CommandPaletteRoutineStatus.IDLE,
    val value: List<CommandPaletteRoutine> = emptyList(),
)

internal class CommandPaletteRoutineRequestFence {
    private var generation = 0L

    fun begin(): Long {
        generation += 1
        return generation
    }

    fun cancel() {
        generation += 1
    }

    fun accepts(token: Long): Boolean = token == generation
}

internal fun commandPaletteRoutinesFromAutomationList(
    raw: JSONArray,
): List<CommandPaletteRoutine> {
    val routines = mutableListOf<CommandPaletteRoutine>()
    val identities = mutableSetOf<String>()
    for (index in 0 until raw.length()) {
        val item = raw.optJSONObject(index) ?: continue
        val agentId = item.optString("agent_id")
            .ifBlank { item.optString("agentId") }
            .trim()
        val automationId = item.optString("id").trim()
        val name = item.optString("name").trim()
        val schedule = item.optString("schedule").trim()
        val triggerDescription = item.optString("trigger_description")
            .ifBlank { item.optString("triggerDescription") }
            .ifBlank { schedule }
            .trim()
        if (
            agentId.isBlank() ||
            automationId.isBlank() ||
            name.isBlank() ||
            triggerDescription.isBlank()
        ) {
            continue
        }
        val createdAtMs = when {
            item.has("created_at_ms") -> item.optLong("created_at_ms", -1L)
            else -> item.optLong("createdAtMs", -1L)
        }
        if (createdAtMs < 0L) continue
        val lastRunAtMs = when {
            item.has("last_run_at_ms") && !item.isNull("last_run_at_ms") ->
                item.optLong("last_run_at_ms", -1L).takeIf { it >= 0L }
            item.has("lastRunAtMs") && !item.isNull("lastRunAtMs") ->
                item.optLong("lastRunAtMs", -1L).takeIf { it >= 0L }
            else -> null
        }
        val identity = "$agentId\u0000$automationId"
        if (!identities.add(identity)) continue
        routines += CommandPaletteRoutine(
            agentId = agentId,
            automationId = automationId,
            name = name,
            triggerDescription = triggerDescription,
            createdAtMs = createdAtMs,
            lastRunAtMs = lastRunAtMs,
        )
    }
    return routines
}

internal fun commandPaletteRoutineEntries(
    routines: List<CommandPaletteRoutine>,
    agentNames: Map<String, String>,
    onOpenAgent: (String) -> Unit,
): List<CommandPaletteEntry> =
    routines.mapNotNull { routine ->
        val agentName = agentNames[routine.agentId]?.takeIf(String::isNotBlank)
            ?: return@mapNotNull null
        CommandPaletteEntry(
            id = "routine:${routine.agentId}:${routine.automationId}",
            kind = CommandPaletteEntryKind.ROUTINE,
            label = routine.name,
            detail = "${routine.triggerDescription} · $agentName",
            searchText = "${routine.name} ${routine.triggerDescription} $agentName",
            activate = { onOpenAgent(routine.agentId) },
        )
    }
