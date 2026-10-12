package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.VncLivenessCounters
import com.ombhrum.fabushi.androidpreload.VncLivenessDetector
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VncLivenessDetectorTest {
    @Test
    fun impactfulInputWithoutRemoteProgressReportsOneStallEpisode() {
        val detector = VncLivenessDetector()

        assertNull(detector.sample(VncLivenessCounters(frames = 10, inputEvents = 0, observedAtMs = 0)))
        assertNull(detector.sample(VncLivenessCounters(frames = 10, inputEvents = 1, observedAtMs = 1_000)))
        assertNull(detector.sample(VncLivenessCounters(frames = 10, inputEvents = 2, observedAtMs = 5_000)))

        val stall = detector.sample(
            VncLivenessCounters(frames = 10, inputEvents = 3, observedAtMs = 10_000),
        )
        requireNotNull(stall)
        assertEquals(9_000L, stall.stallMs)
        assertEquals(3L, stall.impactfulInputs)
        assertEquals(0L, stall.inBytes)

        assertNull(
            detector.sample(
                VncLivenessCounters(frames = 10, inputEvents = 4, observedAtMs = 11_000),
            ),
        )
    }

    @Test
    fun inputIsDemandNotLivenessAndRemoteProgressRearmsDetector() {
        val detector = VncLivenessDetector(staleAfterMs = 10_000L, minimumImpactfulInputs = 3L)

        assertTrue(detector.observe(VncLivenessCounters(1, 0, 0)))
        assertTrue(detector.observe(VncLivenessCounters(1, 2, 5_000)))
        assertFalse(detector.observe(VncLivenessCounters(1, 3, 10_000)))

        // A received frame is authoritative remote progress and rearms future stall detection.
        assertTrue(detector.observe(VncLivenessCounters(2, 3, 11_000)))
        assertTrue(detector.observe(VncLivenessCounters(2, 6, 15_000)))
        assertTrue(detector.observe(VncLivenessCounters(2, 6, 21_000)))
        assertFalse(detector.observe(VncLivenessCounters(2, 9, 22_000)))
    }

    @Test
    fun inboundBytesSuppressFalsePositiveAndCounterResetRebaselines() {
        val detector = VncLivenessDetector()

        assertNull(detector.sample(VncLivenessCounters(4, 0, 0, inBytes = 100)))
        assertNull(detector.sample(VncLivenessCounters(4, 3, 10_000, inBytes = 140)))

        // Reconnect/counter reset must never be interpreted as a stall.
        assertNull(detector.sample(VncLivenessCounters(0, 0, 11_000, inBytes = 0)))
        assertNull(detector.sample(VncLivenessCounters(0, 3, 21_000, inBytes = 0)))
        assertEquals(
            3L,
            detector.sample(VncLivenessCounters(0, 4, 22_000, inBytes = 0))
                ?.impactfulInputs,
        )
    }
}
