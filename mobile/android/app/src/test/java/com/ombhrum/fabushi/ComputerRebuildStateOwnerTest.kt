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
            ComputerRebuildBoxIngress("box-a", "pulling", 10.0),
            projectForeverBoxRebuildEvent(
                JSONObject("""{"agentId":"box-a","state":"running","pull":{"percent":10},"vncUrl":"wss://viewer"}"""),
            ),
        )
        assertEquals(
            ComputerRebuildBoxIngress("box-a", "running", null),
            projectForeverBoxRebuildEvent(
                JSONObject("""{"agentId":"box-a","state":"running","vncUrl":"wss://viewer"}"""),
            ),
        )
        assertEquals(
            ComputerRebuildBoxIngress("box-a", "local", null),
            projectForeverBoxRebuildEvent(JSONObject("""{"agentId":"box-a","state":"running"}""")),
        )
        assertEquals(
            ComputerRebuildBoxIngress("box-a", "sleeping", null),
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
    fun requestedUpdateSettlesOnMatchingMigrationDone() {
        val owner = ComputerRebuildStateOwner(MemoryStore(), processGeneration = 101)
        owner.observeAccount(101)
        owner.reserveRequest(101, "request-update")
        owner.acceptRequest(
            accountEpoch = 101,
            requestId = "request-update",
            operationId = "operation-update",
            kind = ComputerRebuildKind.UPDATE,
        )

        val creating = owner.observeMigration(
            101,
            "operation-update",
            ComputerRebuildMigrationPhase.CREATING,
        )
        assertEquals(ComputerRebuildSource.REQUEST, creating.source)
        assertFalse(creating.terminalMigration)

        val done = owner.observeMigration(
            101,
            "operation-update",
            ComputerRebuildMigrationPhase.DONE,
        )
        assertTrue(done.terminalMigration)
        val settled = owner.deactivate(101)
        assertNull(settled.operationId)
        assertNull(settled.requestId)
        assertEquals(ComputerRebuildResolution.SETTLED, settled.lastResolution)
        assertEquals(AccountRebuildState.IDLE, owner.accountProjection(101))
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

    @Test
    fun rebuildProgressFactsPersistAcrossProcessDeathWithoutReplayingSideEffect() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store, processGeneration = 120)
        owner.observeAccount(120)
        owner.observeBox(120, "box-a", "running")
        owner.observeBox(120, "box-a", "pulling", 37.5)
        owner.observeMigration(120, null, ComputerRebuildMigrationPhase.BACKING_UP)
        owner.observeMigration(120, null, ComputerRebuildMigrationPhase.CREATING)

        val beforeRestart = owner.snapshot(120)
        assertEquals(37.5, beforeRestart.pullPercent)
        assertEquals(
            listOf(
                ComputerRebuildMigrationPhase.BACKING_UP,
                ComputerRebuildMigrationPhase.CREATING,
            ),
            beforeRestart.migrationPhases,
        )

        val reopened = ComputerRebuildStateOwner(store, processGeneration = 121)
        val recovered = reopened.snapshot(120)
        assertEquals(121L, recovered.processGeneration)
        assertEquals(37.5, recovered.pullPercent)
        assertEquals(beforeRestart.migrationPhases, recovered.migrationPhases)
        assertTrue(recovered.outcomeUnknown)
    }

    @Test
    fun rebuildProgressRejectsInvalidPullPercentAndBoundsMigrationHistory() {
        val owner = ComputerRebuildStateOwner(MemoryStore())
        owner.observeAccount(130)
        assertThrows(IllegalArgumentException::class.java) {
            owner.observeBox(130, "box-a", "pulling", 101.0)
        }
        owner.observeBox(130, "box-a", "running")
        repeat(20) { index ->
            val phase = if (index % 2 == 0) {
                ComputerRebuildMigrationPhase.BACKING_UP
            } else {
                ComputerRebuildMigrationPhase.CREATING
            }
            owner.observeMigration(130, null, phase)
        }
        assertEquals(16, owner.snapshot(130).migrationPhases.size)
    }


    @Test
    fun queuedUpdateSurvivesProcessDeathWithoutBecomingOutcomeUnknown() {
        val store = MemoryStore()
        val owner = ComputerRebuildStateOwner(store, processGeneration = 140)
        owner.observeAccount(140)
        owner.observeBox(
            accountEpoch = 140,
            boxId = "forever-box",
            phase = "running",
            imageUpdateAvailable = true,
        )
        val queued = owner.queueUpdateWhenIdle(140)
        assertTrue(queued.queuedWhenIdle)
        assertFalse(queued.pending)
        assertFalse(queued.outcomeUnknown)

        val reopened = ComputerRebuildStateOwner(store, processGeneration = 141)
        val restored = reopened.snapshot(140)
        assertTrue(restored.queuedWhenIdle)
        assertFalse(restored.pending)
        assertFalse(restored.outcomeUnknown)

        val reserved = reopened.reserveQueuedRequest(140, "queued-request-140")
        assertFalse(reserved.queuedWhenIdle)
        assertTrue(reserved.pending)
        assertEquals("queued-request-140", reserved.requestId)

        val ambiguous = reopened.markRequestOutcomeUnknown(140, "queued-request-140")
        assertTrue(ambiguous.outcomeUnknown)
        assertThrows(IllegalArgumentException::class.java) {
            reopened.reserveRequest(140, "must-not-replay")
        }
    }

    @Test
    fun queuedUpdateIsAccountFencedCancellableAndClearedWhenUpdateDisappears() {
        val owner = ComputerRebuildStateOwner(MemoryStore())
        owner.observeAccount(150)
        owner.observeBox(150, "forever-box", "running", imageUpdateAvailable = true)
        assertTrue(owner.queueUpdateWhenIdle(150).queuedWhenIdle)
        assertFalse(owner.cancelQueuedUpdate(150).queuedWhenIdle)

        owner.queueUpdateWhenIdle(150)
        val unavailable = owner.observeBox(
            accountEpoch = 150,
            boxId = "forever-box",
            phase = "running",
            imageUpdateAvailable = false,
        )
        assertFalse(unavailable.queuedWhenIdle)

        owner.observeAccount(151)
        assertFalse(owner.snapshot(151).queuedWhenIdle)
        assertThrows(IllegalArgumentException::class.java) {
            owner.queueUpdateWhenIdle(150)
        }
    }


    @Test
    fun pendingStallTelemetryProjectsCanonicalEpisodeAckAndAndroidTransportLiveness() {
        val snapshot = ComputerRebuildSnapshot(
            accountEpoch = 17L,
            kind = ComputerRebuildKind.RECONNECTING,
            operationId = "operation-17",
            acknowledged = true,
            connected = false,
            episodeStartedAtMs = 1_000L,
        )
        assertNull(
            projectComputerRebuildPendingStallTelemetry(
                snapshot,
                ComputerRebuildVisibilityTelemetry(
                    elapsedMs = 599_999L,
                    stageElapsedMs = 1L,
                    isStalled = false,
                    stage = null,
                ),
            ),
        )

        val projected = projectComputerRebuildPendingStallTelemetry(
            snapshot,
            ComputerRebuildVisibilityTelemetry(
                elapsedMs = 600_000L,
                stageElapsedMs = 42L,
                isStalled = true,
                stage = ComputerRebuildMigrationPhase.CREATING,
            ),
        )!!
        assertEquals("17:1000:operation-17", projected.episodeKey)
        assertEquals("reconnecting", projected.rebuildKind)
        assertEquals(600_000L, projected.elapsedMs)
        assertTrue(projected.hasAck)
        assertFalse(projected.streamAttached)
        assertNull(
            projectComputerRebuildPendingStallTelemetry(
                ComputerRebuildSnapshot(accountEpoch = 17L),
                ComputerRebuildVisibilityTelemetry(600_000L, 0L, true, null),
            ),
        )
    }

    @Test
    fun visibilityTelemetryPersistsEpisodeStageAndStallAcrossProcessReopen() {
        val store = MemoryStore()
        var now = 1_000L
        val owner = ComputerRebuildStateOwner(store, processGeneration = 7L, nowMs = { now })
        owner.observeAccount(3L)
        owner.begin(3L, ComputerRebuildKind.UPDATE, "op-visible", ComputerRebuildSource.REQUEST)

        now += 599_999L
        var visibility = owner.visibilityTelemetry(3L)!!
        assertEquals(599_999L, visibility.elapsedMs)
        assertFalse(visibility.isStalled)

        now += 1L
        visibility = owner.visibilityTelemetry(3L)!!
        assertTrue(visibility.isStalled)
        owner.observeMigration(3L, "op-visible", ComputerRebuildMigrationPhase.BACKING_UP)
        now += 3_210L
        visibility = owner.visibilityTelemetry(3L)!!
        assertEquals(ComputerRebuildMigrationPhase.BACKING_UP, visibility.stage)
        assertEquals(3_210L, visibility.stageElapsedMs)

        val reopened = ComputerRebuildStateOwner(store, processGeneration = 8L, nowMs = { now })
        val reopenedVisibility = reopened.visibilityTelemetry(3L)!!
        assertTrue(reopenedVisibility.isStalled)
        assertEquals(603_210L, reopenedVisibility.elapsedMs)
        assertEquals(3_210L, reopenedVisibility.stageElapsedMs)

        reopened.observeAccount(4L)
        assertNull(reopened.visibilityTelemetry(4L))
    }

    @Test
    fun stalledRebuildTelemetryProjectionIsEpisodeScopedAndReadOnly() {
        val store = MemoryStore()
        var now = 5_000L
        val owner = ComputerRebuildStateOwner(store, processGeneration = 12L, nowMs = { now })
        owner.observeAccount(9L)
        owner.reserveRequest(9L, "request-9")
        owner.acceptRequest(
            accountEpoch = 9L,
            requestId = "request-9",
            operationId = "operation-9",
            kind = ComputerRebuildKind.UPDATE,
        )

        assertNull(
            projectComputerRebuildStallTelemetry(
                owner.snapshot(9L),
                owner.visibilityTelemetry(9L),
            ),
        )

        now += COMPUTER_REBUILD_STALLED_AFTER_MS
        val snapshot = owner.snapshot(9L)
        val projection = projectComputerRebuildStallTelemetry(
            snapshot,
            owner.visibilityTelemetry(9L),
        )!!
        assertEquals("update", projection.rebuildKind)
        assertEquals(COMPUTER_REBUILD_STALLED_AFTER_MS, projection.elapsedMs)
        assertTrue(projection.hasAck)
        assertTrue(projection.streamAttached)
        assertTrue(projection.dedupeKey.contains("operation-9"))
        assertEquals(snapshot, owner.snapshot(9L), "telemetry projection must not mutate rebuild truth")

        owner.deactivate(9L)
        assertNull(
            projectComputerRebuildStallTelemetry(
                owner.snapshot(9L),
                owner.visibilityTelemetry(9L),
            ),
        )
    }


}

class AndroidStartupTelemetryTrackerTest {
    @Test
    fun bufferedStartupTelemetryFlushesBeforeReady() {
        var now = 100L
        var scheduledDelay = -1L
        var stuckCallback: (() -> Unit)? = null
        val tracker = AndroidStartupTelemetryTracker(
            monotonicNowMs = { now },
            scheduleStuck = { delay, callback ->
                scheduledDelay = delay
                stuckCallback = callback
            },
        )
        tracker.markPhase("coordinator")
        tracker.armStuckWatchdog()
        val events = mutableListOf<AndroidStartupTelemetryEvent>()
        tracker.attach(events::add)
        assertEquals(ANDROID_STARTUP_STUCK_MS, scheduledDelay)
        assertEquals(listOf("start"), events.map { it.outcome })

        now = 120_200L
        stuckCallback!!.invoke()
        tracker.markPhase("window")
        now = 120_260L
        tracker.noteReady()
        assertEquals(listOf("start", "stuck", "ready"), events.map { it.outcome })
        assertEquals("coordinator", events[1].phase)
        assertEquals(120_100L, events[1].elapsedMs)
        assertEquals(120_100L, events[1].phaseElapsedMs)
        assertEquals(60L, events[2].windowMs)
    }

    @Test
    fun startupFailureSettlesOnceAndSuppressesLateWatchdog() {
        var now = 5L
        var stuckCallback: (() -> Unit)? = null
        val tracker = AndroidStartupTelemetryTracker(
            monotonicNowMs = { now },
            scheduleStuck = { _, callback -> stuckCallback = callback },
        )
        tracker.armStuckWatchdog()
        tracker.markPhase("services")
        val events = mutableListOf<AndroidStartupTelemetryEvent>()
        tracker.attach(events::add)
        now = 25L
        tracker.noteFailed()
        tracker.noteReady()
        now = 130_000L
        stuckCallback!!.invoke()
        assertEquals(listOf("start", "failed"), events.map { it.outcome })
        assertEquals("services", events.last().phase)
        assertEquals(20L, events.last().elapsedMs)
    }
}

