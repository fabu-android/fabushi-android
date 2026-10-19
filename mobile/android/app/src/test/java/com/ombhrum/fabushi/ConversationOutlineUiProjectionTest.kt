package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidConversationOutlineItem
import org.junit.Assert.assertEquals
import org.junit.Test

class ConversationOutlineUiProjectionTest {
    @Test
    fun toolLabelsAndPreviewsAreDeterministic() {
        val item = AndroidConversationOutlineItem(
            kind = "tool-call",
            id = "1",
            name = "WebSearchToolCall",
            status = "done",
            summary = "line one\nline two",
        )
        assertEquals("Web Search", conversationOutlineItemLabel(item))
        assertEquals("line one", conversationOutlineItemPreview(item))
    }

    @Test
    fun sendMessagePreviewUsesCanonicalMessageValue() {
        val item = AndroidConversationOutlineItem(
            kind = "send-message",
            id = "2",
            messageType = "attachment",
            messageValue = "file://attachment",
        )
        assertEquals("Message", conversationOutlineItemLabel(item))
        assertEquals("file://attachment", conversationOutlineItemPreview(item))
    }
}
