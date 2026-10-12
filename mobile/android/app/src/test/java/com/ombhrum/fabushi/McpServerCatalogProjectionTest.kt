package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class McpServerCatalogProjectionTest {
    @Test
    fun parserKeepsOnlyBoundedTypedServerMetadata() {
        val values = JSONArray()
            .put(JSONObject().put("id", "17").put("name", "Calendar").put("url", "https://mcp.example.test/api").put("transport", "http").put("disabledByTeamAdminPolicy", true))
            .put(JSONObject().put("id", "18").put("name", "Local").put("transport", "stdio"))
            .put(JSONObject().put("id", "19").put("name", "Broken").put("transport", "sse"))
            .put(JSONObject().put("id", "").put("name", "Missing id").put("transport", "http").put("url", "https://mcp.example.test"))
            .put(JSONObject().put("id", "20").put("name", "Unknown transport").put("transport", "pipe").put("url", "https://mcp.example.test"))

        assertEquals(
            listOf(
                McpServerCatalogItem("17", "Calendar", "https://mcp.example.test/api", "http", true),
                McpServerCatalogItem("18", "Local", null, "stdio", false),
            ),
            parseMcpServerCatalog(values),
        )
    }

    @Test
    fun teamPopularityParserKeepsNonNegativeCounts() {
        val projection = JSONObject()
            .put(
                "counts",
                JSONObject()
                    .put("42", 7)
                    .put("9001", 0)
                    .put("negative", -1)
                    .put("   ", 3),
            )

        assertEquals(
            mapOf("42" to 7L, "9001" to 0L),
            parseMcpTeamPopularity(projection),
        )
        assertEquals(emptyMap<String, Long>(), parseMcpTeamPopularity(JSONObject()))
    }

    @Test
    fun staleAccountOrGenerationCannotApplyTeamPopularityResult() {
        assertTrue(shouldApplyMcpTeamPopularityResult(7, 7, "account-a", true, "account-a"))
        assertFalse(shouldApplyMcpTeamPopularityResult(7, 8, "account-a", true, "account-a"))
        assertFalse(shouldApplyMcpTeamPopularityResult(7, 7, "account-a", true, "account-b"))
        assertFalse(shouldApplyMcpTeamPopularityResult(7, 7, "account-a", false, "account-a"))
    }

    @Test
    fun staleAccountOrGenerationCannotApplyCatalogResult() {
        assertTrue(shouldApplyMcpServerCatalogResult(7, 7, "account-a", true, "account-a"))
        assertFalse(shouldApplyMcpServerCatalogResult(7, 8, "account-a", true, "account-a"))
        assertFalse(shouldApplyMcpServerCatalogResult(7, 7, "account-a", true, "account-b"))
        assertFalse(shouldApplyMcpServerCatalogResult(7, 7, "account-a", false, "account-a"))
    }
}
