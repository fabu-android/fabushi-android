package com.ombhrum.fabushi

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteCommandJournalTest {
    @Test
    fun duplicateTerminalRequestReplaysButMismatchedSessionFailsClosed() {
        val dir = createTempDir(prefix = "remote-command-journal-")
        try {
            val journal = RemoteCommandJournal(File(dir, "journal.properties"))
            val first = journal.begin("request-1", "device-a", "session-a", "fabushi.app.action")
            assertTrue(first is RemoteCommandAdmission.Execute)
            journal.complete("request-1", "{\"ok\":true}")
            val replay = journal.begin("request-1", "device-a", "session-a", "fabushi.app.action")
            assertTrue(replay is RemoteCommandAdmission.Replay)
            assertEquals(
                RemoteCommandState.COMPLETED,
                (replay as RemoteCommandAdmission.Replay).record.state,
            )
            val mismatch = runCatching {
                journal.begin("request-1", "device-a", "session-b", "fabushi.app.action")
            }.exceptionOrNull()
            assertEquals("remote_command_identity_session_mismatch", mismatch?.message)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun processRestartTurnsPendingSideEffectIntoOutcomeUnknown() {
        val dir = createTempDir(prefix = "remote-command-restart-")
        try {
            val file = File(dir, "journal.properties")
            RemoteCommandJournal(file).begin(
                "request-2",
                "device-a",
                "session-a",
                "fabushi.app.action",
            )
            val recovered = RemoteCommandJournal(file)
            assertEquals(
                RemoteCommandState.OUTCOME_UNKNOWN,
                recovered.get("request-2")?.state,
            )
            val replay = runCatching {
                recovered.begin("request-2", "device-a", "session-a", "fabushi.app.action")
            }.exceptionOrNull()
            assertEquals("remote_command_outcome_unknown_reconcile_required", replay?.message)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun disconnectMarksAllPendingCommandsOutcomeUnknown() {
        val dir = createTempDir(prefix = "remote-command-disconnect-")
        try {
            val file = File(dir, "journal.properties")
            val journal = RemoteCommandJournal(file)
            journal.begin("request-3", "device-a", "session-a", "fabushi.app.action")
            journal.markPendingOutcomeUnknown("disconnect")
            assertEquals(RemoteCommandState.OUTCOME_UNKNOWN, journal.get("request-3")?.state)
        } finally {
            dir.deleteRecursively()
        }
    }
}
