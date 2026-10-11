package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentTranscriptAssistantPresentationTest {
    private fun sendMessageText(
        content: String,
        images: JSONArray? = null,
        channel: Any? = null,
        streaming: Any? = null,
    ): JSONObject =
        JSONObject()
            .put("id", "send-1")
            .put("agentId", "agent-a")
            .put("kind", "send-message")
            .put(
                "message",
                JSONObject()
                    .put("type", "text")
                    .put("content", content)
                    .also { message ->
                        if (images != null) message.put("images", images)
                        if (channel != null) message.put("channel", channel)
                    },
            )
            .also { entry -> if (streaming != null) entry.put("streaming", streaming) }

    @Test
    fun strictSendMessageTextProjectsImagesChannelAndTrustedSource() {
        val raw = sendMessageText(
            content = "caption",
            images = JSONArray()
                .put(JSONObject().put("url", "https://example.test/a.png").put("alt", "diagram"))
                .put(JSONObject().put("url", "https://example.test/b.webp")),
            channel = "analysis",
        )

        val projected = projectTranscriptSendMessageText(raw)!!
        assertEquals("caption", projected.content)
        assertEquals(listOf("https://example.test/a.png", "https://example.test/b.webp"), projected.images.map { it.url })
        assertEquals("diagram", projected.images.first().alt)
        assertEquals("analysis", projected.channel)
        assertTrue(projected.sourceTrusted)
        assertNull(projected.urlCardUrl)

        val mobile = canonicalMobileTranscriptForAgent(JSONArray().put(raw), "agent-a").single()
        assertEquals(MobileChatRole.ASSISTANT, mobile.role)
        assertEquals("send-message", mobile.transcriptSourceKind)
        assertEquals(projected.images, mobile.assistantImages)
        assertTrue(mobile.assistantSourceTrusted)
    }

    @Test
    fun imageFreeNonStreamingBareLinkUsesUrlCardWhileStreamingDoesNot() {
        val raw = sendMessageText("https://example.test/path")
        val projected = projectTranscriptSendMessageText(raw)!!
        assertEquals("https://example.test/path", projected.urlCardUrl)

        val streaming = projectTranscriptSendMessageText(
            sendMessageText("https://example.test/path", streaming = true),
        )!!
        assertNull(streaming.urlCardUrl)

        val markdown = projectTranscriptSendMessageText(
            sendMessageText("[Docs](https://example.test/docs)"),
        )!!
        assertEquals("https://example.test/docs", markdown.urlCardUrl)
    }

    @Test
    fun malformedSendMessageTextFailsClosedInsteadOfCoercingTypes() {
        assertNull(
            projectTranscriptSendMessageText(
                sendMessageText(
                    "caption",
                    images = JSONArray().put(JSONObject().put("url", 42)),
                ),
            ),
        )
        assertNull(projectTranscriptSendMessageText(sendMessageText("caption", channel = 42)))
        assertNull(projectTranscriptSendMessageText(sendMessageText("caption", streaming = "yes")))
    }

    @Test
    fun ordinaryAssistantMessageConsumesRetainedSendMessageProjectionWithoutOwningIt() {
        val retained = sendMessageText(
            content = "caption",
            images = JSONArray().put(JSONObject().put("url", "https://example.test/a.png")),
            channel = "commentary",
        ).put("isSourceTrusted", true)
        val ordinary = JSONObject()
            .put("id", "m1")
            .put("agentId", "agent-a")
            .put("kind", "message")
            .put("role", "assistant")
            .put("author", "Agent")
            .put("content", "caption")
            .put("sendMessageText", retained)

        val mobile = canonicalMobileTranscriptForAgent(JSONArray().put(ordinary), "agent-a").single()
        assertEquals(listOf("https://example.test/a.png"), mobile.assistantImages.map { it.url })
        assertEquals("commentary", mobile.assistantChannel)
        assertTrue(mobile.assistantSourceTrusted)
        assertNull(mobile.urlCardUrl)
    }

    @Test
    fun remoteImageLoaderAcceptsOnlyBoundedHttpsUrls() {
        assertEquals(
            "https://example.test/image.png",
            normalizeTranscriptRemoteImageUrl("https://example.test/image.png"),
        )
        assertNull(normalizeTranscriptRemoteImageUrl("http://example.test/image.png"))
        assertNull(normalizeTranscriptRemoteImageUrl("file:///data/user/0/private.png"))
        assertNull(normalizeTranscriptRemoteImageUrl("javascript:alert(1)"))
    }

    @Test
    fun directUntrustedOrdinaryImageMetadataNeverBecomesTrustedByPresentation() {
        val ordinary = JSONObject()
            .put("id", "m1")
            .put("agentId", "agent-a")
            .put("kind", "message")
            .put("role", "assistant")
            .put("content", "caption")
            .put("images", JSONArray().put(JSONObject().put("url", "https://example.test/a.png")))

        val mobile = canonicalMobileTranscriptForAgent(JSONArray().put(ordinary), "agent-a").single()
        assertEquals(1, mobile.assistantImages.size)
        assertFalse(mobile.assistantSourceTrusted)
    }
}
