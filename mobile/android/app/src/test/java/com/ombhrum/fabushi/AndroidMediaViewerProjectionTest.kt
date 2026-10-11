package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidMediaViewerProjectionTest {
    private fun message(
        id: String,
        type: String,
        mimeType: String? = null,
        fileName: String? = null,
    ): ChatMessage =
        ChatMessage(
            id = id,
            conversationId = "conversation",
            text = id,
            contentType = type,
            outgoing = false,
            time = "",
            mediaMimeType = mimeType,
            mediaFileName = fileName,
        )

    @Test
    fun previewableStartBuildsPhotoVideoGalleryInTranscriptOrder() {
        val messages = listOf(
            message("text", "text"),
            message("photo", "photo"),
            message("document", "document"),
            message("video", "video"),
        )
        val gallery = projectAndroidMediaGallery(messages, "video")
        assertEquals(listOf("photo", "video"), gallery.map { it.id })
        assertEquals(1, initialAndroidMediaGalleryIndex(gallery, "video"))
    }

    @Test
    fun nonPreviewableStartStaysSingleFileViewer() {
        val messages = listOf(message("photo", "photo"), message("document", "document"))
        val gallery = projectAndroidMediaGallery(messages, "document")
        assertEquals(listOf("document"), gallery.map { it.id })
    }

    @Test
    fun galleryNavigationWrapsBothDirections() {
        assertEquals(0, stepAndroidMediaGalleryIndex(2, 1, 3))
        assertEquals(2, stepAndroidMediaGalleryIndex(0, -1, 3))
        assertEquals(1, stepAndroidMediaGalleryIndex(0, 1, 3))
        assertEquals(-1, stepAndroidMediaGalleryIndex(0, 1, 0))
    }

    @Test
    fun missingStartDoesNotInventGallery() {
        assertTrue(projectAndroidMediaGallery(listOf(message("photo", "photo")), "missing").isEmpty())
    }
    @Test
    fun pdfDetectionUsesMimeOrFileNameWithoutJoiningPhotoVideoGallery() {
        val byMime = message("pdf-mime", "document", mimeType = "application/pdf; charset=binary")
        val byName = message("pdf-name", "document", fileName = "report.PDF")
        assertTrue(isAndroidPdfMessage(byMime))
        assertTrue(isAndroidPdfMessage(byName))
        assertEquals(
            listOf("pdf-mime"),
            projectAndroidMediaGallery(listOf(byMime), "pdf-mime").map { it.id },
        )
    }

    @Test
    fun pdfPageNavigationClampsAtDocumentEdges() {
        assertEquals(0, stepAndroidPdfPageIndex(0, -1, 4))
        assertEquals(1, stepAndroidPdfPageIndex(0, 1, 4))
        assertEquals(3, stepAndroidPdfPageIndex(3, 1, 4))
        assertEquals(-1, stepAndroidPdfPageIndex(0, 1, 0))
    }

    @Test
    fun pdfZoomMatchesDesktopBounds() {
        assertEquals(0.5f, clampAndroidPdfZoom(0.1f))
        assertEquals(1f, clampAndroidPdfZoom(1f))
        assertEquals(4f, clampAndroidPdfZoom(9f))
    }

    @Test
    fun delimitedSpreadsheetDetectionUsesExtensionOrMime() {
        assertTrue(isAndroidDelimitedSpreadsheet("report.CSV", null))
        assertTrue(isAndroidDelimitedSpreadsheet("report.tsv", "application/octet-stream"))
        assertTrue(isAndroidDelimitedSpreadsheet("report.bin", "text/csv; charset=utf-8"))
        assertEquals(
            false,
            isAndroidDelimitedSpreadsheet(
                "report.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
        )
    }

    @Test
    fun csvParserPreservesQuotedCommasEscapedQuotesAndTotalRows() {
        val sheet = parseAndroidDelimitedSpreadsheet(
            "name,note\nAlice,\"hello, world\"\nBob,\"said \"\"hi\"\"\"".toByteArray(),
            "people.csv",
        )
        assertEquals(3, sheet.totalRows)
        assertEquals(listOf("name", "note"), sheet.rows[0])
        assertEquals(listOf("Alice", "hello, world"), sheet.rows[1])
        assertEquals(listOf("Bob", "said \"hi\""), sheet.rows[2])
    }

    @Test
    fun spreadsheetParserTruncatesProjectionButCountsFullRows() {
        val input = buildString {
            append("header\n")
            repeat(10) { index -> append("row-").append(index).append('\n') }
        }
        val sheet = parseAndroidDelimitedSpreadsheet(input.toByteArray(), "rows.csv", maxRows = 3)
        assertEquals(3, sheet.rows.size)
        assertEquals(11, sheet.totalRows)
    }

}
