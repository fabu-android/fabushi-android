package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateHandoffPolicyTest {
    @Test
    fun busyHostDoesNotQuiesceOrPersistMarker() {
        val transitions = mutableListOf<Boolean>()
        var persisted = false

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = { false },
            setQuiescing = { value ->
                transitions += value
                true
            },
            persistRequested = { persisted = true },
        )

        assertEquals(AndroidUpdateHandoffPreparation.HOST_NOT_IDLE, result)
        assertTrue(transitions.isEmpty())
        assertFalse(persisted)
    }

    @Test
    fun hostBecomingBusyAcrossQuiesceRaceIsResumedAndRejected() {
        val transitions = mutableListOf<Boolean>()
        var idleChecks = 0
        var persisted = false

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = {
                idleChecks += 1
                idleChecks == 1
            },
            setQuiescing = { value ->
                transitions += value
                true
            },
            persistRequested = { persisted = true },
        )

        assertEquals(AndroidUpdateHandoffPreparation.HOST_NOT_IDLE, result)
        assertEquals(listOf(true, false), transitions)
        assertFalse(persisted)
    }

    @Test
    fun busyRaceSurfacesResumeFailure() {
        var idleChecks = 0
        val transitions = mutableListOf<Boolean>()

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = {
                idleChecks += 1
                idleChecks == 1
            },
            setQuiescing = { value ->
                transitions += value
                value
            },
            persistRequested = { error("must not persist") },
        )

        assertEquals(AndroidUpdateHandoffPreparation.HOST_NOT_IDLE_RESUME_FAILED, result)
        assertEquals(listOf(true, false), transitions)
    }

    @Test
    fun quiesceFailureDoesNotPersistMarker() {
        var persisted = false

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = { true },
            setQuiescing = { false },
            persistRequested = { persisted = true },
        )

        assertEquals(AndroidUpdateHandoffPreparation.HOST_QUIESCE_FAILED, result)
        assertFalse(persisted)
    }

    @Test
    fun markerFailureResumesHostBeforeReturningFailure() {
        val transitions = mutableListOf<Boolean>()

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = { true },
            setQuiescing = { value ->
                transitions += value
                true
            },
            persistRequested = { error("durable marker write failed") },
        )

        assertEquals(AndroidUpdateHandoffPreparation.MARKER_PERSIST_FAILED, result)
        assertEquals(listOf(true, false), transitions)
    }

    @Test
    fun markerFailureSurfacesResumeFailure() {
        val transitions = mutableListOf<Boolean>()

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = { true },
            setQuiescing = { value ->
                transitions += value
                value
            },
            persistRequested = { error("durable marker write failed") },
        )

        assertEquals(AndroidUpdateHandoffPreparation.MARKER_PERSIST_FAILED_RESUME_FAILED, result)
        assertEquals(listOf(true, false), transitions)
    }

    @Test
    fun successfulPreparationKeepsHostQuiescedForInstallerHandoff() {
        val transitions = mutableListOf<Boolean>()
        var persisted = false
        var idleChecks = 0

        val result = prepareAndroidUpdateInstallerHandoff(
            isHostIdle = {
                idleChecks += 1
                true
            },
            setQuiescing = { value ->
                transitions += value
                true
            },
            persistRequested = { persisted = true },
        )

        assertEquals(AndroidUpdateHandoffPreparation.READY, result)
        assertEquals(2, idleChecks)
        assertEquals(listOf(true), transitions)
        assertTrue(persisted)
    }
}
