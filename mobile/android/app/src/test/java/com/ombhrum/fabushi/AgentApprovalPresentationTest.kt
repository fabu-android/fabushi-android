package com.ombhrum.fabushi

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AgentApprovalPresentationTest {
    @Test
    fun matchingParkedApprovalProjectsWithoutInventingExpiry() {
        val projected = projectPendingAgentApproval(
            JSONObject()
                .put("type", "approval.requested")
                .put("approvalId", "approval-1")
                .put("operationId", "op-1")
                .put("capability", "agent.subagent.review")
                .put("reason", "review required")
                .put("expiresAtMs", JSONObject.NULL),
            "op-1",
        )
        requireNotNull(projected)
        assertEquals("approval-1", projected.approvalId)
        assertEquals("agent.subagent.review", projected.capability)
        assertEquals("review required", projected.reason)
        assertNull(projected.expiresAtMs)
    }

    @Test
    fun staleOperationApprovalIsFencedFromPresentation() {
        assertNull(
            projectPendingAgentApproval(
                JSONObject()
                    .put("type", "approval.requested")
                    .put("approvalId", "approval-stale")
                    .put("operationId", "old-op"),
                "current-op",
            ),
        )
    }
}
