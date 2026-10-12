package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.CoordinatorPortBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Test

class CoordinatorPortBridgeTest {
    private fun fakePort(): AndroidCoordinatorPort =
        Proxy.newProxyInstance(
            AndroidCoordinatorPort::class.java.classLoader,
            arrayOf(AndroidCoordinatorPort::class.java),
        ) { _, method, _ ->
            when {
                method.returnType == java.lang.Boolean.TYPE -> false
                method.returnType == java.lang.Integer.TYPE -> 0
                method.returnType == java.lang.Long.TYPE -> 0L
                else -> null
            }
        } as AndroidCoordinatorPort

    @Test
    fun bridgeEnforcesOnePresentationOwnerUntilMatchingRelease() {
        val port = fakePort()
        val bridge = CoordinatorPortBridge(port)

        assertSame(port, bridge.claim("presentation-a"))
        assertSame(port, bridge.claim("presentation-a"))
        assertEquals("presentation-a", bridge.currentOwner())

        try {
            bridge.claim("presentation-b")
            fail("a second presentation owner must not claim the Coordinator port")
        } catch (_: IllegalStateException) {
            // Expected: the typed port is a single-owner capability.
        }

        bridge.release("presentation-b")
        assertEquals("presentation-a", bridge.currentOwner())

        bridge.release("presentation-a")
        assertNull(bridge.currentOwner())
        assertSame(port, bridge.claim("presentation-b"))
        assertEquals("presentation-b", bridge.currentOwner())
    }

    @Test
    fun blankOwnerCannotClaimBridge() {
        val bridge = CoordinatorPortBridge(fakePort())
        try {
            bridge.claim("   ")
            fail("blank owner must be rejected")
        } catch (_: IllegalArgumentException) {
            // Expected.
        }
        assertNull(bridge.currentOwner())
    }
}
