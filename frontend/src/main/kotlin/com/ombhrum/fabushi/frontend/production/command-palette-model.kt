package com.ombhrum.fabushi

import java.text.Normalizer

internal enum class CommandPaletteTab {
    ALL, MESSAGES, AGENTS, GROUPS, FILES, LINKS, ROUTINES, ACTIONS
}

internal enum class CommandPaletteEntryKind {
    AGENT, GROUP, MESSAGE, FILE, LINK, ROUTINE, COMMAND
}

internal data class CommandPaletteEntry(
    val id: String,
    val kind: CommandPaletteEntryKind,
    val label: String,
    val detail: String? = null,
    val searchText: String = listOfNotNull(label, detail).joinToString(" "),
    val isHidden: Boolean = false,
    val activate: () -> Unit,
)

internal fun normalizePaletteSearch(value: String): String =
    Normalizer.normalize(value, Normalizer.Form.NFKD)
        .lowercase()
        .replace(Regex("\\p{M}+"), "")
        .replace(Regex("\\s+"), " ")
        .trim()

internal fun paletteSearchTokens(value: String): List<String> =
    normalizePaletteSearch(value).split(' ').filter(String::isNotBlank)

internal fun fuzzyPaletteScore(query: String, candidate: String): Int? {
    val tokens = paletteSearchTokens(query)
    if (tokens.isEmpty()) return 0
    val haystack = normalizePaletteSearch(candidate)
    var score = 0
    for (token in tokens) {
        val contiguous = haystack.indexOf(token)
        if (contiguous >= 0) {
            score += 1_000 - contiguous.coerceAtMost(900)
            continue
        }
        var cursor = 0
        var gaps = 0
        var matched = true
        for (ch in token) {
            val found = haystack.indexOf(ch, cursor)
            if (found < 0) {
                matched = false
                break
            }
            gaps += found - cursor
            cursor = found + 1
        }
        if (!matched) return null
        score += 200 - gaps.coerceAtMost(180)
    }
    return score
}

internal fun dedupeCommandPaletteEntries(
    entries: List<CommandPaletteEntry>,
): List<CommandPaletteEntry> {
    val deduped = mutableListOf<CommandPaletteEntry>()
    val indexById = mutableMapOf<String, Int>()
    for (entry in entries) {
        val existing = indexById[entry.id]
        if (existing == null) {
            indexById[entry.id] = deduped.size
            deduped += entry
        } else {
            // Preserve the canonical participant/command slot while projecting
            // the newest row. A replaced roster/search object must not create
            // a parallel result root with the same stable identity.
            deduped[existing] = entry
        }
    }
    return deduped
}

internal fun commandPaletteEntries(
    entries: List<CommandPaletteEntry>,
    tab: CommandPaletteTab,
    query: String,
): List<CommandPaletteEntry> {
    val canonicalEntries = dedupeCommandPaletteEntries(entries)
    val normalizedQuery = query.trim()
    val filteredByTab = canonicalEntries.filter { entry ->
        when (tab) {
            CommandPaletteTab.ALL -> true
            CommandPaletteTab.MESSAGES -> entry.kind == CommandPaletteEntryKind.MESSAGE
            CommandPaletteTab.AGENTS -> entry.kind == CommandPaletteEntryKind.AGENT
            CommandPaletteTab.GROUPS -> entry.kind == CommandPaletteEntryKind.GROUP
            CommandPaletteTab.FILES -> entry.kind == CommandPaletteEntryKind.FILE
            CommandPaletteTab.LINKS -> entry.kind == CommandPaletteEntryKind.LINK
            CommandPaletteTab.ROUTINES -> entry.kind == CommandPaletteEntryKind.ROUTINE
            CommandPaletteTab.ACTIONS -> entry.kind == CommandPaletteEntryKind.COMMAND
        }
    }
    if (normalizedQuery.isBlank()) {
        return if (tab == CommandPaletteTab.ALL) {
            filteredByTab.filter { entry ->
                !entry.isHidden && (
                    entry.kind == CommandPaletteEntryKind.AGENT ||
                        entry.kind == CommandPaletteEntryKind.GROUP ||
                        entry.kind == CommandPaletteEntryKind.COMMAND
                    )
            }
        } else {
            filteredByTab.filterNot(CommandPaletteEntry::isHidden)
        }
    }
    return filteredByTab.mapNotNull { entry ->
        fuzzyPaletteScore(normalizedQuery, entry.searchText)?.let { score -> score to entry }
    }.sortedByDescending { it.first }.map { it.second }
}

internal fun movePaletteHighlight(
    current: Int,
    delta: Int,
    size: Int,
): Int {
    if (size <= 0) return 0
    val bounded = current.coerceIn(0, size - 1)
    return (bounded + delta).coerceIn(0, size - 1)
}

internal fun cyclePaletteTab(
    current: CommandPaletteTab,
    delta: Int,
): CommandPaletteTab {
    val tabs = CommandPaletteTab.entries
    val currentIndex = tabs.indexOf(current)
    val next = ((currentIndex + delta) % tabs.size + tabs.size) % tabs.size
    return tabs[next]
}

internal fun activateCommandPaletteEntry(
    entries: List<CommandPaletteEntry>,
    highlightedIndex: Int,
): Boolean {
    val entry = entries.getOrNull(highlightedIndex) ?: return false
    entry.activate()
    return true
}
