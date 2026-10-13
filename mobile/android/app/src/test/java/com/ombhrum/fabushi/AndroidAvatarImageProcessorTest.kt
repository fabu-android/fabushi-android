package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_OUTPUT_DIMENSION
import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_SOURCE_DIMENSION
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
        assertFalse(
            avatarDimensionsAllowed(
                MAX_AGENT_AVATAR_SOURCE_DIMENSION + 1,
                MAX_AGENT_AVATAR_SOURCE_DIMENSION,
            ),
        )
        assertFalse(
            avatarDimensionsAllowed(
                MAX_AGENT_AVATAR_SOURCE_DIMENSION,
                MAX_AGENT_AVATAR_SOURCE_DIMENSION + 1,
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
        val sample = avatarDecodeSampleSize(
            MAX_AGENT_AVATAR_SOURCE_DIMENSION,
            MAX_AGENT_AVATAR_SOURCE_DIMENSION,
        )
        assertTrue(MAX_AGENT_AVATAR_SOURCE_DIMENSION / sample <= MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
    }
}
