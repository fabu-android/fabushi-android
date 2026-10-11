package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflineAsrContractTest {
    @Test
    fun microphoneLeaseHasExactlyOneOwner() {
        val first = "test-voice-owner"
        val second = "test-asr-owner"
        MicrophoneLease.release(first)
        MicrophoneLease.release(second)
        assertTrue(MicrophoneLease.acquire(first))
        assertEquals(first, MicrophoneLease.currentOwnerForTest())
        assertFalse(MicrophoneLease.acquire(second))
        MicrophoneLease.release(first)
        assertNull(MicrophoneLease.currentOwnerForTest())
        assertTrue(MicrophoneLease.acquire(second))
        MicrophoneLease.release(second)
    }
}
