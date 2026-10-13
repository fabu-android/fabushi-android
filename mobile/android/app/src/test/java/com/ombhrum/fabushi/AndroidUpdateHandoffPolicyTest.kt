package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateHandoffPolicyTest {
    @Test
    fun quiesceFailureDoesNotPersistMarker() {
        var persisted = false

        val result = prepareAndroidUpdateInstallerHandoff(
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

        val result = prepareAndroidUpdateInstallerHandoff(
            setQuiescing = { value ->
                transitions += value
                true
            },
            persistRequested = { persisted = true },
        )

        assertEquals(AndroidUpdateHandoffPreparation.READY, result)
        assertEquals(listOf(true), transitions)
        assertTrue(persisted)
    }
}
