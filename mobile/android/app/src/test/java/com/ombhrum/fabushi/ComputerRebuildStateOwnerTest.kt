package com.ombhrum.fabushi.androidmain.coordinator

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerRebuildStateOwnerTest {
    private class MemoryStore(
        var value: ComputerRebuildSnapshot? = null,
    ) : ComputerRebuildStateStore {
        override fun read(): ComputerRebuildSnapshot? = value
        override fun write(snapshot: ComputerRebuildSnapshot?) {
            value = snapshot
        }
    }

    @Test
    fun resetOperationIsStableAccountFencedAndDuplicateSafe() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store)
        owner.observeAccount(7)
        val first = owner.begin(
            accountEpoch = 7,
            kind = ComputerRebuildKind.RESET,
            operationId = "reset-7",
            source = null,
        )
        val duplicate = owner.begin(
            accountEpoch = 7,
            kind = ComputerRebuildKind.RESET,
            operationId = "reset-7",
            source = null,
        )

        assertEquals(first, duplicate)
        assertEquals("reset-7", duplicate.operationId)
        assertTrue(duplicate.leftHealthy)
        assertFalse(duplicate.acknowledged)
        assertThrows(IllegalArgumentException::class.java) {
            owner.begin(8, ComputerRebuildKind.RESET, "stale", null)
        }
    }

    @Test
    fun coldStartMakesUnsettledOperationOutcomeUnknownWithoutReplay() {
        val store = MemoryStore(
            ComputerRebuildSnapshot(
                accountEpoch = 3,
                kind = ComputerRebuildKind.RECOVER,
                operationId = "recover-3",
                pending = true,
                acknowledged = true,
            ),
        )
        val reopened = ComputerRebuildStateOwner(store)
        val recovered = reopened.snapshot(3)

        assertEquals("recover-3", recovered.operationId)
        assertTrue(recovered.pending)
        assertTrue(recovered.outcomeUnknown)
    }

    @Test
    fun migrationOperationFencesTerminalAndSettlesOnlyAfterMatchingEpisode() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store)
        owner.observeAccount(11)
        owner.begin(11, ComputerRebuildKind.RESET, "migration-a", null)
        owner.observeConnection(11, false)
        owner.observeBox(11, "box-a", "off")
        owner.observeConnection(11, true)

        val stale = owner.observeMigration(
            11,
            "migration-b",
            ComputerRebuildMigrationPhase.DONE,
        )
        assertFalse(stale.terminalMigration)
        assertEquals(ComputerRebuildTeardown.BOX, stale.teardown)
        assertTrue(stale.reconnectedSinceLeft)

        val terminal = owner.observeMigration(
            11,
            "migration-a",
            ComputerRebuildMigrationPhase.DONE,
        )
        assertTrue(terminal.terminalMigration)
        val settled = owner.deactivate(11)
        assertNull(settled.kind)
        assertNull(settled.operationId)
        assertEquals(ComputerRebuildResolution.SETTLED, settled.lastResolution)
    }

    @Test
    fun pullingSameHealthyBoxStartsAutoUpdateAndAccountSwitchClearsIt() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store)
        owner.observeAccount(21)
        owner.observeBox(21, "box-a", "running")
        val update = owner.observeBox(21, "box-a", "pulling")

        assertEquals(ComputerRebuildKind.UPDATE, update.kind)
        assertEquals(ComputerRebuildSource.AUTO, update.source)
        assertEquals("box-a", update.lockBoxId)

        owner.observeAccount(22)
        val switched = owner.snapshot(22)
        assertNull(switched.kind)
        assertNull(switched.operationId)
        assertFalse(switched.outcomeUnknown)
    }

    @Test
    fun failedMigrationIsTerminalAndDoesNotPretendSuccess() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store)
        owner.observeAccount(31)
        owner.begin(31, ComputerRebuildKind.UPDATE, null, ComputerRebuildSource.MIGRATION)
        val failed = owner.observeMigration(
            31,
            null,
            ComputerRebuildMigrationPhase.FAILED,
        )

        assertNull(failed.kind)
        assertEquals(ComputerRebuildResolution.FAILED, failed.lastResolution)
        assertFalse(failed.outcomeUnknown)
    }
}
