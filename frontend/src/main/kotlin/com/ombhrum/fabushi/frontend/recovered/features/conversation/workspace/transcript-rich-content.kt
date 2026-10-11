package com.ombhrum.fabushi

/**
 * Native, presentation-only projection of Desktop assistant rich transcript content.
 *
 * Desktop authority:
 * frontend/src/recovered/features/conversation/workspace/transcript.tsx
 * @ 5cf2f33c394f885a37709a01361dc516b0fb4240.
 *
 * This parser deliberately treats markup as data. It does not execute HTML/JS and does not own
 * canonical transcript state.
 */
internal sealed interface TranscriptRichBlock {
    data class Paragraph(val text: String) : TranscriptRichBlock
    data class Heading(val level: Int, val text: String) : TranscriptRichBlock
    data class Quote(val text: String) : TranscriptRichBlock
    data class ListBlock(
        val ordered: Boolean,
        val start: Int = 1,
        val items: List<TranscriptRichListItem>,
    ) : TranscriptRichBlock
    data class Table(val headers: List<String>, val rows: List<List<String>>) : TranscriptRichBlock
    data class Math(
        val expression: String,
        val presentation: TranscriptSafeMathPresentation = projectTranscriptSafeMath(expression),
    ) : TranscriptRichBlock
    data class Mermaid(
        val code: String,
        val diagram: TranscriptMermaidDiagram?,
    ) : TranscriptRichBlock
    data class Code(val language: String, val code: String) : TranscriptRichBlock
    data object HorizontalRule : TranscriptRichBlock
}

internal data class TranscriptRichListItem(
    val text: String,
    val task: Boolean = false,
    val checked: Boolean = false,
)

internal data class TranscriptRichInline(
    val text: String,
    val bold: Boolean = false,
    val italic: Boolean = false,
    val strike: Boolean = false,
    val code: Boolean = false,
    val math: Boolean = false,
    val url: String? = null,
)

private val fencedCode = Regex("""```([^\r\n]*)\r?\n([\s\S]*?)\r?\n```""")
private val languageToken = Regex("""^[A-Za-z0-9_+\-]+$""")
private val headingLine = Regex("""^\s{0,3}(#{1,3})\s+(.+?)\s*$""")
private val quoteLine = Regex("""^\s*>\s?(.*)$""")
private val orderedLine = Regex("""^\s*(\d+)\.\s+(.+)$""")
private val unorderedLine = Regex("""^\s*[-+*]\s+(.+)$""")
private val taskItem = Regex("""^\[([ xX])]\s+(.+)$""")
private val horizontalRule = Regex("""^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$""")
private val mathFence = Regex("""^\s{0,3}\${'$'}\${'$'}\s*$""")
private val tableSeparatorCell = Regex("""^:?-{3,}:?$""")

internal fun projectTranscriptRichContent(text: String): List<TranscriptRichBlock> {
    val output = mutableListOf<TranscriptRichBlock>()
    var cursor = 0
    fencedCode.findAll(text).forEach { match ->
        if (match.range.first > cursor) {
            output += projectTranscriptRichText(text.substring(cursor, match.range.first))
        }
        val rawLanguage = match.groupValues[1].trim()
        val code = match.groupValues[2]
        output += if (rawLanguage.equals("mermaid", ignoreCase = true)) {
            TranscriptRichBlock.Mermaid(
                code = code,
                diagram = projectTranscriptMermaid(code),
            )
        } else {
            TranscriptRichBlock.Code(
                language = rawLanguage.takeIf(languageToken::matches).orEmpty(),
                code = code,
            )
        }
        cursor = match.range.last + 1
    }
    if (cursor < text.length || output.isEmpty()) {
        output += projectTranscriptRichText(text.substring(cursor))
    }
    return output
}

private fun projectTranscriptRichText(text: String): List<TranscriptRichBlock> {
    val blocks = mutableListOf<TranscriptRichBlock>()
    val paragraph = mutableListOf<String>()
    var activeListOrdered: Boolean? = null
    var activeListStart = 1
    val activeListItems = mutableListOf<TranscriptRichListItem>()
    val activeQuote = mutableListOf<String>()

    fun flushParagraph() {
        if (paragraph.any { it.trim().isNotEmpty() }) {
            blocks += TranscriptRichBlock.Paragraph(paragraph.joinToString("\n"))
        }
        paragraph.clear()
    }
    fun flushList() {
        val ordered = activeListOrdered ?: return
        blocks += TranscriptRichBlock.ListBlock(
            ordered = ordered,
            start = activeListStart,
            items = activeListItems.toList(),
        )
        activeListOrdered = null
        activeListStart = 1
        activeListItems.clear()
    }
    fun flushQuote() {
        if (activeQuote.isNotEmpty()) {
            blocks += TranscriptRichBlock.Quote(activeQuote.joinToString("\n"))
            activeQuote.clear()
        }
    }

    val lines = text.split(Regex("""\r?\n"""))
    var index = 0
    while (index < lines.size) {
        val line = lines[index]
        if (mathFence.matches(line)) {
            var closing = index + 1
            while (closing < lines.size && !mathFence.matches(lines[closing])) closing += 1
            if (closing < lines.size) {
                flushList()
                flushQuote()
                flushParagraph()
                blocks += TranscriptRichBlock.Math(lines.subList(index + 1, closing).joinToString("\n"))
                index = closing + 1
                continue
            }
        }

        val quote = quoteLine.matchEntire(line)
        if (quote != null) {
            flushList()
            flushParagraph()
            activeQuote += quote.groupValues[1]
            index += 1
            continue
        }
        flushQuote()

        val headers = splitTranscriptTableRow(line)
        val separator = lines.getOrNull(index + 1)?.let(::splitTranscriptTableRow)
        if (
            headers != null &&
            separator != null &&
            headers.size == separator.size &&
            separator.all(tableSeparatorCell::matches)
        ) {
            flushList()
            flushParagraph()
            val rows = mutableListOf<List<String>>()
            index += 2
            while (index < lines.size) {
                val row = splitTranscriptTableRow(lines[index])
                if (row == null || row.size != headers.size) break
                rows += row
                index += 1
            }
            blocks += TranscriptRichBlock.Table(headers, rows)
            continue
        }

        if (horizontalRule.matches(line)) {
            flushList()
            flushParagraph()
            blocks += TranscriptRichBlock.HorizontalRule
            index += 1
            continue
        }

        val heading = headingLine.matchEntire(line)
        if (heading != null) {
            flushList()
            flushParagraph()
            blocks += TranscriptRichBlock.Heading(
                level = heading.groupValues[1].length,
                text = heading.groupValues[2].trim(),
            )
            index += 1
            continue
        }

        val ordered = orderedLine.matchEntire(line)
        val unordered = unorderedLine.matchEntire(line)
        if (ordered != null || unordered != null) {
            val isOrdered = ordered != null
            if (activeListOrdered == null || activeListOrdered != isOrdered) {
                flushList()
                flushParagraph()
                activeListOrdered = isOrdered
                activeListStart = ordered?.groupValues?.get(1)?.toIntOrNull() ?: 1
            }
            val raw = (ordered?.groupValues?.get(2) ?: unordered!!.groupValues[1]).trim()
            val task = taskItem.matchEntire(raw)
            activeListItems += if (task == null) {
                TranscriptRichListItem(raw)
            } else {
                TranscriptRichListItem(
                    text = task.groupValues[2],
                    task = true,
                    checked = task.groupValues[1].equals("x", ignoreCase = true),
                )
            }
            index += 1
            continue
        }

        flushList()
        paragraph += line
        index += 1
    }

    flushQuote()
    flushList()
    flushParagraph()
    return blocks
}

private enum class TranscriptInlineMatchKind {
    MATH,
    MARKDOWN_LINK,
    BARE_LINK,
    BOLD,
    ITALIC,
    STRIKE,
    CODE,
}

private data class TranscriptInlineMatch(
    val kind: TranscriptInlineMatchKind,
    val match: MatchResult,
)

private val inlineMathPattern = Regex("""\\\(([^\\\n]*?)\\\)""")
private val markdownLinkPattern = Regex("""\[([^\]\n]+)]\((https?://[^\s)]+)\)""", RegexOption.IGNORE_CASE)
private val bareHttpLinkPattern = Regex("""https?://[^\s<>"\u0027]+""", RegexOption.IGNORE_CASE)
private val boldInlinePattern = Regex("""(\*\*|__)(?=\S)([^\n]*?\S)\1""")
private val italicInlinePattern = Regex("""(\*|_)(?=\S)([^\n]*?\S)\1""")
private val strikeInlinePattern = Regex("""~~(?=\S)([^\n]*?\S)~~""")
private val codeInlinePattern = Regex("""`([^`\n]+)`""")
private val bareUrlTrailingPunctuation = Regex("""[),.!?;:\]}]+$""")

private val transcriptInlinePatterns = listOf(
    TranscriptInlineMatchKind.MATH to inlineMathPattern,
    TranscriptInlineMatchKind.MARKDOWN_LINK to markdownLinkPattern,
    TranscriptInlineMatchKind.BARE_LINK to bareHttpLinkPattern,
    TranscriptInlineMatchKind.BOLD to boldInlinePattern,
    TranscriptInlineMatchKind.ITALIC to italicInlinePattern,
    TranscriptInlineMatchKind.STRIKE to strikeInlinePattern,
    TranscriptInlineMatchKind.CODE to codeInlinePattern,
)

private fun nextTranscriptInlineMatch(text: String, startIndex: Int): TranscriptInlineMatch? =
    transcriptInlinePatterns
        .mapNotNull { (kind, pattern) -> pattern.find(text, startIndex)?.let { TranscriptInlineMatch(kind, it) } }
        .minWithOrNull(
            compareBy<TranscriptInlineMatch> { it.match.range.first }
                .thenBy { it.kind.ordinal },
        )

/**
 * Presentation-only equivalent of Desktop renderAssistantInlineText. Markup remains inert data;
 * only validated HTTP(S) links are carried into the Compose projection.
 */
internal fun projectTranscriptInlineContent(text: String): List<TranscriptRichInline> {
    val output = mutableListOf<TranscriptRichInline>()
    var cursor = 0
    while (cursor < text.length) {
        val candidate = nextTranscriptInlineMatch(text, cursor) ?: break
        val match = candidate.match
        if (match.range.first > cursor) {
            output += TranscriptRichInline(text.substring(cursor, match.range.first))
        }
        when (candidate.kind) {
            TranscriptInlineMatchKind.MATH ->
                output += TranscriptRichInline(text = match.groupValues[1], math = true)
            TranscriptInlineMatchKind.MARKDOWN_LINK -> {
                val href = normalizeTranscriptHttpUrl(match.groupValues[2])
                output += if (href == null) {
                    TranscriptRichInline(match.value)
                } else {
                    TranscriptRichInline(text = match.groupValues[1], url = href)
                }
            }
            TranscriptInlineMatchKind.BARE_LINK -> {
                val trailing = bareUrlTrailingPunctuation.find(match.value)?.value.orEmpty()
                val hrefCandidate = match.value.dropLast(trailing.length)
                val href = normalizeTranscriptHttpUrl(hrefCandidate)
                if (href == null) {
                    output += TranscriptRichInline(match.value)
                } else {
                    output += TranscriptRichInline(text = href, url = href)
                    if (trailing.isNotEmpty()) output += TranscriptRichInline(trailing)
                }
            }
            TranscriptInlineMatchKind.BOLD ->
                output += projectTranscriptInlineContent(match.groupValues[2]).map { it.copy(bold = true) }
            TranscriptInlineMatchKind.ITALIC ->
                output += projectTranscriptInlineContent(match.groupValues[2]).map { it.copy(italic = true) }
            TranscriptInlineMatchKind.STRIKE ->
                output += projectTranscriptInlineContent(match.groupValues[1]).map { it.copy(strike = true) }
            TranscriptInlineMatchKind.CODE ->
                output += TranscriptRichInline(text = match.groupValues[1], code = true)
        }
        cursor = match.range.last + 1
    }
    if (cursor < text.length || output.isEmpty()) {
        output += TranscriptRichInline(text.substring(cursor))
    }
    return output
}
private fun splitTranscriptTableRow(line: String): List<String>? {
    val trimmed = line.trim()
    if ('|' !in trimmed) return null
    val noLeading = trimmed.removePrefix("|")
    val noTrailing = noLeading.removeSuffix("|")
    val cells = noTrailing.split('|').map(String::trim)
    return cells.takeIf { it.size > 1 }
}

internal fun normalizeTranscriptHttpUrl(value: String): String? {
    val trimmed = value.trim()
    if (trimmed.length > 8_192 || trimmed.any { char -> char.code in 0..31 || char.code == 127 }) {
        return null
    }
    val parsed = runCatching { java.net.URI(trimmed) }.getOrNull() ?: return null
    val scheme = parsed.scheme?.lowercase()
    if (scheme !in setOf("http", "https") || parsed.host.isNullOrBlank()) return null
    return trimmed
}
