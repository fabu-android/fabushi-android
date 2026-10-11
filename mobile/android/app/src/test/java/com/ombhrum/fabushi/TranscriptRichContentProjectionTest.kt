package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
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
    fun normalizesOnlyBoundedHttpAndHttpsLinks() {
        assertEquals("https://example.test/path", normalizeTranscriptHttpUrl(" https://example.test/path "))
        assertEquals("http://example.test", normalizeTranscriptHttpUrl("http://example.test"))
        assertNull(normalizeTranscriptHttpUrl("javascript:alert(1)"))
        assertNull(normalizeTranscriptHttpUrl("file:///data/data/secret"))
        assertNull(normalizeTranscriptHttpUrl("https://example.test/\u0001bad"))
    }
}
