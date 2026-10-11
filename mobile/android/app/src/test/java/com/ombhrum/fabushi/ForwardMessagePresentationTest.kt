package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
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
}
