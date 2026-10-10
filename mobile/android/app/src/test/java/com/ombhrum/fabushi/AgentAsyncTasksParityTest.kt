package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentAsyncTasksParityTest {
    @Test
    fun projectionAcceptsDesktopTaskKindsAndPreservesStableIdentity() {
        val rows = JSONArray()
            .put(
                JSONObject()
                    .put("kind", "subagent")
                    .put("id", "generated:one")
                    .put("label", "Inspect repository")
                    .put("status", "running")
                    .put("startedAtMs", 1_000L)
                    .put("detail", "box-1")
                    .put("subagentType", "executor"),
            )
            .put(
                JSONObject()
                    .put("kind", "shell")
                    .put("id", "shell-1")
                    .put("label", "Run checks")
                    .put("status", "running")
                    .put("startedAtMs", 2_000L),
            )
            .put(
                JSONObject()
                    .put("kind", "cloud-agent")
                    .put("id", "cloud-1")
                    .put("label", "Cloud task")
                    .put("status", "running")
                    .put("startedAtMs", 3_000L),
            )

        val tasks = projectMobileAsyncTasks(rows)

        assertEquals(listOf("subagent", "shell", "cloud-agent"), tasks.map(MobileAsyncTask::kind))
        assertEquals("generated:one", tasks.first().id)
        assertEquals("box-1", tasks.first().detail)
        assertEquals("executor", tasks.first().subagentType)
    }

    @Test
    fun projectionRejectsMalformedOrSettledRows() {
        val malformed = JSONArray().put(
            JSONObject()
                .put("kind", "subagent")
                .put("id", "generated:done")
                .put("label", "Done")
                .put("status", "completed")
                .put("startedAtMs", 1L),
        )

        val failure = runCatching { projectMobileAsyncTasks(malformed) }.exceptionOrNull()
        assertTrue(failure is IllegalStateException)
    }

    @Test
    fun relativeTimeFormattingMatchesDesktopThresholds() {
        // Keep the synthetic clock comfortably after Unix epoch so the 1-year case
        // exercises the relative-time threshold instead of the invalid timestamp guard.
        val now = 400L * 24L * 60L * 60L * 1_000L
        assertEquals("now", formatAsyncTaskTime(now - 59_000L, now))
        assertEquals("1m ago", formatAsyncTaskTime(now - 60_000L, now))
        assertEquals("1h ago", formatAsyncTaskTime(now - 60L * 60L * 1_000L, now))
        assertEquals("1d ago", formatAsyncTaskTime(now - 24L * 60L * 60L * 1_000L, now))
        assertEquals("1mo ago", formatAsyncTaskTime(now - 30L * 24L * 60L * 60L * 1_000L, now))
        assertEquals("1y ago", formatAsyncTaskTime(now - 365L * 24L * 60L * 60L * 1_000L, now))
    }

    @Test
    fun taskMetadataUsesDesktopLabelsWithoutInventingState() {
        assertEquals(
            "Subagent · box-1",
            asyncTaskMeta(
                MobileAsyncTask(
                    kind = "subagent",
                    id = "generated:one",
                    label = "Inspect",
                    startedAtMs = 1L,
                    detail = "box-1",
                ),
            ),
        )
        assertEquals(
            "Cloud agent",
            asyncTaskMeta(
                MobileAsyncTask(
                    kind = "cloud-agent",
                    id = "cloud-1",
                    label = "Cloud",
                    startedAtMs = 1L,
                ),
            ),
        )
    }
}
