package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptMotionPreferenceTest {
    @Test
    fun zeroOrInvalidAnimatorScaleUsesReducedMotionPresentation() {
        assertTrue(transcriptShouldReduceMotion(0f))
        assertTrue(transcriptShouldReduceMotion(-1f))
        assertTrue(transcriptShouldReduceMotion(Float.NaN))
        assertTrue(transcriptShouldReduceMotion(Float.POSITIVE_INFINITY))
    }

    @Test
    fun positiveAnimatorScaleKeepsPlatformMotion() {
        assertFalse(transcriptShouldReduceMotion(0.5f))
        assertFalse(transcriptShouldReduceMotion(1f))
        assertFalse(transcriptShouldReduceMotion(10f))
    }
}
