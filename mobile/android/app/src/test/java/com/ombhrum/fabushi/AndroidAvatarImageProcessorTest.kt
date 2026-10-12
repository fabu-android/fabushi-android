package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_OUTPUT_DIMENSION
import com.ombhrum.fabushi.androidmain.media.avatarDecodeSampleSize
import com.ombhrum.fabushi.androidmain.media.avatarDimensionsAllowed
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidAvatarImageProcessorTest {
    @Test
    fun sourceDimensionsMatchDesktopDecodeBoundary() {
        assertTrue(
            avatarDimensionsAllowed(
                MAX_AGENT_AVATAR_SOURCE_DIMENSION,
                MAX_AGENT_AVATAR_SOURCE_DIMENSION,
            ),
        )
        assertTrue(
            avatarDimensionsAllowed(
                MAX_AGENT_AVATAR_SOURCE_DIMENSION * 2,
                MAX_AGENT_AVATAR_SOURCE_DIMENSION * 3,
            ),
        )
        assertFalse(avatarDimensionsAllowed(0, 512))
        assertFalse(avatarDimensionsAllowed(512, 0))
    }

    @Test
    fun decodeSamplingBoundsNativeWorkNearOutputTarget() {
        assertEquals(1, avatarDecodeSampleSize(1024, 1024))
        assertEquals(2, avatarDecodeSampleSize(2048, 1024))
        assertEquals(4, avatarDecodeSampleSize(4096, 4096))
        assertEquals(8, avatarDecodeSampleSize(8192, 4097))
        val sample = avatarDecodeSampleSize(16_384, 8_192)
        assertTrue(16_384 / sample <= MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
        val largeSource = MAX_AGENT_AVATAR_SOURCE_DIMENSION * 3
        val largeSample = avatarDecodeSampleSize(largeSource, largeSource * 2)
        assertTrue((largeSource * 2) / largeSample <= MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
    }
}
