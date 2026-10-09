package com.ombhrum.fabushi

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class CommerceOperationJournalTest {
    @Test
    fun pendingPurchaseKeepsSameKeyAcrossProcessRestart() {
        val dir = createTempDir(prefix = "commerce-journal-")
        try {
            val file = File(dir, "journal.properties")
            val first = CommerceOperationJournal(file)
            val key = first.stableIdempotencyKey("acct-a", "global-dharma", "lifetime")
            val reopened = CommerceOperationJournal(file)
            assertEquals(
                key,
                reopened.stableIdempotencyKey("acct-a", "global-dharma", "lifetime"),
            )
            assertEquals(
                CommerceOperationState.PENDING,
                reopened.record("acct-a", "global-dharma", "lifetime")?.state,
            )
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun accountFenceNeverReusesAnotherAccountsMutationIdentity() {
        val dir = createTempDir(prefix = "commerce-account-")
        try {
            val journal = CommerceOperationJournal(File(dir, "journal.properties"))
            val first = journal.stableIdempotencyKey("acct-a", "global-dharma", "lifetime")
            val second = journal.stableIdempotencyKey("acct-b", "global-dharma", "lifetime")
            assertNotEquals(first, second)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun confirmedPurchaseStaysTerminalAndRejectsForeignKey() {
        val dir = createTempDir(prefix = "commerce-confirmed-")
        try {
            val journal = CommerceOperationJournal(File(dir, "journal.properties"))
            val key = journal.stableIdempotencyKey("acct-a", "global-dharma", "lifetime")
            journal.markConfirmed("acct-a", "global-dharma", "lifetime", key)
            assertEquals(
                CommerceOperationState.CONFIRMED,
                journal.record("acct-a", "global-dharma", "lifetime")?.state,
            )
            val failure = runCatching {
                journal.markConfirmed("acct-a", "global-dharma", "lifetime", "foreign-key")
            }.exceptionOrNull()
            assertEquals("commerce_idempotency_key_mismatch", failure?.message)
        } finally {
            dir.deleteRecursively()
        }
    }
}
