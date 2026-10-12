package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AndroidEdgeFailure
import com.ombhrum.fabushi.androidmain.coordinator.AndroidEdgeFailureBuffer
import org.junit.Assert.assertEquals
import org.junit.Test

class AndroidEdgeFailureReporterTest {
    @Test
    fun preInstallBufferIsBoundedAndFlushesInOrder() {
        val owner = AndroidEdgeFailureBuffer(preInstallCapacity = 2)
        owner.report(AndroidEdgeFailure("account", "refresh", "Timeout"))
        owner.report(AndroidEdgeFailure("mcp", "connect", "ProtocolFailure"))
        owner.report(AndroidEdgeFailure("remote", "dispatch", "Dropped"))

        val received = mutableListOf<AndroidEdgeFailure>()
        owner.install(received::add)

        assertEquals(
            listOf(
                AndroidEdgeFailure("account", "refresh", "Timeout"),
                AndroidEdgeFailure("mcp", "connect", "ProtocolFailure"),
            ),
            received,
        )
    }

    @Test
    fun installedReporterReceivesFailuresWithoutRebuffering() {
        val owner = AndroidEdgeFailureBuffer()
        val received = mutableListOf<AndroidEdgeFailure>()
        owner.install(received::add)
        owner.report(AndroidEdgeFailure("host", "native-create", "NativeInitializationFailure"))
        assertEquals(
            listOf(AndroidEdgeFailure("host", "native-create", "NativeInitializationFailure")),
            received,
        )
    }
}
