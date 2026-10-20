package com.ombhrum.fabushi

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.diagnostics.AndroidProcessCrashMarker
import com.ombhrum.fabushi.androidmain.diagnostics.AndroidProcessCrashRecovery
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidProcessCrashRecoveryInstrumentedTest {
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    private val marker = File(context.filesDir, "telemetry/android-process-crash.v1.json")

    @Before
    fun resetMarker() {
        marker.delete()
    }

    @After
    fun cleanup() {
        marker.delete()
    }

    @Test
    fun markerSurvivesReopenAndRequiresExactAcknowledgement() {
        val expected = AndroidProcessCrashMarker(
            kind = "uncaught_exception",
            errorClass = "IllegalStateException",
            occurredAtMs = 12_345L,
            uptimeMs = 4_321L,
        )
        AndroidProcessCrashRecovery(context).persist(expected)

        val reopened = AndroidProcessCrashRecovery(context)
        assertEquals(expected, reopened.pending())
        assertTrue(marker.exists())

        val stale = expected.copy(occurredAtMs = expected.occurredAtMs + 1L)
        assertFalse(reopened.acknowledge(stale))
        assertEquals(expected, reopened.pending())
        assertTrue(marker.exists())

        assertTrue(reopened.acknowledge(expected))
        assertNull(reopened.pending())
        assertFalse(marker.exists())
    }

    @Test
    fun persistedMarkerContainsOnlyBoundedCrashMetadata() {
        val recovery = AndroidProcessCrashRecovery(context)
        recovery.persist(
            AndroidProcessCrashMarker(
                kind = "uncaught_exception",
                errorClass = "RuntimeException",
                occurredAtMs = 7L,
                uptimeMs = 5L,
            ),
        )

        val raw = marker.readText()
        assertTrue(raw.contains("RuntimeException"))
        assertFalse(raw.contains("message"))
        assertFalse(raw.contains("stack"))
        assertFalse(raw.contains("token"))
        assertFalse(raw.contains("secret"))
    }

    @Test
    fun invalidCrashMetadataFailsClosedWithoutCreatingMarker() {
        val recovery = AndroidProcessCrashRecovery(context)

        assertThrows(IllegalArgumentException::class.java) {
            recovery.persist(
                AndroidProcessCrashMarker(
                    kind = "signal",
                    errorClass = "Signal",
                    occurredAtMs = 1L,
                    uptimeMs = 1L,
                ),
            )
        }
        assertFalse(marker.exists())

        assertThrows(IllegalArgumentException::class.java) {
            recovery.persist(
                AndroidProcessCrashMarker(
                    kind = "uncaught_exception",
                    errorClass = "IllegalStateException: secret",
                    occurredAtMs = 1L,
                    uptimeMs = 1L,
                ),
            )
        }
        assertFalse(marker.exists())
    }
}
