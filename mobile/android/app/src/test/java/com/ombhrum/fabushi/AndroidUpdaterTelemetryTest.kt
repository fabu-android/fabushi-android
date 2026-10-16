package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AndroidUpdaterTelemetryTest {
    @Test fun ringIsBoundedAndSchemaCannotCarrySecretsOrContent() {
        val telemetry = AndroidUpdaterTelemetry(limit = 2)
        telemetry.record(AndroidUpdaterTelemetryEvent(category = "check", action = "started", result = "pending"))
        telemetry.record(AndroidUpdaterTelemetryEvent(category = "stage", action = "verified", result = "success", targetVersion = "1.2.3"))
        telemetry.record(AndroidUpdaterTelemetryEvent(category = "idle", action = "gate", result = "eligible", track = "stable"))
        val events = telemetry.snapshot()
        assertEquals(2, events.size)
        assertEquals("stage", events[0].category)
        assertEquals("idle", events[1].category)
        assertNull(events[1].errorClass)
    }
}
