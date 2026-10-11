package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentTranscriptAttachmentProjectionTest {
    private fun message(
        id: String,
        agentId: String,
        attachments: JSONArray,
    ): JSONObject =
        JSONObject()
            .put("id", id)
            .put("kind", "message")
            .put("role", "user")
            .put("agentId", agentId)
            .put("content", "with files")
            .put("operationId", "operation-$id")
            .put("clientNonce", "nonce-$id")
            .put("timestampMs", 7L)
            .put("attachments", attachments)

    @Test
    fun projectsCanonicalAttachmentsWithoutCrossAgentLeakage() {
        val entries = JSONArray()
            .put(
                message(
                    "m-a",
                    "agent-a",
                    JSONArray()
                        .put(
                            JSONObject()
                                .put("path", "fabushi-blob://blob-a")
                                .put("name", "report.pdf")
                                .put("size", 42L)
                                .put("mimeType", "application/pdf"),
                        )
                        .put(
                            JSONObject()
                                .put("path", "file:///private/photo.png")
                                .put("name", "photo.png"),
                        ),
                ),
            )
            .put(
                message(
                    "m-b",
                    "agent-b",
                    JSONArray().put(
                        JSONObject()
                            .put("path", "fabushi-blob://secret")
                            .put("name", "other-account.txt"),
                    ),
                ),
            )

        val projected = canonicalMobileTranscriptForAgent(entries, "agent-a")
        assertEquals(listOf("m-a"), projected.map { it.id })
        val attachments = projected.single().attachments
        assertEquals(listOf("report.pdf", "photo.png"), attachments.map { it.name })
        assertEquals("fabushi-blob://blob-a", attachments.first().path)
        assertEquals(42L, attachments.first().sizeBytes)
        assertEquals("application/pdf", attachments.first().mimeType)
        assertNull(attachments.last().sizeBytes)
        assertNull(attachments.last().mimeType)

        val canonical = canonicalAgentTranscriptMessages(entries)
        val agentA = canonical.single { it.agentId == "agent-a" }
        assertEquals(attachments, agentA.attachments)
    }

    @Test
    fun malformedAttachmentRowsFailClosedWithoutDroppingMessage() {
        val entries = JSONArray().put(
            message(
                "m-a",
                "agent-a",
                JSONArray()
                    .put(JSONObject().put("name", "missing-path.txt"))
                    .put(JSONObject().put("path", "fabushi-blob://missing-name"))
                    .put(
                        JSONObject()
                            .put("path", "fabushi-blob://valid")
                            .put("name", "valid.txt")
                            .put("size", -1L)
                            .put("mimeType", "text/plain"),
                    ),
            ),
        )

        val message = canonicalMobileTranscriptForAgent(entries, "agent-a").single()
        assertEquals("with files", message.text)
        assertEquals(listOf("valid.txt"), message.attachments.map { it.name })
        assertNull(message.attachments.single().sizeBytes)
        assertEquals("text/plain", message.attachments.single().mimeType)
    }

    @Test
    fun absentAttachmentArrayProjectsAsImmutableEmptyList() {
        val entry = JSONObject()
            .put("id", "m-a")
            .put("kind", "message")
            .put("role", "assistant")
            .put("agentId", "agent-a")
            .put("content", "plain")
        val projected = canonicalMobileTranscriptForAgent(JSONArray().put(entry), "agent-a")
        assertTrue(projected.single().attachments.isEmpty())
    }
}
