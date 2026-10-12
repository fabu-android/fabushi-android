package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ForwardMessagePresentationTest {
    @Test
    fun destinationsAreNormalizedAndDeduplicatedWithoutChangingSelectionOrder() {
        assertEquals(
            listOf("conversation-b", "conversation-a"),
            normalizeForwardDestinations(
                listOf(" conversation-b ", "", "conversation-a", "conversation-b"),
            ),
        )
    }

    @Test
    fun oneForwardBatchGetsStableDestinationScopedMutationIdentities() {
        val first = forwardClientMessageId("batch-7", "conversation-a")
        val duplicate = forwardClientMessageId("batch-7", "conversation-a")
        val otherDestination = forwardClientMessageId("batch-7", "conversation-b")
        assertEquals(first, duplicate)
        assertNotEquals(first, otherDestination)
        assertEquals("android:forward:batch-7:conversation-a", first)
    }
    @Test
    fun forwardPrivacyAndDestinationIdentityAreFrozenIntoEachCommand() {
        val request = ForwardMessageBatchRequest(
            sourceConversationId = "source",
            messageId = "message-1",
            destinationConversationIds = listOf("destination"),
            batchId = "batch-9",
            dropSenderNames = false,
            dropCaptions = true,
        )
        val command = buildForwardCommand(request, "destination")
        assertEquals("source", command.getString("sourceConversationId"))
        assertEquals("message-1", command.getString("messageId"))
        assertEquals("destination", command.getString("destinationConversationId"))
        assertEquals("android:forward:batch-9:destination", command.getString("clientMessageId"))
        assertTrue(command.getJSONObject("privacy").getBoolean("dropCaptions"))
        assertTrue(command.getJSONObject("privacy").getBoolean("dropSenderNames"))
    }

    @Test
    fun keyboardNavigationAndShortcutsMatchDesktopForwardDialogContract() {
        assertEquals(1, nextForwardRecipientIndex(0, 8, ForwardRecipientNavigationKey.ARROW_DOWN))
        assertEquals(0, nextForwardRecipientIndex(0, 8, ForwardRecipientNavigationKey.ARROW_UP))
        assertEquals(6, nextForwardRecipientIndex(0, 8, ForwardRecipientNavigationKey.PAGE_DOWN))
        assertEquals(1, nextForwardRecipientIndex(7, 8, ForwardRecipientNavigationKey.PAGE_UP))
        assertEquals(7, nextForwardRecipientIndex(2, 8, ForwardRecipientNavigationKey.END))
        assertEquals(0, nextForwardRecipientIndex(2, 8, ForwardRecipientNavigationKey.HOME))
        assertTrue(isForwardSubmitShortcut("Enter", ctrlKey = true, metaKey = false))
        assertTrue(isForwardToggleShortcut(" ", ctrlKey = false, metaKey = false))
        assertFalse(isForwardToggleShortcut("Enter", ctrlKey = true, metaKey = false))
    }
}
