package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflineAsrDraftControlTest {
    @Test
    fun resultAppendsOnlyWhenDraftIsUnchanged() {
        assertEquals(
            "hello world",
            applyOfflineAsrDraftResult(
                draftAtStart = "hello",
                currentDraft = "hello",
                recognizedText = "  world  ",
            ),
        )
        assertEquals(
            "spoken text",
            applyOfflineAsrDraftResult(
                draftAtStart = "",
                currentDraft = "",
                recognizedText = " spoken text ",
            ),
        )
    }

    @Test
    fun lateResultCannotOverwriteUserEditedDraft() {
        assertNull(
            applyOfflineAsrDraftResult(
                draftAtStart = "before",
                currentDraft = "before user edit",
                recognizedText = "recognized",
            ),
        )
    }

    @Test
    fun generationFenceRejectsCancelledOrSupersededResult() {
        val fence = OfflineAsrGenerationFence()
        val first = fence.begin()
        assertTrue(fence.accepts(first))

        val second = fence.begin()
        assertFalse(fence.accepts(first))
        assertTrue(fence.accepts(second))

        fence.invalidate()
        assertFalse(fence.accepts(second))
    }
}
