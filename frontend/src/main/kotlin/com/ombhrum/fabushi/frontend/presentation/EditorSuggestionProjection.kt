package com.ombhrum.fabushi

import org.json.JSONArray

enum class MobileEditorSuggestionStatus {
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    UNAVAILABLE,
    CANCELLED,
}

enum class MobileEditorSuggestionCategory {
    ASSISTANT,
    AUTOMATION,
    WORKFLOW,
}

data class MobileEditorSuggestion(
    val key: String,
    val id: String,
    val category: MobileEditorSuggestionCategory,
    val label: String,
    val subtitle: String? = null,
    val isGroup: Boolean = false,
)

data class MobileEditorSuggestionSnapshot(
    val status: MobileEditorSuggestionStatus = MobileEditorSuggestionStatus.IDLE,
    val rows: List<MobileEditorSuggestion> = emptyList(),
    val trigger: Char? = null,
    val query: String = "",
    val replaceFrom: Int = -1,
)

internal data class MobileEditorSuggestionQuery(
    val trigger: Char,
    val query: String,
    val replaceFrom: Int,
)

internal data class MobileEditorSuggestionRequestToken(
    val generation: Long,
    val accountSlot: String,
    val agentId: String,
)

internal class MobileEditorSuggestionRequestFence {
    private var generation = 0L
    private var disposed = false

    fun begin(accountSlot: String, agentId: String): MobileEditorSuggestionRequestToken? {
        if (disposed || accountSlot.isBlank() || agentId.isBlank()) return null
        generation += 1
        return MobileEditorSuggestionRequestToken(generation, accountSlot, agentId)
    }

    fun invalidate() {
        generation += 1
    }

    fun dispose() {
        disposed = true
        generation += 1
    }

    fun accepts(
        token: MobileEditorSuggestionRequestToken,
        accountSlot: String?,
        agentId: String?,
    ): Boolean =
        !disposed &&
            token.generation == generation &&
            token.accountSlot == accountSlot &&
            token.agentId == agentId
}

/**
 * Android mirrors Desktop's released suggestion producer boundary: the shipping adapter owns
 * Agent mentions, scheduled automations/workflows and emoji; PR/MCP provider modules exist in
 * Desktop source but are not mounted by its current production adapter.
 *
 * This matcher intentionally covers the current native plain-text editor boundary only. Typed
 * rich-text reference nodes are a separate workspace responsibility and must not be faked here.
 */
internal fun findMobileEditorSuggestionQuery(draft: String): MobileEditorSuggestionQuery? {
    if (draft.isEmpty()) return null
    val tailStart = (draft.length - 200).coerceAtLeast(0)
    val tail = draft.substring(tailStart)
    for (index in tail.lastIndex downTo 0) {
        val trigger = tail[index]
        if (trigger != '@' && trigger != '/') continue
        val previous = if (index == 0) null else tail[index - 1]
        if (previous != null && !previous.isWhitespace() && previous != '(') continue
        val query = tail.substring(index + 1)
        if (query.length > 50 || query.any { it == '\n' || it == '\r' || it == '@' || it == '/' }) return null
        return MobileEditorSuggestionQuery(
            trigger = trigger,
            query = query,
            replaceFrom = tailStart + index,
        )
    }
    return null
}

private fun editorSuggestionString(value: Any?): String? =
    (value as? String)?.trim()?.takeIf(String::isNotEmpty)

private fun normalizedEditorSuggestionText(value: String): String =
    value.lowercase()
        .replace(Regex("[^\\p{L}\\p{N}]+"), " ")
        .trim()

private fun fuzzyEditorSuggestionScore(value: String, token: String): Int? {
    if (token.isEmpty()) return 0
    val candidate = normalizedEditorSuggestionText(value)
    if (candidate.contains(token)) return 10_000 - candidate.indexOf(token) * 10 - candidate.length
    var cursor = 0
    var score = 0
    for (character in candidate) {
        if (cursor < token.length && character == token[cursor]) {
            cursor += 1
            score += 5
        } else if (cursor > 0) {
            score -= 1
        }
    }
    return if (cursor == token.length) score else null
}

private fun filterAndRankEditorSuggestions(
    rows: List<MobileEditorSuggestion>,
    query: String,
): List<MobileEditorSuggestion> {
    val normalizedQuery = normalizedEditorSuggestionText(query)
    if (normalizedQuery.isEmpty()) return rows.distinctBy(MobileEditorSuggestion::key)
    return rows
        .distinctBy(MobileEditorSuggestion::key)
        .mapNotNull { row ->
            val score = listOfNotNull(
                fuzzyEditorSuggestionScore(row.label, normalizedQuery),
                row.subtitle?.let { fuzzyEditorSuggestionScore(it, normalizedQuery) },
            ).maxOrNull()
            score?.let { row to it }
        }
        .sortedByDescending { it.second }
        .map { it.first }
}

internal fun projectMobileEditorSuggestions(
    agents: JSONArray,
    automations: JSONArray,
    activeAgentId: String,
    trigger: Char,
    query: String,
): List<MobileEditorSuggestion> {
    val assistants = buildList {
        val seen = linkedSetOf<String>()
        for (index in 0 until agents.length()) {
            val row = agents.optJSONObject(index) ?: continue
            val id = editorSuggestionString(row.opt("id")) ?: continue
            val label = editorSuggestionString(row.opt("name"))
                ?: editorSuggestionString(row.opt("displayName"))
                ?: continue
            if (
                row.optBoolean("isHiddenFromSidebar", false) ||
                row.optBoolean("hiddenFromSidebar", false) ||
                !seen.add(id)
            ) {
                continue
            }
            val isGroup = row.optBoolean("isGroup", false)
            val memberCount = row.optJSONArray("memberIds")?.length() ?: 0
            add(
                MobileEditorSuggestion(
                    key = "assistants:$id",
                    id = id,
                    category = MobileEditorSuggestionCategory.ASSISTANT,
                    label = label,
                    subtitle = if (isGroup && memberCount > 0) "$memberCount agents" else null,
                    isGroup = isGroup,
                ),
            )
        }
    }

    val automationRows = buildList {
        val seen = linkedSetOf<String>()
        for (index in 0 until automations.length()) {
            val row = automations.optJSONObject(index) ?: continue
            val id = editorSuggestionString(row.opt("id")) ?: continue
            val label = editorSuggestionString(row.opt("name")) ?: continue
            val owner = editorSuggestionString(row.opt("agent_id"))
                ?: editorSuggestionString(row.opt("agentId"))
                ?: continue
            if (owner != activeAgentId || !seen.add(id)) continue
            val schedule = editorSuggestionString(row.opt("schedule"))
            val triggerDescription = editorSuggestionString(row.opt("trigger_description"))
                ?: editorSuggestionString(row.opt("triggerDescription"))
            add(
                MobileEditorSuggestion(
                    key = "automations:$id",
                    id = id,
                    category = if (schedule == null) {
                        MobileEditorSuggestionCategory.WORKFLOW
                    } else {
                        MobileEditorSuggestionCategory.AUTOMATION
                    },
                    label = label,
                    subtitle = triggerDescription ?: schedule,
                ),
            )
        }
    }

    val rows = when (trigger) {
        '@' -> buildList {
            if (assistants.size >= 2) {
                add(
                    MobileEditorSuggestion(
                        key = "assistants:__everyone__",
                        id = "__everyone__",
                        category = MobileEditorSuggestionCategory.ASSISTANT,
                        label = "everyone",
                    ),
                )
            }
            addAll(assistants)
            addAll(automationRows.filter { it.category == MobileEditorSuggestionCategory.AUTOMATION })
        }
        '/' -> automationRows.filter { it.category == MobileEditorSuggestionCategory.WORKFLOW }
        else -> emptyList()
    }
    return filterAndRankEditorSuggestions(rows, query).take(96)
}

internal fun applyMobileEditorSuggestion(
    draft: String,
    snapshot: MobileEditorSuggestionSnapshot,
    suggestion: MobileEditorSuggestion,
): String {
    val trigger = snapshot.trigger ?: return draft
    if (snapshot.replaceFrom !in 0 until draft.length || draft[snapshot.replaceFrom] != trigger) return draft
    val prefix = draft.substring(0, snapshot.replaceFrom)
    val insertionTrigger = if (suggestion.category == MobileEditorSuggestionCategory.WORKFLOW) '/' else '@'
    return prefix + insertionTrigger + suggestion.label + " "
}
