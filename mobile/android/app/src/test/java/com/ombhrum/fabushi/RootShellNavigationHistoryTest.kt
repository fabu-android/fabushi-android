package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RootShellNavigationHistoryTest {
    private class MemoryPersistence : RootShellNavigationPersistence {
        var scope: RootShellNavigationScope? = null
        var state: RootShellNavigationState? = null

        override fun load(scope: RootShellNavigationScope): RootShellNavigationState? =
            state?.takeIf { this.scope == scope }

        override fun save(scope: RootShellNavigationScope, state: RootShellNavigationState) {
            this.scope = scope
            this.state = state
        }

        override fun clear() {
            scope = null
            state = null
        }
    }

    private fun signedIn(owner: RootShellNavigationOwner, epoch: Long = 3L, generation: Long = 7L) {
        owner.observeAccount(true, generation, epoch, "account-a")
    }

    @Test fun multiStepBackAndForwardSettlesPendingWithoutDuplicatingHistory() {
        val owner = RootShellNavigationOwner(MemoryPersistence())
        signedIn(owner)
        owner.observeAppliedAgent("a")
        owner.observeAppliedAgent("b")
        owner.observeAppliedAgent("c")

        assertEquals("b", owner.navigate(RootShellNavigationDirection.BACK, setOf("a", "b", "c")))
        owner.observeAppliedAgent("b")
        assertEquals("a", owner.navigate(RootShellNavigationDirection.BACK, setOf("a", "b", "c")))
        owner.observeAppliedAgent("a")
        assertEquals("b", owner.navigate(RootShellNavigationDirection.FORWARD, setOf("a", "b", "c")))
        owner.observeAppliedAgent("b")
        assertEquals("c", owner.navigate(RootShellNavigationDirection.FORWARD, setOf("a", "b", "c")))
        owner.observeAppliedAgent("c")

        assertEquals(listOf("a", "b", "c"), owner.snapshotForTest().entries)
        assertNull(owner.snapshotForTest().pendingTarget)
        assertEquals("c", owner.snapshotForTest().appliedAgentId)
    }

    @Test fun deletedAgentsAreSkippedWithoutRewritingHistoricalEntries() {
        val owner = RootShellNavigationOwner(MemoryPersistence())
        signedIn(owner)
        owner.observeAppliedAgent("a")
        owner.observeAppliedAgent("deleted")
        owner.observeAppliedAgent("c")

        assertEquals("a", owner.navigate(RootShellNavigationDirection.BACK, setOf("a", "c")))
        assertEquals(listOf("a", "deleted", "c"), owner.snapshotForTest().entries)
    }

    @Test fun newSelectionAfterBackTruncatesForwardHistory() {
        val owner = RootShellNavigationOwner(MemoryPersistence())
        signedIn(owner)
        owner.observeAppliedAgent("a")
        owner.observeAppliedAgent("b")
        owner.observeAppliedAgent("c")
        assertEquals("b", owner.navigate(RootShellNavigationDirection.BACK, setOf("a", "b", "c", "d")))
        owner.observeAppliedAgent("b")
        owner.observeAppliedAgent("d")

        assertEquals(listOf("a", "b", "d"), owner.snapshotForTest().entries)
        assertNull(owner.navigate(RootShellNavigationDirection.FORWARD, setOf("a", "b", "c", "d")))
    }

    @Test fun accountEpochSwitchFencesHistoryAndStaleEpochCannotRestoreIt() {
        val persistence = MemoryPersistence()
        val owner = RootShellNavigationOwner(persistence)
        signedIn(owner, epoch = 4L, generation = 9L)
        owner.observeAppliedAgent("a")
        owner.observeAccount(true, 10L, 5L, "account-b")
        assertTrue(owner.snapshotForTest().entries.isEmpty())

        owner.observeAccount(true, 8L, 4L, "account-a")
        assertTrue(owner.snapshotForTest().entries.isEmpty())
    }

    @Test fun processRecoveryRestoresSameAccountHistory() {
        val persistence = MemoryPersistence()
        val first = RootShellNavigationOwner(persistence)
        signedIn(first, epoch = 11L, generation = 2L)
        first.observeAppliedAgent("a")
        first.observeAppliedAgent("b")

        val reopened = RootShellNavigationOwner(persistence)
        reopened.observeAccount(true, 1L, 11L, "account-a")
        assertEquals(listOf("a", "b"), reopened.snapshotForTest().entries)
        assertEquals("b", reopened.snapshotForTest().appliedAgentId)
    }

    @Test fun logoutClearsDurableHistory() {
        val persistence = MemoryPersistence()
        val owner = RootShellNavigationOwner(persistence)
        signedIn(owner)
        owner.observeAppliedAgent("a")
        owner.observeAccount(false, 8L, 3L, "account-a")
        assertTrue(owner.snapshotForTest().entries.isEmpty())
        assertNull(persistence.state)
    }
}
