package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AvatarMotionParityTest {
    @Test
    fun idleAvatarDoesNotScheduleInfiniteMotion() {
        assertNull(avatarAnimationDurationMillis(active = false))
        assertNull(avatarAnimationDurationMillis(AgentAvatarVisualState.IDLE))
    }

    @Test
    fun activeAvatarRetainsBoundedNativeComposeMotion() {
        assertEquals(1200, avatarAnimationDurationMillis(active = true))
        assertEquals(1200, avatarAnimationDurationMillis(AgentAvatarVisualState.SEARCHING))
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

    @Test
    fun activityProjectionMatchesDesktopPersonaPriority() {
        assertEquals(
            AgentAvatarVisualState.THINKING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityKind = "thinking",
                currentActivityTool = "GenerateImage",
                isRunning = true,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.SENDING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityKind = "tool",
                currentActivityTool = "SendToAgent",
                isRunning = true,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.SEARCHING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityTool = "browser_open",
                isRunning = true,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.LOADING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityTool = "GenerateImage",
                isRunning = true,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.ORBIT,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityTool = "CheckSubagent",
                isRunning = true,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.WORKING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityTool = "UnknownTool",
                isRunning = false,
            ),
        )
    }

    @Test
    fun verbAndComposingFallbacksMatchDesktopAvatarState() {
        assertEquals(
            AgentAvatarVisualState.SEARCHING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityVerb = "reading",
                isRunning = false,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.SENDING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                currentActivityVerb = "sending",
                isRunning = false,
            ),
        )
        assertEquals(
            AgentAvatarVisualState.THINKING,
            agentAvatarVisualState(
                awaitingUserResponse = null,
                isComposingMessage = true,
                isRunning = true,
            ),
        )
    }

    @Test
    fun avatarDispatcherUsesDesktopPrecedence() {
        assertEquals(
            AgentAvatarPresentationKind.PHOTO,
            agentAvatarPresentationKind(
                MobileBotSummaryAndroid(
                    id = "photo",
                    name = "Photo",
                    avatarDataUrl = "data:image/png;base64,AA==",
                    isSharedRoom = true,
                    isGroup = true,
                ),
            ),
        )
        assertEquals(
            AgentAvatarPresentationKind.SHARED_ROOM,
            agentAvatarPresentationKind(
                MobileBotSummaryAndroid(
                    id = "room",
                    name = "Room",
                    isSharedRoom = true,
                    isGroup = true,
                ),
            ),
        )
        assertEquals(
            AgentAvatarPresentationKind.GROUP,
            agentAvatarPresentationKind(
                MobileBotSummaryAndroid(id = "group", name = "Group", isGroup = true),
            ),
        )
        assertEquals(
            AgentAvatarPresentationKind.PERSONA,
            agentAvatarPresentationKind(MobileBotSummaryAndroid(id = "agent", name = "Agent")),
        )
    }
}
