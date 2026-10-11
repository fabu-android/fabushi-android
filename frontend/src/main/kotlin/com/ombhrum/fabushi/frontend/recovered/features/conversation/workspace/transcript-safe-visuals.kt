package com.ombhrum.fabushi

/**
 * Bounded native presentation for Desktop math.tsx and mermaid.tsx.
 *
 * Android never evaluates transcript math/diagram content as HTML or JavaScript. Math is rendered
 * from an inert, allow-listed TeX-like subset. Mermaid is limited to small graph/flowchart inputs
 * and projected into an immutable node/edge model for Compose.
 */
internal data class TranscriptSafeMathPresentation(
    val source: String,
    val displayText: String,
    val supported: Boolean,
)

private const val MAX_TRANSCRIPT_MATH_LENGTH = 4_096
private const val MAX_TRANSCRIPT_MERMAID_LENGTH = 16_384
private const val MAX_TRANSCRIPT_MERMAID_LINES = 256
private const val MAX_TRANSCRIPT_MERMAID_NODES = 64
private const val MAX_TRANSCRIPT_MERMAID_EDGES = 96

private val transcriptMathCommand = Regex("""\\([A-Za-z]+)""")
private val transcriptMathForbidden = Regex(
    """(?i)(?:\\(?:href|url|htmlclass|htmlid|htmlstyle|includegraphics|require|def|gdef|newcommand|renewcommand|write|input|include)\b|javascript:|data:|<\s*/?\s*(?:script|iframe|object|embed)\b)""",
)
private val transcriptMathAllowedCommands = setOf(
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi", "psi",
    "omega", "Gamma", "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma", "Upsilon", "Phi", "Psi",
    "Omega", "frac", "sqrt", "text", "operatorname", "mathrm", "mathbf", "mathit", "sin", "cos",
    "tan", "log", "ln", "exp", "min", "max", "lim", "sum", "prod", "int", "cdot", "times", "pm",
    "le", "leq", "ge", "geq", "ne", "neq", "in", "notin", "subset", "subseteq", "supset",
    "supseteq", "to", "rightarrow", "leftarrow", "leftrightarrow", "infty", "partial", "nabla",
    "left", "right", "begin", "end", "overline", "underline", "vec", "hat", "bar",
)

private val transcriptMathGlyphs = linkedMapOf(
    "\\\\times" to "×",
    "\\\\cdot" to "·",
    "\\\\pm" to "±",
    "\\\\leq" to "≤",
    "\\\\le" to "≤",
    "\\\\geq" to "≥",
    "\\\\ge" to "≥",
    "\\\\neq" to "≠",
    "\\\\ne" to "≠",
    "\\\\infty" to "∞",
    "\\\\rightarrow" to "→",
    "\\\\to" to "→",
    "\\\\leftarrow" to "←",
    "\\\\leftrightarrow" to "↔",
    "\\\\alpha" to "α",
    "\\\\beta" to "β",
    "\\\\gamma" to "γ",
    "\\\\delta" to "δ",
    "\\\\theta" to "θ",
    "\\\\lambda" to "λ",
    "\\\\mu" to "μ",
    "\\\\pi" to "π",
    "\\\\rho" to "ρ",
    "\\\\sigma" to "σ",
    "\\\\phi" to "φ",
    "\\\\omega" to "ω",
    "\\\\sum" to "∑",
    "\\\\prod" to "∏",
    "\\\\int" to "∫",
)

internal fun projectTranscriptSafeMath(expression: String): TranscriptSafeMathPresentation {
    val source = expression.trim()
    if (
        source.isEmpty() ||
        source.length > MAX_TRANSCRIPT_MATH_LENGTH ||
        source.any { it.code in 0..8 || it.code in 11..31 || it.code == 127 } ||
        transcriptMathForbidden.containsMatchIn(source)
    ) {
        return TranscriptSafeMathPresentation(source, source, supported = false)
    }
    val unsupportedCommand = transcriptMathCommand
        .findAll(source)
        .map { it.groupValues[1] }
        .firstOrNull { it !in transcriptMathAllowedCommands }
    if (unsupportedCommand != null) {
        return TranscriptSafeMathPresentation(source, source, supported = false)
    }
    var display = source
    transcriptMathGlyphs.forEach { (command, glyph) ->
        display = display.replace(command, glyph)
    }
    return TranscriptSafeMathPresentation(source, display, supported = true)
}

internal data class TranscriptMermaidNode(
    val id: String,
    val label: String,
)

internal data class TranscriptMermaidEdge(
    val from: String,
    val to: String,
    val label: String? = null,
)

internal data class TranscriptMermaidDiagram(
    val direction: String,
    val nodes: List<TranscriptMermaidNode>,
    val edges: List<TranscriptMermaidEdge>,
)

private val transcriptMermaidHeader = Regex("""(?i)^(?:graph|flowchart)\s+(TD|TB|BT|LR|RL)\s*$""")
private val transcriptMermaidNode = Regex(
    """^([A-Za-z_][A-Za-z0-9_-]*)(?:\[(.{1,120})]|\((.{1,120})\)|\{(.{1,120})})?$""",
)
private val transcriptMermaidUnsafe = Regex(
    """(?i)(?:%%\{|^\s*(?:click|linkstyle|style|classdef|class|subgraph|end)\b|javascript:|data:|<|>)""",
)

private fun parseTranscriptMermaidNode(token: String): TranscriptMermaidNode? {
    val match = transcriptMermaidNode.matchEntire(token.trim()) ?: return null
    val id = match.groupValues[1]
    val rawLabel = match.groupValues.drop(2).firstOrNull(String::isNotEmpty) ?: id
    val label = rawLabel.trim().removeSurrounding("\"").trim()
    if (
        label.isEmpty() ||
        label.length > 120 ||
        label.any { it.code in 0..31 || it.code == 127 } ||
        transcriptMermaidUnsafe.containsMatchIn(label)
    ) return null
    return TranscriptMermaidNode(id, label)
}

internal fun projectTranscriptMermaid(code: String): TranscriptMermaidDiagram? {
    val source = code.trim()
    if (
        source.isEmpty() ||
        source.length > MAX_TRANSCRIPT_MERMAID_LENGTH ||
        source.any { it.code in 0..8 || it.code in 11..31 || it.code == 127 } ||
        transcriptMermaidUnsafe.containsMatchIn(source)
    ) return null

    val lines = source.lines()
    if (lines.size > MAX_TRANSCRIPT_MERMAID_LINES) return null
    val first = lines.indexOfFirst { it.isNotBlank() }
    if (first < 0) return null
    val header = transcriptMermaidHeader.matchEntire(lines[first].trim()) ?: return null
    val nodes = linkedMapOf<String, TranscriptMermaidNode>()
    val edges = mutableListOf<TranscriptMermaidEdge>()

    fun addNode(node: TranscriptMermaidNode): Boolean {
        val existing = nodes[node.id]
        if (existing == null || existing.label == existing.id) nodes[node.id] = node
        return nodes.size <= MAX_TRANSCRIPT_MERMAID_NODES
    }

    for (lineIndex in first + 1 until lines.size) {
        val line = lines[lineIndex].trim()
        if (line.isEmpty() || line.startsWith("%%")) continue
        if (transcriptMermaidUnsafe.containsMatchIn(line)) return null

        val arrow = when {
            "-->" in line -> "-->"
            "---" in line -> "---"
            else -> null
        }
        if (arrow == null) {
            val node = parseTranscriptMermaidNode(line) ?: return null
            if (!addNode(node)) return null
            continue
        }

        val arrowIndex = line.indexOf(arrow)
        val leftToken = line.substring(0, arrowIndex).trim()
        var rightToken = line.substring(arrowIndex + arrow.length).trim()
        var edgeLabel: String? = null
        if (rightToken.startsWith("|")) {
            val close = rightToken.indexOf('|', startIndex = 1)
            if (close <= 1) return null
            edgeLabel = rightToken.substring(1, close).trim().takeIf(String::isNotEmpty)
            if (
                edgeLabel != null &&
                (edgeLabel.length > 120 || transcriptMermaidUnsafe.containsMatchIn(edgeLabel))
            ) return null
            rightToken = rightToken.substring(close + 1).trim()
        }

        val from = parseTranscriptMermaidNode(leftToken) ?: return null
        val to = parseTranscriptMermaidNode(rightToken) ?: return null
        if (!addNode(from) || !addNode(to)) return null
        edges += TranscriptMermaidEdge(from.id, to.id, edgeLabel)
        if (edges.size > MAX_TRANSCRIPT_MERMAID_EDGES) return null
    }

    if (nodes.isEmpty()) return null
    return TranscriptMermaidDiagram(
        direction = header.groupValues[1].uppercase(),
        nodes = nodes.values.toList(),
        edges = edges.toList(),
    )
}
