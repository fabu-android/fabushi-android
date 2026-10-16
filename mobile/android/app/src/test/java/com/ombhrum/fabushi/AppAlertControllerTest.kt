package com.ombhrum.fabushi

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AppAlertControllerTest {
    private fun scope() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @Test fun confirmCancelQueueAndThirdRequestFollowDesktopContract() {
        val scope = scope()
        val controller = AppAlertController(scope)
        val settled = mutableListOf<Pair<String, Boolean>>()

        controller.alert(
            AppAlertRequest("Delete", confirmLabel = "Delete", cancelLabel = "Cancel"),
        ) { settled += "first" to it }
        assertEquals("Delete", controller.getSnapshot()?.request?.title)

        controller.alert(
            AppAlertRequest("cancellable", confirmLabel = "OK", cancelLabel = "Cancel"),
        ) { settled += "rejected" to it }
        assertTrue(settled.contains("rejected" to false))

        controller.confirm()
        assertTrue(settled.contains("first" to true))
        assertNull(controller.getSnapshot())

        controller.alert(AppAlertRequest("A", confirmLabel = "Continue")) {
            settled += "A" to it
        }
        controller.alert(AppAlertRequest("B", confirmLabel = "Continue")) {
            settled += "B" to it
        }
        controller.alert(AppAlertRequest("C", confirmLabel = "Continue")) {
            settled += "C" to it
        }
        assertTrue(settled.contains("C" to false))

        controller.confirm()
        assertEquals("B", controller.getSnapshot()?.request?.title)
        controller.cancel()
        assertTrue(settled.contains("A" to true))
        assertTrue(settled.contains("B" to false))
        scope.cancel()
    }

    @Test fun performFailureThrowRetrySecondaryAndPendingSuppressionAreDeterministic() = runBlocking {
        val scope = scope()
        val controller = AppAlertController(scope)
        val gate = CompletableDeferred<String?>()
        var settlements = 0
        controller.alert(
            AppAlertRequest(
                title = "Retryable",
                confirmLabel = "Retry",
                pendingLabel = "Working",
                cancelLabel = "Cancel",
                perform = { gate.await() },
            ),
        ) { settlements += 1 }

        controller.confirm()
        assertTrue(controller.getSnapshot()?.isPerforming == true)
        controller.cancel()
        controller.confirm()
        assertEquals(0, settlements)

        gate.complete("provider unavailable")
        yield()
        assertFalse(controller.getSnapshot()?.isPerforming == true)
        assertEquals("provider unavailable", controller.getSnapshot()?.failure)

        controller.reset()
        var secondaryRuns = 0
        controller.alert(
            AppAlertRequest(
                title = "Secondary",
                confirmLabel = "Primary",
                secondary = AppAlertSecondaryAction("Other", destructive = true) {
                    secondaryRuns += 1
                    null
                },
            ),
        ) { settlements += if (it) 10 else 100 }
        controller.confirmSecondary()
        yield()
        assertEquals(1, secondaryRuns)
        assertNull(controller.getSnapshot())

        controller.alert(
            AppAlertRequest(
                title = "Throw",
                confirmLabel = "Try",
                perform = { error("network down") },
            ),
        ) {}
        controller.confirm()
        yield()
        assertEquals("network down", controller.getSnapshot()?.failure)
        controller.cancel()
        scope.cancel()
    }

    @Test fun resetDisposeLateCompletionSubscribersAndRecreationAreFenced() = runBlocking {
        val scope = scope()
        val controller = AppAlertController(scope)
        val gate = CompletableDeferred<String?>()
        val settled = mutableListOf<Boolean>()
        val subscription = controller.subscribe {}
        assertEquals(1, controller.subscriberCountForTest())

        controller.alert(
            AppAlertRequest("Active", confirmLabel = "Go", perform = { gate.await() }),
        ) { settled += it }
        controller.alert(AppAlertRequest("Waiting", confirmLabel = "Go")) { settled += it }
        controller.confirm()
        assertTrue(controller.getSnapshot()?.isPerforming == true)

        controller.reset()
        assertEquals(listOf(false, false), settled)
        assertNull(controller.getSnapshot())
        gate.complete(null)
        yield()
        assertNull(controller.getSnapshot())

        subscription.close()
        assertEquals(0, controller.subscriberCountForTest())
        controller.dispose()
        controller.alert(AppAlertRequest("After dispose", confirmLabel = "No")) { settled += it }
        assertEquals(false, settled.last())

        val recreated = AppAlertController(scope())
        assertNull(recreated.getSnapshot())
        recreated.dispose()
        scope.cancel()
    }
}
