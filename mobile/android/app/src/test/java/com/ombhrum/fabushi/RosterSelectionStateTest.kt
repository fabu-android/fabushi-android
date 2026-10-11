package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RosterSelectionStateTest {
    private class MemoryPersistence : RosterSelectionPersistence {
        val values = linkedMapOf<String, StoredRosterSelection>()
        val writes = mutableListOf<Pair<String, RosterSelectionState>>()
        val clears = mutableListOf<String>()

        override fun read(accountSlot: String): StoredRosterSelection =
            values[accountSlot] ?: StoredRosterSelection.Absent

        override fun write(accountSlot: String, state: RosterSelectionState) {
            writes += accountSlot to state
            values[accountSlot] = StoredRosterSelection.Envelope(
                schemaVersion = 1,
                agentId = state.currentAgentId,
            )
        }

        override fun clear(accountSlot: String) {
            clears += accountSlot
            values.remove(accountSlot)
        }
    }

    @Test
    fun persistenceKeyMatchesDesktopAccountSensitiveEncoding() {
        assertEquals(
            "sand.client.slice.account.account%2Eone.selection.last-agent",
            rosterSelectionPersistenceKey("account.one"),
        )
    }

    @Test
    fun processDeathRestoresSelectionOnlyForSameAccount() {
        val persistence = MemoryPersistence()
        val firstProcess = RosterSelectionStore(persistence)
        firstProcess.restore("account.one")
        assertTrue(firstProcess.select("agent-2"))
        firstProcess.settle("agent-2")
        firstProcess.dispose()

        val restored = RosterSelectionStore(persistence)
        restored.restore("account.one")
        assertEquals("agent-2", restored.get().currentAgentId)
        assertFalse(restored.get().isLoadPending)

        restored.restore("account.two")
        assertNull(restored.get().currentAgentId)
    }

    @Test
    fun missingPendingSelectionWaitsForSettleBeforeFallback() {
        val persistence = MemoryPersistence()
        val store = RosterSelectionStore(persistence)
        store.restore("account.one")
        assertTrue(store.select("missing"))
        assertTrue(store.get().isLoadPending)

        store.reconcile(
            agentIds = listOf("agent-1"),
            isRosterComplete = true,
        )
        assertEquals("missing", store.get().currentAgentId)
        assertTrue(store.get().isLoadPending)

        store.settle("missing")
        assertEquals("agent-1", store.get().currentAgentId)
        assertFalse(store.get().isLoadPending)
        assertEquals("agent-1", persistence.writes.last().second.currentAgentId)
    }

    @Test
    fun incompleteRosterNeverReconcilesSelection() {
        val persistence = MemoryPersistence()
        val store = RosterSelectionStore(persistence)
        store.restore("account.one")
        store.select("agent-2")
        store.settle("agent-2")

        store.reconcile(
            agentIds = listOf("agent-1"),
            isRosterComplete = false,
        )
        assertEquals("agent-2", store.get().currentAgentId)
    }

    @Test
    fun corruptOrWrongSchemaStateIsClearedFailClosed() {
        val persistence = MemoryPersistence().apply {
            values["corrupt"] = StoredRosterSelection.Corrupt
            values["old"] = StoredRosterSelection.Envelope(
                schemaVersion = 99,
                agentId = "agent-2",
            )
        }
        val store = RosterSelectionStore(persistence)

        store.restore("corrupt")
        assertNull(store.get().currentAgentId)
        assertTrue("corrupt" in persistence.clears)

        store.restore("old")
        assertNull(store.get().currentAgentId)
        assertTrue("old" in persistence.clears)
    }
}
