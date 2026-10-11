package com.ombhrum.fabushi

import java.net.URI

enum class CommandPaletteLinkStatus {
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    CANCELLED,
}

data class CommandPaletteLink(
    val url: String,
)

data class CommandPaletteLinkMetadata(
    val title: String? = null,
    val description: String? = null,
    val hostname: String? = null,
)

data class CommandPaletteLinkSnapshot(
    val status: CommandPaletteLinkStatus = CommandPaletteLinkStatus.IDLE,
    val value: List<CommandPaletteLink> = emptyList(),
    val metadata: Map<String, CommandPaletteLinkMetadata> = emptyMap(),
)

internal data class CommandPaletteLinkRequestScope(
    val accountSlot: String?,
    val conversationId: String?,
)

internal class CommandPaletteLinkRequestFence {
    private var generation = 0L
    private var scope: CommandPaletteLinkRequestScope? = null

    fun begin(scope: CommandPaletteLinkRequestScope): Long {
        generation += 1
        this.scope = scope
        return generation
    }

    fun cancel() {
        generation += 1
        scope = null
    }

    fun accepts(
        token: Long,
        currentScope: CommandPaletteLinkRequestScope,
    ): Boolean = generation == token && scope == currentScope
}

internal fun normalizeCommandPaletteLink(value: String): String? {
    val trimmed = value.trim()
    if (trimmed.isEmpty() || trimmed.length > 8_192) return null
    if (trimmed.any { it.code in 0..31 || it.code == 127 }) return null
    val parsed = runCatching { URI(trimmed) }.getOrNull() ?: return null
    val scheme = parsed.scheme?.lowercase()
    if (scheme !in setOf("http", "https") || parsed.host.isNullOrBlank()) return null
    return trimmed
}

internal fun commandPaletteLinkDisplayUrl(url: String): String {
    val parsed = runCatching { URI(url) }.getOrNull() ?: return url
    val host = parsed.host?.takeIf(String::isNotBlank) ?: return url
    val path = parsed.rawPath?.takeUnless { it.isBlank() || it == "/" }.orEmpty()
    return host + path
}

private val commandPaletteMarkdownLinkRegex =
    Regex("""\[[^\]]*]\(\s*([^\s)]+)(?:\s+[^)]*)?\)""")
private val commandPaletteBareLinkRegex =
    Regex("""https?://[^\s<>"'`()\]]+""", RegexOption.IGNORE_CASE)

private fun trimCommandPaletteBareLink(value: String): String {
    var candidate = value.trimEnd()
    while (candidate.lastOrNull() in setOf('.', ',', ';', ':', '!', '?')) {
        candidate = candidate.dropLast(1)
    }
    return candidate
}

internal fun commandPaletteLinksFromConversation(
    messageTexts: List<String>,
): List<CommandPaletteLink> {
    val seen = linkedSetOf<String>()
    val result = mutableListOf<CommandPaletteLink>()
    fun collect(raw: String) {
        val url = normalizeCommandPaletteLink(raw) ?: return
        if (seen.add(url)) result += CommandPaletteLink(url)
    }
    messageTexts.forEach { text ->
        commandPaletteMarkdownLinkRegex.findAll(text).forEach { match ->
            collect(match.groupValues.getOrElse(1) { "" })
        }
        commandPaletteBareLinkRegex.findAll(text).forEach { match ->
            collect(trimCommandPaletteBareLink(match.value))
        }
        collect(text)
    }
    return result
}

internal fun commandPaletteLinkMetadataProjection(
    snapshot: TranscriptLinkCardSnapshot,
): CommandPaletteLinkMetadata? {
    if (snapshot.status != TranscriptLinkCardStatus.READY) return null
    val metadata = snapshot.metadata ?: return null
    return CommandPaletteLinkMetadata(
        title = metadata.title,
        description = metadata.description,
        hostname = metadata.hostname,
    )
}

internal fun commandPaletteLinkEntries(
    links: List<CommandPaletteLink>,
    metadata: Map<String, CommandPaletteLinkMetadata>,
    onOpen: (CommandPaletteLink) -> Unit,
): List<CommandPaletteEntry> =
    links.map { link ->
        val preview = metadata[link.url]
        val title = preview?.title?.trim().takeUnless { it.isNullOrEmpty() }
        val display = commandPaletteLinkDisplayUrl(link.url)
        CommandPaletteEntry(
            id = "link:${link.url}",
            kind = CommandPaletteEntryKind.LINK,
            label = title ?: display,
            detail = display.takeIf { title != null },
            searchText = listOf(
                link.url,
                display,
                title.orEmpty(),
                preview?.description.orEmpty(),
                preview?.hostname.orEmpty(),
            ).joinToString(" "),
            activate = { onOpen(link) },
        )
    }
