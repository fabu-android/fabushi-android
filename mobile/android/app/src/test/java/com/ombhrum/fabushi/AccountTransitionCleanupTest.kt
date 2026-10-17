package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AccountTransitionCleanup
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AccountTransitionCleanupTest {
    @Test
    fun cleanupContinuesAfterFailureAndCollectsError() {
        val observed = mutableListOf<String>()
        val cleanup = AccountTransitionCleanup()

        // add() is LIFO so register in reverse execution order.
        cleanup.add { observed += "protected-binding" }
        cleanup.add {
            observed += "pairing"
            error("pairing clear failed")
        }
        cleanup.add { observed += "remote-session" }
        cleanup.add { observed += "send-queue" }

        val failures = cleanup.runAll()

        assertEquals(
            listOf("send-queue", "remote-session", "pairing", "protected-binding"),
            observed,
        )
        assertEquals(1, failures.size)
        assertTrue(failures.single().message.orEmpty().contains("pairing clear failed"))
    }

    @Test
    fun cleanupRunsAtMostOnce() {
        var calls = 0
        val cleanup = AccountTransitionCleanup()
        cleanup.add { calls += 1 }

        assertTrue(cleanup.runAll().isEmpty())
        assertTrue(cleanup.runAll().isEmpty())
        assertEquals(1, calls)
    }
}
