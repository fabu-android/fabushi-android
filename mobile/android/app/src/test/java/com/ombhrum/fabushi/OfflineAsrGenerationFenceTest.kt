package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflineAsrGenerationFenceTest {
    @Test
    fun newerSessionRejectsLateCompletion() {
        val fence = OfflineAsrGenerationFence()
        val first = fence.begin()
        val second = fence.begin()
        assertFalse(fence.accepts(first))
        assertTrue(fence.accepts(second))
    }

    @Test
    fun disposeInvalidatesInFlightCompletion() {
        val fence = OfflineAsrGenerationFence()
        val generation = fence.begin()
        fence.invalidate()
        assertFalse(fence.accepts(generation))
    }
}
