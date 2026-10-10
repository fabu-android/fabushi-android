package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AccountRebuildState
import org.json.JSONObject
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
        assertEquals(AccountRebuildState.RECONNECTING, owner.accountProjection(7))
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
        assertEquals(AccountRebuildState.OUTCOME_UNKNOWN, reopened.accountProjection(3))
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
    fun foreverBoxAdapterMatchesDesktopPhaseRulesAndFailsClosed() {
        assertEquals(
            "box-a" to "pulling",
            projectForeverBoxRebuildEvent(
                JSONObject("""{"agentId":"box-a","state":"running","pull":{"percent":10},"vncUrl":"wss://viewer"}"""),
            ),
        )
        assertEquals(
            "box-a" to "running",
            projectForeverBoxRebuildEvent(
                JSONObject("""{"agentId":"box-a","state":"running","vncUrl":"wss://viewer"}"""),
            ),
        )
        assertEquals(
            "box-a" to "local",
            projectForeverBoxRebuildEvent(JSONObject("""{"agentId":"box-a","state":"running"}""")),
        )
        assertEquals(
            "box-a" to "sleeping",
            projectForeverBoxRebuildEvent(
                JSONObject("""{"payload":{"agentId":"box-a","state":"hibernated"}}"""),
            ),
        )
        assertNull(projectForeverBoxRebuildEvent(JSONObject("""{"state":"running"}""")))
    }

    @Test
    fun projectionReturnsIdleOnlyAfterOwnerSettlesTheEpisode() {
        val owner = ComputerRebuildStateOwner(MemoryStore())
        owner.observeAccount(29)
        assertEquals(AccountRebuildState.IDLE, owner.accountProjection(29))
        owner.begin(29, ComputerRebuildKind.UPDATE, null, ComputerRebuildSource.REQUEST)
        owner.observeConnection(29, false)
        owner.observeConnection(29, true)
        assertEquals(AccountRebuildState.RECONNECTING, owner.accountProjection(29))
        owner.deactivate(29)
        assertEquals(AccountRebuildState.IDLE, owner.accountProjection(29))
    }

    @Test
    fun requestAckCancelAndMismatchRemainOperationFenced() {
        val owner = ComputerRebuildStateOwner(MemoryStore(), processGeneration = 9)
        owner.observeAccount(41)
        owner.setPending(41, true)
        owner.begin(41, ComputerRebuildKind.RESET, "reset-a", null)

        val mismatch = owner.acknowledge(41, "reset-b")
        assertFalse(mismatch.acknowledged)
        assertTrue(mismatch.pending)
        assertEquals(AccountRebuildState.RECONNECTING, owner.accountProjection(41))

        val acknowledged = owner.acknowledge(41, "reset-a")
        assertTrue(acknowledged.acknowledged)
        owner.setPending(41, false)
        val cancelled = owner.deactivate(41)
        assertNull(cancelled.kind)
        assertFalse(cancelled.pending)
        assertEquals(ComputerRebuildResolution.CANCELLED, cancelled.lastResolution)
        assertEquals(AccountRebuildState.IDLE, owner.accountProjection(41))
    }

    @Test
    fun terminalMigrationRejectsStaleOperationAndDeactivationSettlesToIdle() {
        val owner = ComputerRebuildStateOwner(MemoryStore(), processGeneration = 12)
        owner.observeAccount(51)
        owner.begin(51, ComputerRebuildKind.RESET, "migration-a", null)
        owner.observeConnection(51, false)
        owner.observeBox(51, "box-a", "off")
        owner.observeConnection(51, true)

        val stale = owner.observeMigration(51, "migration-b", ComputerRebuildMigrationPhase.DONE)
        assertEquals("migration-a", stale.operationId)
        assertFalse(stale.terminalMigration)

        val terminal = owner.observeMigration(51, "migration-a", ComputerRebuildMigrationPhase.DONE)
        assertTrue(terminal.terminalMigration)
        val settled = owner.deactivate(51)
        assertNull(settled.kind)
        assertEquals(ComputerRebuildResolution.SETTLED, settled.lastResolution)
        assertEquals(AccountRebuildState.IDLE, owner.accountProjection(51))
    }

    @Test
    fun processRestartRebindsGenerationAndKeepsUnsettledOutcomeUnknown() {
        val store = MemoryStore(
            ComputerRebuildSnapshot(
                accountEpoch = 61,
                processGeneration = 4,
                kind = ComputerRebuildKind.UPDATE,
                source = ComputerRebuildSource.REQUEST,
                pending = true,
            ),
        )
        val reopened = ComputerRebuildStateOwner(store, processGeneration = 5)
        val snapshot = reopened.snapshot(61)
        assertEquals(5L, snapshot.processGeneration)
        assertTrue(snapshot.outcomeUnknown)
        assertEquals(AccountRebuildState.OUTCOME_UNKNOWN, reopened.accountProjection(61))
    }

    @Test
    fun migrationAndDevSignalAdaptersMatchDesktopWireShapesAndFailClosed() {
        val migration = projectBoxMigrationRebuildEvent(
            JSONObject(
                """{"type":"box-migration","payload":{"operationId":{"value":"op-1"},"phase":"creating","detail":"boot"}}""",
            ),
        )
        assertEquals("op-1", migration?.operationId)
        assertEquals(ComputerRebuildMigrationPhase.CREATING, migration?.phase)
        assertNull(
            projectBoxMigrationRebuildEvent(
                JSONObject("""{"type":"box-migration","payload":{"operationId":"forged","phase":"done"}}"""),
            ),
        )
        assertTrue(
            isDevBoxRebuildStartEvent(
                JSONObject("""{"type":"dev-box-rebuild","payload":{"type":"start"}}"""),
            ),
        )
        assertFalse(
            isDevBoxRebuildStartEvent(
                JSONObject("""{"type":"dev-box-rebuild","payload":{"type":"other"}}"""),
            ),
        )
    }

    @Test
    fun requestIdentityOffsetAndProcessRecoveryNeverReplaySideEffect() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store, processGeneration = 70)
        owner.observeAccount(70)
        val reserved = owner.reserveRequest(70, "request-70")
        assertTrue(reserved.pending)
        assertEquals("request-70", reserved.requestId)

        val accepted = owner.acceptRequest(
            accountEpoch = 70,
            requestId = "request-70",
            operationId = "operation-70",
            kind = ComputerRebuildKind.UPDATE,
        )
        assertFalse(accepted.pending)
        assertTrue(accepted.acknowledged)
        assertEquals("operation-70", accepted.operationId)

        val offset = owner.recordMigrationOffset(70, "operation-70", "cursor-11")
        assertEquals("cursor-11", offset.migrationOffsetKey)

        val reopened = ComputerRebuildStateOwner(store, processGeneration = 71)
        val recovered = reopened.snapshot(70)
        assertEquals("request-70", recovered.requestId)
        assertEquals("operation-70", recovered.operationId)
        assertEquals("cursor-11", recovered.migrationOffsetKey)
        assertTrue(recovered.outcomeUnknown)
        assertThrows(IllegalArgumentException::class.java) {
            reopened.reserveRequest(70, "request-new")
        }

        val reconciled = reopened.observeMigration(
            70,
            "operation-70",
            ComputerRebuildMigrationPhase.CREATING,
        )
        assertFalse(reconciled.outcomeUnknown)
    }

    @Test
    fun staleOperationCannotAdvanceMigrationCursorOrSettleRequest() {
        val owner = ComputerRebuildStateOwner(MemoryStore(), processGeneration = 80)
        owner.observeAccount(80)
        owner.reserveRequest(80, "request-80")
        owner.acceptRequest(80, "request-80", "operation-80", ComputerRebuildKind.RESET)

        // Stream cursor advancement is fenced by the active operation identity supplied by the
        // watcher, not by the event's operation identity. This lets unrelated account-wide events
        // advance without mutating this rebuild episode.
        val cursorAfterOtherEvent = owner.recordMigrationOffset(80, "operation-80", "cursor-stale")
        assertEquals("cursor-stale", cursorAfterOtherEvent.migrationOffsetKey)
        val staleDone = owner.observeMigration(
            80,
            "operation-other",
            ComputerRebuildMigrationPhase.DONE,
        )
        assertEquals("operation-80", staleDone.operationId)
        assertFalse(staleDone.terminalMigration)

        owner.recordMigrationOffset(80, "operation-80", "")
        assertEquals("", owner.snapshot(80).migrationOffsetKey)
        owner.recordMigrationOffset(80, "operation-80", "cursor-good")
        val done = owner.observeMigration(80, "operation-80", ComputerRebuildMigrationPhase.DONE)
        assertEquals("cursor-good", done.migrationOffsetKey)
        assertTrue(done.terminalMigration)
        val settled = owner.deactivate(80)
        assertNull(settled.requestId)
        assertEquals("", settled.migrationOffsetKey)
        assertEquals(ComputerRebuildResolution.SETTLED, settled.lastResolution)
    }

    @Test
    fun backendRejectionAndUnknownOutcomeRemainDistinctAndAccountFenced() {
        val rejected = ComputerRebuildStateOwner(MemoryStore())
        rejected.observeAccount(90)
        rejected.reserveRequest(90, "request-rejected")
        val rejectedState = rejected.rejectRequest(90, "request-rejected")
        assertNull(rejectedState.requestId)
        assertFalse(rejectedState.outcomeUnknown)
        assertEquals(ComputerRebuildResolution.FAILED, rejectedState.lastResolution)

        val unknownStore = MemoryStore()
        val unknown = ComputerRebuildStateOwner(unknownStore)
        unknown.observeAccount(91)
        unknown.reserveRequest(91, "request-unknown")
        val unknownState = unknown.markRequestOutcomeUnknown(91, "request-unknown")
        assertEquals("request-unknown", unknownState.requestId)
        assertTrue(unknownState.outcomeUnknown)
        assertThrows(IllegalArgumentException::class.java) {
            unknown.markRequestOutcomeUnknown(92, "request-unknown")
        }
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
