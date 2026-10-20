package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HiddenBotsProjectionTest {
    @Test
    fun projectionContainsOnlyHiddenBotsAndDeduplicatesIdentity() {
        val rows = projectHiddenBotRows(
            listOf(
                MobileBotSummaryAndroid(id = "a", name = "A", isHidden = true),
                MobileBotSummaryAndroid(id = "b", name = "B", isHidden = false),
                MobileBotSummaryAndroid(id = "a", name = "A duplicate", isHidden = true),
            ),
        )

        assertEquals(listOf(HiddenBotRowProjection("a", "A")), rows)
    }

    @Test
    fun emptyLabelOnlyAppearsForEmptyProjection() {
        assertEquals("No hidden bots", hiddenChatsEmptyLabel(emptyList()))
        assertNull(hiddenChatsEmptyLabel(listOf(HiddenBotRowProjection("a", "A"))))
    }
}
