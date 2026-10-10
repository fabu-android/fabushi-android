package com.ombhrum.fabushi.androidmain.security

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteControlSessionCredentialPolicyTest {
    private val validToken = "m".repeat(64)

    @Test
    fun roundTripKeepsMobileCredentialSecretFromPresentationProjection() {
        val credential = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )

        val parsed = RemoteControlSessionCredential.parse(credential.toSecretJson())
        assertEquals(credential, parsed)
        val projection = parsed.publicProjection()
        assertEquals("computer-1", projection.getString("deviceId"))
        assertEquals("remote-session-1", projection.getString("sessionId"))
        assertFalse(projection.has("mobileToken"))
        assertFalse(projection.has("requestId"))
        assertFalse(projection.has("iceServersJson"))
        assertFalse(projection.has("accountFence"))
    }

    @Test
    fun rejectsCredentialPlaneConfusionAndUnknownSecretFields() {
        val base = JSONObject()
            .put("deviceId", "computer-1")
            .put("clientId", "remote-client-1")
            .put("sessionId", "remote-session-1")
            .put("mobileToken", validToken)
            .put("accountFence", "session:account-1")
            .put("accountEpoch", 7)
            .put("expiresAt", 2_000_000_000)

        val wrongClientPlane = JSONObject(base.toString())
        wrongClientPlane.remove("mobileToken")
        wrongClientPlane.put("clientToken", validToken)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(wrongClientPlane.toString())
        }

        val wrongExecutorPlane = JSONObject(base.toString())
            .put("bearerCredential", validToken)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(wrongExecutorPlane.toString())
        }
    }

    @Test
    fun transportRouteAllowsDirectFallbackButNeverRelayUpgrade() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )
        val direct = RemoteControlTransportPolicy.record(
            base,
            "remote-session-1",
            "fabushi-webrtc",
            "direct-preferred",
            "direct",
            null,
            100,
        )
        val relay = RemoteControlTransportPolicy.record(
            direct,
            "remote-session-1",
            "fabushi-webrtc",
            "direct-preferred",
            "relay",
            "us-west",
            101,
        )
        assertEquals("relay", relay.selectedRoute)
        assertEquals("us-west", relay.relayRegion)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlTransportPolicy.record(
                relay,
                "remote-session-1",
                "fabushi-webrtc",
                "direct-preferred",
                "direct",
                null,
                102,
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlTransportPolicy.record(
                relay,
                "remote-session-1",
                "other-provider",
                "direct-preferred",
                "relay",
                "us-west",
                102,
            )
        }
    }

    @Test
    fun relayOnlyTransportCannotRecordDirectRoute() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlTransportPolicy.record(
                base,
                "remote-session-1",
                "fabushi-webrtc",
                "relay-only",
                "direct",
                null,
                100,
            )
        }
    }

    @Test
    fun legacyCredentialMigratesWithZeroSignalCursors() {
        val legacy = JSONObject()
            .put("deviceId", "computer-1")
            .put("clientId", "remote-client-1")
            .put("sessionId", "remote-session-1")
            .put("mobileToken", validToken)
            .put("accountFence", "session:account-1")
            .put("accountEpoch", 7)
            .put("expiresAt", 2_000_000_000)

        val parsed = RemoteControlSessionCredential.parse(legacy.toString())
        assertEquals(0L, parsed.lastAcknowledgedSignalId)
        assertEquals(0L, parsed.highestDrainedSignalId)
    }

    @Test
    fun signalCursorRequiresDrainBeforeWholeBatchAcknowledgement() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )

        val drained = RemoteControlSignalCursorPolicy.recordDrain(
            base,
            "remote-session-1",
            0,
            9,
        )
        assertEquals(0L, drained.lastAcknowledgedSignalId)
        assertEquals(9L, drained.highestDrainedSignalId)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSignalCursorPolicy.acknowledge(drained, "remote-session-1", 8)
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSignalCursorPolicy.recordDrain(
                drained,
                "remote-session-1",
                0,
                0,
            )
        }
        val acknowledged = RemoteControlSignalCursorPolicy.acknowledge(
            drained,
            "remote-session-1",
            9,
        )
        assertEquals(9L, acknowledged.lastAcknowledgedSignalId)
        assertEquals(9L, acknowledged.highestDrainedSignalId)
        assertTrue(
            runCatching {
                RemoteControlSignalCursorPolicy.recordDrain(
                    acknowledged,
                    "remote-session-1",
                    9,
                    9,
                )
            }.isSuccess,
        )
    }

    @Test
    fun processRestartFencesViewportAndRequiresReconciliation() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
            processGeneration = 10,
            viewportRevision = 4,
            lifecycle = RemoteControlSessionLifecycle.READY,
        )

        val recovered = RemoteControlSessionStatePolicy.bindProcess(base, 11)
        assertEquals(11L, recovered.processGeneration)
        assertEquals(5L, recovered.viewportRevision)
        assertEquals(1, recovered.reconnectCount)
        assertEquals(RemoteControlSessionLifecycle.RECONNECTING, recovered.lifecycle)
        assertTrue(recovered.reconcileRequired)
        assertFalse(recovered.humanTakeover)
    }

    @Test
    fun humanTakeoverIsRevisionFencedAndBumpsViewportOnEachLeaseTransfer() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
            processGeneration = 9,
            viewportRevision = 12,
            selectedRoute = "direct",
            provider = "fabushi-webrtc",
            routePolicy = "direct-preferred",
            transportUpdatedAt = 100,
            lifecycle = RemoteControlSessionLifecycle.NEGOTIATING,
        )

        val takeover = RemoteControlSessionStatePolicy.setHumanTakeover(base, 12, true)
        assertTrue(takeover.humanTakeover)
        assertEquals(13L, takeover.viewportRevision)
        assertEquals(RemoteControlSessionLifecycle.HUMAN_TAKEOVER, takeover.lifecycle)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionStatePolicy.setHumanTakeover(takeover, 12, false)
        }
        val handedBack = RemoteControlSessionStatePolicy.setHumanTakeover(takeover, 13, false)
        assertFalse(handedBack.humanTakeover)
        assertEquals(14L, handedBack.viewportRevision)
        assertEquals(RemoteControlSessionLifecycle.NEGOTIATING, handedBack.lifecycle)
    }

    @Test
    fun authenticatedSignalDrainSettlesReconnectAndRemoteCloseStaysOutcomeUnknown() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
            processGeneration = 12,
            viewportRevision = 8,
            provider = "fabushi-webrtc",
            routePolicy = "direct-preferred",
            selectedRoute = "relay",
            transportUpdatedAt = 100,
            lifecycle = RemoteControlSessionLifecycle.RECONNECTING,
            reconcileRequired = true,
        )
        val ready = RemoteControlSessionStatePolicy.reconcileAfterDrain(
            base,
            sawReady = true,
            sawClose = false,
        )
        assertEquals(RemoteControlSessionLifecycle.READY, ready.lifecycle)
        assertFalse(ready.reconcileRequired)
        assertEquals(8L, ready.viewportRevision)

        val remoteClose = RemoteControlSessionStatePolicy.reconcileAfterDrain(
            ready,
            sawReady = false,
            sawClose = true,
        )
        assertEquals(RemoteControlSessionLifecycle.OUTCOME_UNKNOWN, remoteClose.lifecycle)
        assertTrue(remoteClose.reconcileRequired)
        assertEquals(9L, remoteClose.viewportRevision)
    }

    @Test
    fun transportFailureEntersReconnectWithoutReplayingHumanInputLease() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            requestId = "android-remote-0123456789abcdef",
            iceServersJson = """[{"urls":["stun:stun.example.com"]}]""",
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
            processGeneration = 12,
            viewportRevision = 8,
            provider = "fabushi-webrtc",
            routePolicy = "direct-preferred",
            selectedRoute = "direct",
            transportUpdatedAt = 100,
            humanTakeover = true,
            lifecycle = RemoteControlSessionLifecycle.HUMAN_TAKEOVER,
        )
        val reconnecting = RemoteControlSessionStatePolicy.markReconnectRequired(
            base,
            processGeneration = 12,
            expectedViewportRevision = 8,
        )
        assertEquals(RemoteControlSessionLifecycle.RECONNECTING, reconnecting.lifecycle)
        assertEquals(1, reconnecting.reconnectCount)
        assertTrue(reconnecting.reconcileRequired)
        assertFalse(reconnecting.humanTakeover)
        assertEquals(8L, reconnecting.viewportRevision)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionStatePolicy.markReconnectRequired(
                reconnecting,
                processGeneration = 13,
                expectedViewportRevision = 8,
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionStatePolicy.markReconnectRequired(
                reconnecting,
                processGeneration = 12,
                expectedViewportRevision = 7,
            )
        }
    }

    @Test
    fun closeFailureRemainsDurableOutcomeUnknownInsteadOfDeletingTruth() {
        val base = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
            processGeneration = 3,
            viewportRevision = 2,
            lifecycle = RemoteControlSessionLifecycle.READY,
        )
        val closing = RemoteControlSessionStatePolicy.beginClosing(base)
        assertEquals(RemoteControlSessionLifecycle.CLOSING, closing.lifecycle)
        assertTrue(closing.reconcileRequired)
        assertEquals(3L, closing.viewportRevision)

        val unknown = RemoteControlSessionStatePolicy.markCloseOutcomeUnknown(closing)
        assertEquals(RemoteControlSessionLifecycle.OUTCOME_UNKNOWN, unknown.lifecycle)
        assertTrue(unknown.reconcileRequired)
        assertFalse(unknown.humanTakeover)
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionStatePolicy.advanceViewport(unknown, unknown.viewportRevision)
        }
    }

    @Test
    fun rejectsInvalidEpochExpiryAndControlCharacters() {
        val credential = RemoteControlSessionCredential(
            deviceId = "computer-1",
            clientId = "remote-client-1",
            sessionId = "remote-session-1",
            mobileToken = validToken,
            accountFence = "session:account-1",
            accountEpoch = 7,
            expiresAt = 2_000_000_000,
        )

        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("accountEpoch", 0).toString(),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("expiresAt", 0).toString(),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteControlSessionCredential.parse(
                JSONObject(credential.toSecretJson()).put("sessionId", "bad\nidentity").toString(),
            )
        }
    }
}
