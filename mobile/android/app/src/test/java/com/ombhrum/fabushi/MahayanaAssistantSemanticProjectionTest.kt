package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MahayanaAssistantSemanticProjectionTest {
    @Test
    fun assistantIsFindableOnChatListAndWhileOpenButHiddenFromChannels() {
        assertTrue(
            MahayanaAssistantSemanticProjection.visible(
                destinationIsHome = true,
                activeSection = null,
                regularConversationOpen = false,
                assistantOpen = false,
            ),
        )
        assertTrue(
            MahayanaAssistantSemanticProjection.visible(
                destinationIsHome = true,
                activeSection = null,
                regularConversationOpen = false,
                assistantOpen = true,
            ),
        )
        assertFalse(
            MahayanaAssistantSemanticProjection.visible(
                destinationIsHome = true,
                activeSection = AndroidMobileSection.CHANNELS,
                regularConversationOpen = false,
                assistantOpen = false,
            ),
        )
        assertFalse(
            MahayanaAssistantSemanticProjection.visible(
                destinationIsHome = true,
                activeSection = null,
                regularConversationOpen = true,
                assistantOpen = false,
            ),
        )
    }

    @Test
    fun unreadNoneSemanticNodeExistsOnlyOnInactiveChatList() {
        assertTrue(
            MahayanaAssistantSemanticProjection.unreadVisible(
                destinationIsHome = true,
                activeSection = null,
                regularConversationOpen = false,
                assistantOpen = false,
            ),
        )
        assertFalse(
            MahayanaAssistantSemanticProjection.unreadVisible(
                destinationIsHome = true,
                activeSection = null,
                regularConversationOpen = false,
                assistantOpen = true,
            ),
        )
        assertFalse(
            MahayanaAssistantSemanticProjection.unreadVisible(
                destinationIsHome = true,
                activeSection = AndroidMobileSection.CHANNELS,
                regularConversationOpen = false,
                assistantOpen = false,
            ),
        )
    }

    @Test
    fun semanticIdsRemainDesktopCompatibleAndStable() {
        assertTrue(MahayanaAssistantSemanticProjection.AgentId.endsWith(":agent:assistant"))
        assertTrue(MahayanaAssistantSemanticProjection.UnreadAgentId.endsWith(":agent:assistant"))
        assertTrue(MahayanaAssistantSemanticProjection.UnreadNoneName == "unread-none")
        assertTrue(MahayanaAssistantSemanticProjection.UnreadPositiveName == "unread-positive")
        assertTrue(MahayanaAssistantSemanticProjection.unreadName(false) == "unread-none")
        assertTrue(MahayanaAssistantSemanticProjection.unreadName(true) == "unread-positive")
    }
}
