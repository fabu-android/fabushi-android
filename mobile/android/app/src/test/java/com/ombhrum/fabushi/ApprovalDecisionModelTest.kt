package com.ombhrum.fabushi

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ApprovalDecisionModelTest {
    @Test
    fun remoteApprovalProjectsStandingPermissionSurface() {
        val pending = projectPendingAgentApproval(
            JSONObject()
                .put("type", "approval.requested")
                .put("approvalId", "approval-1")
                .put("operationId", "operation-1")
                .put("capability", "remote.shell")
                .put("reason", "Run on trusted computer")
                .put("remoteDispatch", true),
            "operation-1",
        )!!
        assertTrue(pending.remoteDispatch)
        assertEquals("always", MobileApprovalDecision.ALWAYS.wireValue)
        assertEquals("never", MobileApprovalDecision.NEVER.wireValue)
    }

    @Test
    fun genericApprovalDoesNotProjectStandingPermissionSurface() {
        val pending = projectPendingAgentApproval(
            JSONObject()
                .put("type", "approval.requested")
                .put("approvalId", "approval-2")
                .put("operationId", "operation-2")
                .put("capability", "camera"),
            "operation-2",
        )!!
        assertFalse(pending.remoteDispatch)
        assertEquals("allow-once", MobileApprovalDecision.ALLOW_ONCE.wireValue)
        assertEquals("deny", MobileApprovalDecision.DENY.wireValue)
    }
}
