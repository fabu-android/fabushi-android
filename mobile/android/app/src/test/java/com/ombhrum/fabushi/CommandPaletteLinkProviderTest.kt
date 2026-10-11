package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandPaletteLinkProviderTest {
    @Test
    fun extractsMarkdownAndBareLinksFromCurrentConversationAndDedupes() {
        val links = commandPaletteLinksFromConversation(
            listOf(
                "Read [Docs](https://example.com/docs) and https://openai.com/research.",
                "https://example.com/docs",
                "not a url",
            ),
        )
        assertEquals(
            listOf("https://example.com/docs", "https://openai.com/research"),
            links.map { it.url },
        )
        assertEquals("example.com/docs", commandPaletteLinkDisplayUrl(links.first().url))
        assertNull(normalizeCommandPaletteLink("javascript:alert(1)"))
        assertNull(normalizeCommandPaletteLink("ftp://example.com/file"))
    }

    @Test
    fun metadataOnlyEnhancesPresentationAndFenceRejectsLateResults() {
        val links = listOf(CommandPaletteLink("https://example.com/docs"))
        val entries = commandPaletteLinkEntries(
            links = links,
            metadata = mapOf(
                "https://example.com/docs" to TranscriptLinkCardMetadata(
                    title = "Example Docs",
                    description = "Reference",
                    hostname = "example.com",
                ),
            ),
        ) {}
        assertEquals("Example Docs", entries.single().label)
        assertEquals("example.com/docs", entries.single().detail)

        val fence = CommandPaletteLinkRequestFence()
        val first = fence.begin()
        val second = fence.begin()
        assertFalse(fence.accepts(first))
        assertTrue(fence.accepts(second))
        fence.cancel()
        assertFalse(fence.accepts(second))
    }
}
