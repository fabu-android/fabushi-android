package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class TranscriptLoadErrorProjectionTest {
    @Test
    fun copyMatchesDesktopAuthority() {
        assertEquals("Couldn't load conversation", ANDROID_TRANSCRIPT_LOAD_ERROR_TITLE)
        assertEquals(
            "Couldn't load this conversation. Check your connection and try again.",
            ANDROID_TRANSCRIPT_LOAD_ERROR_DETAIL,
        )
        assertEquals("Retry", ANDROID_TRANSCRIPT_LOAD_ERROR_RETRY)
    }
}
