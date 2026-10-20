package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptRichContentProjectionTest {
    @Test
    fun projectsDesktopBlockGrammarWithoutExecutingMarkup() {
        val blocks = projectTranscriptRichContent(
            """
            # Heading

            > first
            > second

            3. ordered
            4. [x] checked

            | A | B |
            | --- | :---: |
            | 1 | 2 |

            ${'$'}${'$'}
            a^2 + b^2
            ${'$'}${'$'}

            ---

            ```kotlin
            println("ok")
            ```
            """.trimIndent(),
        )

        assertTrue(blocks.any { it == TranscriptRichBlock.Heading(1, "Heading") })
        assertTrue(blocks.any { it == TranscriptRichBlock.Quote("first\nsecond") })
        val list = blocks.filterIsInstance<TranscriptRichBlock.ListBlock>().single()
        assertTrue(list.ordered)
        assertEquals(3, list.start)
        assertEquals(
            TranscriptRichListItem(text = "checked", task = true, checked = true),
            list.items[1],
        )
        val table = blocks.filterIsInstance<TranscriptRichBlock.Table>().single()
        assertEquals(listOf("A", "B"), table.headers)
        assertEquals(listOf(listOf("1", "2")), table.rows)
        assertEquals("a^2 + b^2", blocks.filterIsInstance<TranscriptRichBlock.Math>().single().expression.trim())
        assertTrue(blocks.any { it is TranscriptRichBlock.HorizontalRule })
        assertEquals(
            TranscriptRichBlock.Code("kotlin", "println(\"ok\")"),
            blocks.filterIsInstance<TranscriptRichBlock.Code>().single(),
        )
    }

    @Test
    fun invalidFenceLanguageFallsBackToUnlabelledCode() {
        val blocks = projectTranscriptRichContent(
            "```bad language!\nbody\n```",
        )
        assertEquals(TranscriptRichBlock.Code("", "body"), blocks.single())
    }

    @Test
    fun mermaidFenceUsesBoundedNativeProjectionAndKeepsLabelsInert() {
        val block = projectTranscriptRichContent(
            "```mermaid\nflowchart LR\nA[Start] -->|next| B[Finish]\n```",
        ).single() as TranscriptRichBlock.Mermaid

        val diagram = block.diagram!!
        assertEquals("LR", diagram.direction)
        assertEquals(listOf("Start", "Finish"), diagram.nodes.map { it.label })
        assertEquals(TranscriptMermaidEdge("A", "B", "next"), diagram.edges.single())
    }

    @Test
    fun mermaidRejectsExecutableOrUnsupportedDirectivesAndFallsBackSafely() {
        val unsafe = projectTranscriptRichContent(
            "```mermaid\nflowchart TD\nA --> B\nclick A javascript:alert(1)\n```",
        ).single() as TranscriptRichBlock.Mermaid
        assertNull(unsafe.diagram)

        assertNull(projectTranscriptMermaid("flowchart TD\nsubgraph hidden\nA-->B\nend"))
        assertNull(projectTranscriptMermaid("flowchart TD\nA[<script>] --> B"))
    }

    @Test
    fun mermaidViewportBoundsZoomPanAndFitState() {
        val zoomed = applyTranscriptMermaidViewportTransform(
            current = TranscriptMermaidViewport(),
            zoomChange = 2f,
            panXChange = 24f,
            panYChange = -12f,
        )
        assertEquals(2f, zoomed.scale, 0.001f)
        assertEquals(24f, zoomed.panX, 0.001f)
        assertEquals(-12f, zoomed.panY, 0.001f)

        val bounded = applyTranscriptMermaidViewportTransform(
            current = zoomed,
            zoomChange = 99f,
            panXChange = Float.POSITIVE_INFINITY,
            panYChange = 9_999f,
        )
        assertEquals(3f, bounded.scale, 0.001f)
        assertEquals(24f, bounded.panX, 0.001f)
        assertEquals(4_096f, bounded.panY, 0.001f)
        assertEquals(TranscriptMermaidViewport(), fitTranscriptMermaidViewport())
    }

    @Test
    fun mathProjectionAllowsBoundedInertSubsetAndRejectsActiveExtensions() {
        val safe = projectTranscriptSafeMath("""\alpha + x \leq \infty""")
        assertTrue(safe.supported)
        assertEquals("α + x ≤ ∞", safe.displayText)

        val unsafe = projectTranscriptSafeMath("""\href{https://example.test}{click}""")
        assertFalse(unsafe.supported)
        assertTrue(unsafe.displayText.contains("href"))
    }

    @Test
    fun malformedUnclosedMathRemainsVisiblePlainText() {
        val blocks = projectTranscriptRichContent("before\n\$\$\nnot closed")
        val visible = blocks.filterIsInstance<TranscriptRichBlock.Paragraph>().joinToString("\n") { it.text }
        assertTrue(visible.contains("before"))
        assertTrue(visible.contains("not closed"))
    }

    @Test
    fun projectsAssistantInlineMarksMathAndSafeLinks() {
        val spans = projectTranscriptInlineContent(
            "plain **bold** *italic* ~~strike~~ `code` \\(x + y\\) [docs](https://example.test/path) https://example.test/end).",
        )

        assertTrue(spans.any { it.text == "bold" && it.bold })
        assertTrue(spans.any { it.text == "italic" && it.italic })
        assertTrue(spans.any { it.text == "strike" && it.strike })
        assertTrue(spans.any { it.text == "code" && it.code })
        assertTrue(spans.any { it.text == "x + y" && it.math })
        assertTrue(spans.any { it.text == "docs" && it.url == "https://example.test/path" })
        assertTrue(spans.any { it.text == "https://example.test/end" && it.url == "https://example.test/end" })
        assertTrue(spans.any { it.text == ")." && it.url == null })
    }

    @Test
    fun unsafeOrMalformedInlineLinksRemainInertText() {
        val javascript = projectTranscriptInlineContent("[bad](javascript:alert(1))")
        assertEquals("[bad](javascript:alert(1))", javascript.joinToString("") { it.text })
        assertTrue(javascript.none { it.url != null })

        val hostless = projectTranscriptInlineContent("https:///private/path")
        assertEquals("https:///private/path", hostless.joinToString("") { it.text })
        assertTrue(hostless.none { it.url != null })

        assertNull(normalizeTranscriptHttpUrl("https:///private/path"))
        assertNull(normalizeTranscriptHttpUrl("https:relative"))
    }

    @Test
    fun nestedInlineMarksPreserveRecursivePresentationOnlyProjection() {
        val spans = projectTranscriptInlineContent("**bold and `code`**")
        assertTrue(spans.any { it.text == "bold and " && it.bold })
        assertTrue(spans.any { it.text == "code" && it.bold && it.code })
    }

    @Test
    fun projectsPersistedTipTapMarksBlocksAndReferences() {
        val blocks = projectTranscriptTipTapDocument(
            """{
              "type":"doc",
              "content":[
                {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Title","marks":[{"type":"bold"}]}]},
                {"type":"paragraph","content":[
                  {"type":"text","text":"safe","marks":[{"type":"italic"},{"type":"underline"},{"type":"link","attrs":{"href":"https://example.test/path"}}]},
                  {"type":"hardBreak"},
                  {"type":"mention","attrs":{"label":"Alice"}},
                  {"type":"workflowReference","attrs":{"label":"Deploy"}},
                  {"type":"prReference","attrs":{"prNumber":42}}
                ]},
                {"type":"blockquote","content":[{"type":"paragraph","content":[{"type":"text","text":"quoted","marks":[{"type":"strike"}]}]}]},
                {"type":"orderedList","attrs":{"start":3},"content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"item"}]}]}]},
                {"type":"codeBlock","content":[{"type":"text","text":"echo ok"}]},
                {"type":"horizontalRule"}
              ]
            }""",
        )!!

        val heading = blocks[0] as TranscriptTipTapBlock.Heading
        assertEquals(2, heading.level)
        assertTrue(heading.inline.single().bold)

        val paragraph = blocks[1] as TranscriptTipTapBlock.Paragraph
        assertTrue(paragraph.inline.first().italic)
        assertTrue(paragraph.inline.first().underline)
        assertEquals("https://example.test/path", paragraph.inline.first().url)
        assertEquals("\n", paragraph.inline[1].text)
        assertEquals("@Alice", paragraph.inline[2].text)
        assertEquals("@Deploy", paragraph.inline[3].text)
        assertEquals("#42", paragraph.inline[4].text)

        val quote = blocks[2] as TranscriptTipTapBlock.Quote
        assertTrue((quote.blocks.single() as TranscriptTipTapBlock.Paragraph).inline.single().strike)
        val list = blocks[3] as TranscriptTipTapBlock.ListBlock
        assertTrue(list.ordered)
        assertEquals(3, list.start)
        assertEquals("item", (list.items.single().single() as TranscriptTipTapBlock.Paragraph).inline.single().text)
        assertEquals("echo ok", (blocks[4] as TranscriptTipTapBlock.Code).text)
        assertTrue(blocks[5] is TranscriptTipTapBlock.HorizontalRule)
    }

    @Test
    fun malformedTipTapFallsBackAndUnsafeLinkMarkRemainsInert() {
        assertNull(projectTranscriptTipTapDocument("{not-json"))
        assertNull(projectTranscriptTipTapDocument("""{"type":"paragraph","content":[]}"""))

        val blocks = projectTranscriptTipTapDocument(
            """{"type":"doc","content":[{"type":"paragraph","content":[
              {"type":"text","text":"bad","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]},
              {"type":"mystery","content":[{"type":"text","text":"still visible"}]}
            ]}]}""",
        )!!
        val paragraph = blocks.single() as TranscriptTipTapBlock.Paragraph
        assertEquals("bad", paragraph.inline[0].text)
        assertNull(paragraph.inline[0].url)
        assertEquals("still visible", paragraph.inline[1].text)
    }

    @Test
    fun normalizesOnlyBoundedHttpAndHttpsLinks() {
        assertEquals("https://example.test/path", normalizeTranscriptHttpUrl(" https://example.test/path "))
        assertEquals("http://example.test", normalizeTranscriptHttpUrl("http://example.test"))
        assertNull(normalizeTranscriptHttpUrl("javascript:alert(1)"))
        assertNull(normalizeTranscriptHttpUrl("file:///data/data/secret"))
        assertNull(normalizeTranscriptHttpUrl("https://example.test/\u0001bad"))
    }
}
