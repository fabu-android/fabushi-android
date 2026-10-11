package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidMediaViewerProjectionTest {
    private fun message(id: String, type: String): ChatMessage =
        ChatMessage(
            id = id,
            conversationId = "conversation",
            text = id,
            contentType = type,
            outgoing = false,
            time = "",
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
}
