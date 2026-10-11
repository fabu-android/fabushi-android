package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteComputerPresentationPolicyTest {
    @Test
    fun parsesOnlyPublicComputerAndPairingProjection() {
        val state = RemoteComputerPresentationPolicy.parse(
            JSONObject()
                .put(
                    "computers",
                    JSONArray().put(
                        JSONObject()
                            .put("deviceId", "computer-1")
                            .put("label", "Work Mac")
                            .put("provider", "fabushi")
                            .put("platform", "macos")
                            .put("appVersion", "1.2.3")
                            .put("capabilities", JSONArray().put("display").put("input"))
                            .put("activeSessionCount", 1)
                            .put("lastSeenAt", 1234)
                            .put("online", true),
                    ),
                ),
            JSONObject()
                .put("paired", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("accountEpoch", 7),
        )

        assertEquals(1, state.computers.size)
        assertEquals("Work Mac", state.computers.single().label)
        assertTrue(state.computers.single().online)
        assertEquals(setOf("display", "input"), state.computers.single().capabilities)
        assertEquals("android-client-1", state.pairing?.clientId)
        assertEquals(7L, state.pairing?.accountEpoch)
    }

    @Test
    fun unpairedProjectionDoesNotInventClientIdentity() {
        val state = RemoteComputerPresentationPolicy.parse(
            JSONObject().put("computers", JSONArray()),
            JSONObject().put("paired", false),
        )
        assertTrue(state.computers.isEmpty())
        assertNull(state.pairing)
    }

    @Test
    fun rejectsAnySecretCredentialBeforePresentation() {
        for (key in listOf(
            "clientToken",
            "mobileToken",
            "deviceSecret",
            "bearerCredential",
            "remote_control_token",
            "credential",
        )) {
            val pairing = JSONObject()
                .put("paired", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("accountEpoch", 7)
                .put(key, "secret")
            assertThrows(IllegalArgumentException::class.java) {
                RemoteComputerPresentationPolicy.parse(
                    JSONObject().put("computers", JSONArray()),
                    pairing,
                )
            }
        }
    }

    @Test
    fun parsesPublicControlSessionAndFencesItToCurrentPairing() {
        val state = RemoteComputerPresentationPolicy.parse(
            JSONObject().put("computers", JSONArray()),
            JSONObject()
                .put("paired", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("accountEpoch", 7),
            JSONObject()
                .put("stored", true)
                .put("deviceId", "computer-1")
                .put("clientId", "android-client-1")
                .put("sessionId", "remote-session-1")
                .put("accountEpoch", 7)
                .put("expiresAt", 2_000_000_000)
                .put("lastAcknowledgedSignalId", 4)
                .put("highestDrainedSignalId", 4)
                .put("selectedRoute", "relay")
                .put("viewportRevision", 9)
                .put("humanTakeover", true)
                .put("lifecycle", "human_takeover")
                .put("reconnectCount", 2)
                .put("reconcileRequired", false),
        )

        assertTrue(state.session.stored)
        assertEquals("remote-session-1", state.session.sessionId)
        assertEquals(9L, state.session.viewportRevision)
        assertTrue(state.session.humanTakeover)
        assertEquals("human_takeover", state.session.lifecycle)
    }

    @Test
    fun rejectsNestedCredentialsAndStaleSessionIdentity() {
        val paired = JSONObject()
            .put("paired", true)
            .put("deviceId", "computer-1")
            .put("clientId", "android-client-1")
            .put("accountEpoch", 7)

        assertThrows(IllegalArgumentException::class.java) {
            RemoteComputerPresentationPolicy.parse(
                JSONObject()
                    .put(
                        "computers",
                        JSONArray().put(
                            JSONObject()
                                .put("deviceId", "computer-1")
                                .put("credentialEnvelope", JSONObject().put("credential", "secret")),
                        ),
                    ),
                paired,
            )
        }

        assertThrows(IllegalArgumentException::class.java) {
            RemoteComputerPresentationPolicy.parse(
                JSONObject().put("computers", JSONArray()),
                paired,
                JSONObject()
                    .put("stored", true)
                    .put("deviceId", "computer-other")
                    .put("clientId", "android-client-1")
                    .put("sessionId", "remote-session-1")
                    .put("accountEpoch", 7)
                    .put("expiresAt", 2_000_000_000)
                    .put("lifecycle", "pending"),
            )
        }
    }

    @Test
    fun pairingCodeAndLabelMatchCoordinatorContract() {
        assertEquals(
            "A1B2C3D4E5F6",
            RemoteComputerPresentationPolicy.normalizePairingCode(" a1b2c3d4e5f6 "),
        )
        assertNull(RemoteComputerPresentationPolicy.normalizePairingCode("A1B2-C3D4-E5F6"))
        assertNull(RemoteComputerPresentationPolicy.normalizePairingCode("G1B2C3D4E5F6"))
        assertEquals(
            "Fabushi Android",
            RemoteComputerPresentationPolicy.normalizePairingLabel(" Fabushi Android "),
        )
        assertNull(RemoteComputerPresentationPolicy.normalizePairingLabel(" "))
        assertFalse(RemoteComputerPresentationPolicy.normalizePairingLabel("bad\nlabel") != null)
    }

    @Test
    fun rebuildProgressMatchesDesktopUpdateMigrationAndPullSemantics() {
        val projection = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "update")
                    .put("boxPhase", "pulling")
                    .put("pullPercent", 50.0)
                    .put("migrationPhases", JSONArray().put("backing-up").put("creating"))
                    .put("connected", true)
                    .put("leftHealthy", true)
                    .put("terminalMigration", false),
            ),
        )

        assertEquals("update", projection.kind)
        assertEquals(2, projection.activeIndex)
        assertEquals(6, projection.steps.size)
        assertEquals((2.5 / 6.0), projection.progress, 0.0001)
        assertNull(projection.reconnectVariant)
    }

    @Test
    fun rebuildProgressProjectsResetRecoverAndReconnectVariants() {
        val reset = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "reset")
                    .put("boxPhase", "off")
                    .put("migrationPhases", JSONArray().put("wiping").put("creating")),
            ),
        )
        assertEquals(2, reset.activeIndex)
        assertEquals(6, reset.steps.size)

        val recover = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "recover")
                    .put("boxPhase", "running")
                    .put("leftHealthy", true)
                    .put("migrationPhases", JSONArray().put("creating").put("moving")),
            ),
        )
        assertEquals(recover.steps.lastIndex, recover.activeIndex)

        val network = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "reconnecting")
                    .put("connected", false),
            ),
        )
        assertEquals("network", network.reconnectVariant)

        val restarting = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "reconnecting")
                    .put("connected", true)
                    .put("boxPhase", "off"),
            ),
        )
        assertEquals("restarting", restarting.reconnectVariant)
    }

    @Test
    fun rebuildProgressFailsClosedForIdleAndInvalidPullPercent() {
        assertNull(ComputerRebuildProgressPolicy.project(JSONObject()))

        val projection = requireNotNull(
            ComputerRebuildProgressPolicy.project(
                JSONObject()
                    .put("kind", "update")
                    .put("boxPhase", "pulling")
                    .put("pullPercent", 999.0)
                    .put("migrationPhases", JSONArray()),
            ),
        )
        assertEquals(2.0 / 6.0, projection.progress, 0.0001)
    }

}
