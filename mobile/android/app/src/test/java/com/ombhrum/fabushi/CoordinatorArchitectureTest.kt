package com.ombhrum.fabushi
import com.ombhrum.fabushi.androidmain.coordinator.CoordinatorProcessRuntime
import com.ombhrum.fabushi.androidmain.coordinator.CoordinatorResyncTracker
import com.ombhrum.fabushi.androidmain.coordinator.InMemoryCoordinatorEpochStore
import com.ombhrum.fabushi.androidpreload.BoxVncClipboardPaste
import com.ombhrum.fabushi.androidpreload.EdgeReply
import com.ombhrum.fabushi.androidpreload.RpcEdgeRuntime
import com.ombhrum.fabushi.androidpreload.ViewerVisibilityGate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CoordinatorArchitectureTest {
    @Test fun processRecreationAdvancesPersistentGenerationAndRejectsStaleResync() {
        val store = InMemoryCoordinatorEpochStore()
        val first = CoordinatorProcessRuntime(store)
        val generation1 = first.start()
        first.crash("process killed")
        val recreated = CoordinatorProcessRuntime(store)
        val generation2 = recreated.start()
        assertTrue(generation2 > generation1)
        val resync = CoordinatorResyncTracker()
        assertTrue(resync.accept(generation2, 1))
        assertFalse(resync.accept(generation1, 99))
        assertFalse(resync.accept(generation2, 1))
        assertTrue(resync.accept(generation2, 2))
    }

    @Test fun typedEdgeFailsClosedForUnknownMethodAndHandlerFailure() {
        val edge = RpcEdgeRuntime()
        assertTrue(edge.call("missing", null) is EdgeReply.Failure)
        edge.register("ok") { "value" }
        assertEquals(EdgeReply.Success("value"), edge.call("ok", null))
        edge.register("bad") { error("boom") }
        assertTrue(edge.call("bad", null) is EdgeReply.Failure)
    }

    @Test fun viewerVisibilityAndClipboardProjectionAreDeterministic() {
        val gate = ViewerVisibilityGate()
        gate.setViewerRequested(true)
        assertTrue(gate.isViewerVisible())
        gate.setAppVisible(false)
        assertFalse(gate.isViewerVisible())
        assertEquals("a\nb", BoxVncClipboardPaste.normalizeForRemote("a\r\nb"))
    }
}
