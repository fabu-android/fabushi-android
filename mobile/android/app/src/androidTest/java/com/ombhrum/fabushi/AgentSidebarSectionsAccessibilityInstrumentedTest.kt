package com.ombhrum.fabushi

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AgentSidebarSectionsAccessibilityInstrumentedTest {
    @Test
    fun adaptiveLayoutBreakpointsMatchPhoneTabletAndFoldableContract() {
        assertEquals(0, agentSidebarContentPaddingDp(411))
        assertEquals(32, agentSidebarContentPaddingDp(720))
        assertEquals(72, agentSidebarContentPaddingDp(900))
    }

    @Test
    fun talkBackSectionLabelIncludesExpandedStateCountAndAction() {
        assertEquals(
            "Expanded section Operations with 2 Agents. Toggle section",
            agentSidebarSectionSemanticsLabel("Operations", 2, false),
        )
    }
}
