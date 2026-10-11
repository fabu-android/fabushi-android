package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class AgentTranscriptPaginationProjectionTest {
    private fun message(id: String, text: String = id): MobileChatMessage =
        MobileChatMessage(
            id = id,
            role = MobileChatRole.ASSISTANT,
            text = text,
        )

    @Test
    fun prependOlderPageKeepsChronologicalOrderAndDeduplicatesBoundary() {
        val older = listOf(message("m1"), message("m2"), message("m3", "older-boundary"))
        val current = listOf(message("m3", "current-boundary"), message("m4"), message("m5"))

        val merged = prependCanonicalMobileTranscriptPage(older, current)

        assertEquals(listOf("m1", "m2", "m3", "m4", "m5"), merged.map { it.id })
        assertEquals(
            "older-boundary",
            merged.single { it.id == "m3" }.text,
            "the canonical older page wins a duplicate cursor-boundary row",
        )
    }

    @Test
    fun prependEmptyPageLeavesCurrentProjectionUnchanged() {
        val current = listOf(message("m4"), message("m5"))
        assertEquals(current, prependCanonicalMobileTranscriptPage(emptyList(), current))
    }
}
