package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class ConversationOutlineLifecycleProjectionTest {
    @Test
    fun runningSubagentProjectionDeduplicatesAndExcludesOtherTaskKinds() {
        val result = projectConversationOutlineSubagents(
            listOf(
                MobileAsyncTask("subagent", "child", "Review", 1L, subagentType = "reviewer"),
                MobileAsyncTask("shell", "shell", "Build", 2L),
                MobileAsyncTask("subagent", "child", "Duplicate", 3L, subagentType = "reviewer"),
            ),
        )
        assertEquals(1, result.size)
        assertEquals("child", result.single().id)
        assertEquals("reviewer", result.single().type)
        assertEquals("running", result.single().status)
    }
}
