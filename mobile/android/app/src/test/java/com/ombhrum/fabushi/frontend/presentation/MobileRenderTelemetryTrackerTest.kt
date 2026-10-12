package com.ombhrum.fabushi.frontend.presentation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MobileRenderTelemetryTrackerTest {
    @Test
    fun firstChunkReportsTtfrAndTerminalStreamMetrics() {
        var monotonic = 1_000L
        var epoch = 50_000L
        val tracker = MobileRenderTelemetryTracker(
            monotonicNowMs = { monotonic },
            epochNowMs = { epoch },
        )

        tracker.begin("operation-1")
        monotonic = 1_125L
        val ttfr = tracker.observeChunk("operation-1", "assistant_text")
        requireNotNull(ttfr)
        assertEquals(125L, ttfr.ttfrMs)
        assertEquals(50_000L, ttfr.enterEpochMs)

        monotonic = 1_400L
        assertNull(tracker.observeChunk("operation-1", "assistant_text"))
        monotonic = 2_700L
        assertNull(tracker.observeChunk("operation-1", "assistant_text"))
        monotonic = 3_000L

        val stream = tracker.finish("operation-1", "turn_ended")
        requireNotNull(stream)
        assertEquals(3, stream.chunkCount)
        assertEquals(1_300L, stream.maxGapMs)
        assertEquals(1_300L, stream.stalledMs)
        assertEquals(1, stream.stallCount)
        assertEquals(2_000L, stream.streamMs)
        assertNull(tracker.finish("operation-1", "turn_ended"))
    }

    @Test
    fun duplicateBeginDoesNotResetEntryTimeAndWrongOperationIsIgnored() {
        var monotonic = 10L
        val tracker = MobileRenderTelemetryTracker(
            monotonicNowMs = { monotonic },
            epochNowMs = { 123L },
        )
        tracker.begin("operation-1")
        monotonic = 25L
        tracker.begin("operation-1")
        assertNull(tracker.observeChunk("operation-2", "assistant_text"))

        monotonic = 40L
        val ttfr = tracker.observeChunk("operation-1", "assistant_text")
        requireNotNull(ttfr)
        assertEquals(30L, ttfr.ttfrMs)

        tracker.abandon("operation-1")
        assertNull(tracker.finish("operation-1", "superseded"))
    }
}
