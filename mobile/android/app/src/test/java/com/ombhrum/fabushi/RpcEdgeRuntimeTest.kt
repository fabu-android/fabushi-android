package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.EDGE_HANDLER_FAILED
import com.ombhrum.fabushi.androidpreload.EDGE_UNKNOWN_METHOD
import com.ombhrum.fabushi.androidpreload.EdgeReply
import com.ombhrum.fabushi.androidpreload.RpcEdgeRuntime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RpcEdgeRuntimeTest {
    @Test
    fun callsFailClosedWithDesktopCompatibleErrorCodes() {
        val edge = RpcEdgeRuntime()

        val unknown = edge.call("missing", null)
        assertTrue(unknown is EdgeReply.Failure)
        assertEquals(EDGE_UNKNOWN_METHOD, (unknown as EdgeReply.Failure).code)

        edge.register("explode") { error("boom") }
        val failed = edge.call("explode", null)
        assertTrue(failed is EdgeReply.Failure)
        assertEquals(EDGE_HANDLER_FAILED, (failed as EdgeReply.Failure).code)

        edge.register("echo") { payload -> payload }
        assertEquals(EdgeReply.Success("value"), edge.call("echo", "value"))
    }

    @Test
    fun subscriptionCloseRemovesOnlyItsOwnListeners() {
        val edge = RpcEdgeRuntime()
        val first = mutableListOf<Any?>()
        val second = mutableListOf<Any?>()

        val firstSubscription = edge.subscribe(mapOf("changed" to first::add))
        val secondSubscription = edge.subscribe(mapOf("changed" to second::add))
        assertEquals(2, edge.listenerCount("changed"))

        edge.emit("changed", 1)
        assertEquals(listOf(1), first)
        assertEquals(listOf(1), second)

        firstSubscription.close()
        assertEquals(1, edge.listenerCount("changed"))
        edge.emit("changed", 2)
        assertEquals(listOf(1), first)
        assertEquals(listOf(1, 2), second)

        secondSubscription.close()
        assertEquals(0, edge.listenerCount("changed"))
        edge.emit("changed", 3)
        assertEquals(listOf(1), first)
        assertEquals(listOf(1, 2), second)
    }
}
