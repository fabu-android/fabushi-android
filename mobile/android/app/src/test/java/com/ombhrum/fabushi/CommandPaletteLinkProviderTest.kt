package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandPaletteLinkProviderTest {
    @Test
    fun extractsOnlyCurrentConversationLinksDedupesAndRejectsUnsafeSchemes() {
        val currentConversation = listOf(
            "Read [Docs](https://example.com/docs) and https://openai.com/research.",
            "https://example.com/docs",
            "not a url",
            "javascript:alert(1)",
            "ftp://example.com/file",
        )
        val links = commandPaletteLinksFromConversation(currentConversation)

        assertEquals(
            listOf("https://example.com/docs", "https://openai.com/research"),
            links.map { it.url },
        )
        assertEquals("example.com/docs", commandPaletteLinkDisplayUrl(links.first().url))
        assertNull(normalizeCommandPaletteLink("javascript:alert(1)"))
        assertNull(normalizeCommandPaletteLink("ftp://example.com/file"))
        assertNull(normalizeCommandPaletteLink("file:///tmp/private"))
    }

    @Test
    fun metadataProjectionDoesNotExposeTranscriptMetadataTypeAndFailureIsIgnored() {
        val ready = TranscriptLinkCardSnapshot(
            status = TranscriptLinkCardStatus.READY,
            metadata = TranscriptLinkCardMetadata(
                title = "Example Docs",
                description = "Reference",
                siteName = "Internal-only field",
                hostname = "example.com",
            ),
        )
        assertEquals(
            CommandPaletteLinkMetadata(
                title = "Example Docs",
                description = "Reference",
                hostname = "example.com",
            ),
            commandPaletteLinkMetadataProjection(ready),
        )
        assertNull(
            commandPaletteLinkMetadataProjection(
                TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.FAILED),
            ),
        )

        val entries = commandPaletteLinkEntries(
            links = listOf(CommandPaletteLink("https://example.com/docs")),
            metadata = mapOf(
                "https://example.com/docs" to requireNotNull(commandPaletteLinkMetadataProjection(ready)),
            ),
        ) {}
        assertEquals("Example Docs", entries.single().label)
        assertEquals("example.com/docs", entries.single().detail)
    }

    @Test
    fun requestFenceRejectsCancelledLateClosedConversationAndAccountSwitchedResults() {
        val fence = CommandPaletteLinkRequestFence()
        val accountAChat1 = CommandPaletteLinkRequestScope("account-a", "chat-1")
        val accountAChat2 = CommandPaletteLinkRequestScope("account-a", "chat-2")
        val accountBChat1 = CommandPaletteLinkRequestScope("account-b", "chat-1")

        val first = fence.begin(accountAChat1)
        assertTrue(fence.accepts(first, accountAChat1))
        assertFalse(fence.accepts(first, accountAChat2))
        assertFalse(fence.accepts(first, accountBChat1))

        val second = fence.begin(accountAChat1)
        assertFalse(fence.accepts(first, accountAChat1))
        assertTrue(fence.accepts(second, accountAChat1))

        fence.cancel()
        assertFalse(fence.accepts(second, accountAChat1))
        assertFalse(fence.accepts(second, CommandPaletteLinkRequestScope("account-a", null)))
    }
}
