package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidContentSearchMedia
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Test

class CommandPaletteFileProviderTest {
    private fun message(
        id: String,
        agentId: String,
        timestampMs: Long,
        attachments: JSONArray,
    ): JSONObject =
        JSONObject()
            .put("id", id)
            .put("kind", "message")
            .put("role", "user")
            .put("agentId", agentId)
            .put("content", "files")
            .put("timestampMs", timestampMs)
            .put("attachments", attachments)

    @Test
    fun projectsCanonicalFilesWithDesktopEntryIdentityAndKinds() {
        val transcript = JSONArray()
            .put(
                message(
                    id = "entry-new",
                    agentId = "agent-a",
                    timestampMs = 20L,
                    attachments = JSONArray()
                        .put(
                            JSONObject()
                                .put("path", "fabushi-blob://report")
                                .put("name", "report.pdf")
                                .put("size", 42L)
                                .put("mimeType", "application/pdf"),
                        )
                        .put(
                            JSONObject()
                                .put("path", "fabushi-blob://photo")
                                .put("name", "photo.png")
                                .put("mimeType", "image/png"),
                        ),
                ),
            )
            .put(
                message(
                    id = "entry-old",
                    agentId = "agent-b",
                    timestampMs = 10L,
                    attachments = JSONArray().put(
                        JSONObject()
                            .put("path", "fabushi-blob://notes")
                            .put("name", "notes.md")
                            .put("mimeType", "text/markdown"),
                    ),
                ),
            )

        val all = commandPaletteFilesFromTranscript(transcript, "")
        assertEquals(listOf("report.pdf", "notes.md"), all.map { it.fileName })
        assertEquals(CommandPaletteFileKind.PDF, all.first().kind)
        assertEquals(42L, all.first().sizeBytes)

        val image = commandPaletteFilesFromTranscript(transcript, "image")
        assertEquals(listOf("photo.png"), image.map { it.fileName })
        assertEquals("entry-new", image.single().entryId)
        assertEquals(CommandPaletteFileKind.IMAGE, image.single().kind)

        val markdown = commandPaletteFilesFromTranscript(transcript, "notes")
        assertEquals(CommandPaletteFileKind.MARKDOWN, markdown.single().kind)
    }

    @Test
    fun enforcesLimitUnknownAgentFilteringAndGenerationFence() {
        val transcript = JSONArray()
            .put(
                message(
                    "entry-a",
                    "agent-a",
                    2L,
                    JSONArray().put(
                        JSONObject()
                            .put("path", "fabushi-blob://a")
                            .put("name", "a.txt"),
                    ),
                ),
            )
            .put(
                message(
                    "entry-b",
                    "agent-b",
                    1L,
                    JSONArray().put(
                        JSONObject()
                            .put("path", "fabushi-blob://b")
                            .put("name", "b.txt"),
                    ),
                ),
            )
        assertEquals(1, commandPaletteFilesFromTranscript(transcript, "", limit = 1).size)

        val entries = commandPaletteFileEntries(
            commandPaletteFilesFromTranscript(transcript, ""),
            agentNames = mapOf("agent-a" to "Alpha"),
        ) {}
        assertEquals(listOf("file:agent-a:entry-a"), entries.map { it.id })

        val fence = CommandPaletteFileRequestFence()
        val old = fence.begin()
        val current = fence.begin()
        assertFalse(fence.accepts(old))
        assertTrue(fence.accepts(current))
        fence.cancel()
        assertFalse(fence.accepts(current))
    }

    @Test
    fun projectsHostOwnedMediaMatchesWithoutReclassifyingOrReordering() {
        val projected = commandPaletteFilesFromHostMatches(
            listOf(
                AndroidContentSearchMedia(
                    agentId = "agent-b",
                    entryId = "entry-archive",
                    fileName = "opaque.data",
                    path = "fabushi-blob://archive",
                    ext = ".bin",
                    mime = "application/octet-stream",
                    kind = "archive",
                    timestampMs = 20L,
                    sizeBytes = 42L,
                    width = null,
                    height = null,
                ),
                AndroidContentSearchMedia(
                    agentId = "agent-a",
                    entryId = "entry-image",
                    fileName = "photo.png",
                    path = "fabushi-blob://image",
                    ext = ".png",
                    mime = "image/png",
                    kind = "image",
                    timestampMs = 10L,
                    sizeBytes = null,
                    width = 320L,
                    height = 200L,
                ),
            ),
        )

        assertEquals(listOf("entry-archive", "entry-image"), projected.map { it.entryId })
        assertEquals(
            listOf(CommandPaletteFileKind.ARCHIVE, CommandPaletteFileKind.IMAGE),
            projected.map { it.kind },
        )
        assertEquals("fabushi-blob://archive", projected.first().path)
        assertEquals(42L, projected.first().sizeBytes)
    }

    @Test
    fun rejectsMalformedHostOwnedMediaMatches() {
        assertThrows(IllegalStateException::class.java) {
            commandPaletteFilesFromHostMatches(
                listOf(
                    AndroidContentSearchMedia(
                        agentId = "agent-a",
                        entryId = "entry-a",
                        fileName = "mystery.bin",
                        path = "fabushi-blob://mystery",
                        ext = ".bin",
                        mime = null,
                        kind = "unknown",
                        timestampMs = 1L,
                        sizeBytes = null,
                        width = null,
                        height = null,
                    ),
                ),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            commandPaletteFilesFromHostMatches(
                listOf(
                    AndroidContentSearchMedia(
                        agentId = "",
                        entryId = "entry-a",
                        fileName = "report.pdf",
                        path = "fabushi-blob://report",
                        ext = ".pdf",
                        mime = "application/pdf",
                        kind = "pdf",
                        timestampMs = 1L,
                        sizeBytes = null,
                        width = null,
                        height = null,
                    ),
                ),
            )
        }
    }
}
