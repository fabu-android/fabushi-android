package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AvatarMotionParityTest {
    @Test
    fun idleAvatarDoesNotScheduleInfiniteMotion() {
        assertNull(avatarAnimationDurationMillis(active = false))
    }

    @Test
    fun activeAvatarRetainsBoundedNativeComposeMotion() {
        assertEquals(1200, avatarAnimationDurationMillis(active = true))
    }
}
