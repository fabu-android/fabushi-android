package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject

internal data class TranscriptTipTapInline(
    val text: String,
    val bold: Boolean = false,
    val italic: Boolean = false,
    val strike: Boolean = false,
    val underline: Boolean = false,
    val code: Boolean = false,
    val url: String? = null,
)

internal sealed interface TranscriptTipTapBlock {
    data class Paragraph(val inline: List<TranscriptTipTapInline>) : TranscriptTipTapBlock
    data class Heading(val level: Int, val inline: List<TranscriptTipTapInline>) : TranscriptTipTapBlock
    data class Quote(val blocks: List<TranscriptTipTapBlock>) : TranscriptTipTapBlock
    data class ListBlock(
        val ordered: Boolean,
        val start: Int = 1,
        val items: List<List<TranscriptTipTapBlock>>,
    ) : TranscriptTipTapBlock
    data class Code(val text: String) : TranscriptTipTapBlock
    data object HorizontalRule : TranscriptTipTapBlock
}

/**
 * Read-only native projection of Desktop's persisted TipTap/ProseMirror user-message JSON.
 *
 * The Host-owned JSON remains canonical. This parser never mutates it, never executes HTML, and
 * ignores unsupported marks while preserving visible text. A malformed document returns null so
 * the caller can fall back to the canonical plain-text message.
 */
internal fun projectTranscriptTipTapDocument(value: String?): List<TranscriptTipTapBlock>? {
    if (value.isNullOrBlank()) return null
    val root = runCatching { JSONObject(value) }.getOrNull() ?: return null
    if (root.optString("type") != "doc") return null
    return parseTipTapBlockChildren(root.optJSONArray("content") ?: JSONArray())
}

private fun parseTipTapBlockChildren(content: JSONArray): List<TranscriptTipTapBlock> =
    buildList {
        for (index in 0 until content.length()) {
            val node = content.optJSONObject(index) ?: continue
            addAll(parseTipTapNode(node))
        }
    }

private fun parseTipTapNode(node: JSONObject): List<TranscriptTipTapBlock> {
    val type = node.optString("type")
    val children = node.optJSONArray("content") ?: JSONArray()
    return when (type) {
        "paragraph" -> listOf(TranscriptTipTapBlock.Paragraph(parseTipTapInlineChildren(children)))
        "heading" -> listOf(
            TranscriptTipTapBlock.Heading(
                level = node.optJSONObject("attrs")?.optInt("level", 1)?.coerceIn(1, 6) ?: 1,
                inline = parseTipTapInlineChildren(children),
            ),
        )
        "blockquote" -> listOf(TranscriptTipTapBlock.Quote(parseTipTapBlockChildren(children)))
        "bulletList", "orderedList" -> {
            val items = buildList {
                for (index in 0 until children.length()) {
                    val item = children.optJSONObject(index) ?: continue
                    if (item.optString("type") != "listItem") continue
                    add(parseTipTapBlockChildren(item.optJSONArray("content") ?: JSONArray()))
                }
            }
            listOf(
                TranscriptTipTapBlock.ListBlock(
                    ordered = type == "orderedList",
                    start = node.optJSONObject("attrs")?.optInt("start", 1)?.coerceAtLeast(1) ?: 1,
                    items = items,
                ),
            )
        }
        "listItem" -> parseTipTapBlockChildren(children)
        "codeBlock" -> listOf(TranscriptTipTapBlock.Code(flattenTipTapText(node)))
        "horizontalRule" -> listOf(TranscriptTipTapBlock.HorizontalRule)
        "hardBreak" -> listOf(TranscriptTipTapBlock.Paragraph(listOf(TranscriptTipTapInline("\n"))))
        "text", "mention", "workflowReference", "prReference" ->
            listOf(TranscriptTipTapBlock.Paragraph(listOfNotNull(parseTipTapInlineNode(node))))
        "doc" -> parseTipTapBlockChildren(children)
        else -> {
            // Desktop's renderer falls through to children for unknown container nodes.
            val blocks = parseTipTapBlockChildren(children)
            if (blocks.isNotEmpty()) blocks else {
                parseTipTapInlineNode(node)
                    ?.let { listOf(TranscriptTipTapBlock.Paragraph(listOf(it))) }
                    ?: emptyList()
            }
        }
    }
}

private fun parseTipTapInlineChildren(content: JSONArray): List<TranscriptTipTapInline> =
    buildList {
        for (index in 0 until content.length()) {
            val node = content.optJSONObject(index) ?: continue
            if (node.optString("type") == "hardBreak") {
                add(TranscriptTipTapInline("\n"))
                continue
            }
            parseTipTapInlineNode(node)?.let(::add)
            if (node.optString("type") !in setOf("text", "mention", "workflowReference", "prReference", "hardBreak")) {
                addAll(parseTipTapInlineChildren(node.optJSONArray("content") ?: JSONArray()))
            }
        }
    }

private fun parseTipTapInlineNode(node: JSONObject): TranscriptTipTapInline? {
    val type = node.optString("type")
    val baseText = when (type) {
        "text" -> node.optString("text")
        "mention" -> {
            val attrs = node.optJSONObject("attrs") ?: JSONObject()
            "@" + attrs.optString("label").ifBlank { attrs.optString("id") }
        }
        "workflowReference" -> "@" + (node.optJSONObject("attrs")?.optString("label").orEmpty())
        "prReference" -> {
            val number = node.optJSONObject("attrs")?.optInt("prNumber", -1) ?: -1
            if (number > 0) "#$number" else "#"
        }
        else -> return null
    }
    if (baseText.isEmpty() && type == "text") return TranscriptTipTapInline("")

    var projected = TranscriptTipTapInline(text = baseText)
    val marks = node.optJSONArray("marks") ?: JSONArray()
    for (index in 0 until marks.length()) {
        val mark = marks.optJSONObject(index) ?: continue
        projected = when (mark.optString("type")) {
            "bold" -> projected.copy(bold = true)
            "italic" -> projected.copy(italic = true)
            "strike" -> projected.copy(strike = true)
            "underline" -> projected.copy(underline = true)
            "code" -> projected.copy(code = true)
            "link" -> {
                val href = mark.optJSONObject("attrs")?.optString("href").orEmpty()
                projected.copy(url = normalizeTranscriptHttpUrl(href))
            }
            else -> projected
        }
    }
    return projected
}

private fun flattenTipTapText(node: JSONObject): String {
    val out = StringBuilder()
    fun visit(current: JSONObject) {
        when (current.optString("type")) {
            "text" -> out.append(current.optString("text"))
            "hardBreak" -> out.append('\n')
            "mention" -> {
                val attrs = current.optJSONObject("attrs") ?: JSONObject()
                out.append('@').append(attrs.optString("label").ifBlank { attrs.optString("id") })
            }
            "workflowReference" -> out.append('@').append(current.optJSONObject("attrs")?.optString("label").orEmpty())
            "prReference" -> {
                val number = current.optJSONObject("attrs")?.optInt("prNumber", -1) ?: -1
                out.append(if (number > 0) "#$number" else "#")
            }
        }
        val children = current.optJSONArray("content") ?: return
        for (index in 0 until children.length()) {
            children.optJSONObject(index)?.let(::visit)
        }
    }
    visit(node)
    return out.toString()
}
