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

    @Test
    fun awaitingUserTakesPrecedenceOverRunningForAvatarState() {
        assertEquals(
            AgentAvatarVisualState.IDLE,
            agentAvatarVisualState(awaitingUserResponse = true, isRunning = true),
        )
        assertEquals(
            AgentAvatarVisualState.WORKING,
            agentAvatarVisualState(awaitingUserResponse = null, isRunning = true),
        )
        assertEquals(
            AgentAvatarVisualState.IDLE,
            agentAvatarVisualState(awaitingUserResponse = null, isRunning = false),
        )
    }
}
