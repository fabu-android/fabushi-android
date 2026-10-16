package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RosterRefreshFenceTest {
    @Test
    fun newerRefreshSupersedesOlderProjection() {
        val fence = RosterRefreshFence()
        val first = fence.begin()
        val second = fence.begin()

        assertFalse(fence.isCurrent(first))
        assertTrue(fence.isCurrent(second))
    }

    @Test
    fun accountResetInvalidatesInFlightRefresh() {
        val fence = RosterRefreshFence()
        val token = fence.begin()

        fence.invalidate()

        assertFalse(fence.isCurrent(token))
        assertFalse(fence.accepts(token, "account-a", "account-a"))
    }

    @Test
    fun canonicalAccountMustStillMatchBeforeCommit() {
        val fence = RosterRefreshFence()
        val token = fence.begin()

        assertTrue(fence.accepts(token, "account-a", "account-a"))
        assertFalse(fence.accepts(token, "account-a", "account-b"))
        assertFalse(fence.accepts(token, "account-a", null))
    }
}
