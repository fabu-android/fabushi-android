package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidCrossUserSharingCapability
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCrossUserSharingState
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SharedRoomsPresentationTest {
    @Test
    fun projection_requires_capability_and_canonical_manager_and_dedupes_rooms() {
        val rooms = JSONArray()
            .put(
                JSONObject()
                    .put("id", "room-a")
                    .put("name", "Alpha")
                    .put("hostAuthId", "host-a")
                    .put("hostName", "A")
                    .put(
                        "members",
                        JSONArray()
                            .put(JSONObject().put("authId", "one").put("name", "One"))
                            .put(
                                JSONObject()
                                    .put("authId", "two")
                                    .put("agentId", "agent-two")
                                    .put("name", "Two"),
                            )
                            .put(
                                JSONObject()
                                    .put("authId", "two")
                                    .put("agentId", "agent-two")
                                    .put("name", "Duplicate"),
                            ),
                    ),
            )
            .put(JSONObject().put("roomId", "room-a").put("name", "duplicate"))
            .put(JSONObject().put("roomId", "room-b").put("displayName", "Beta").put("memberCount", 3))
        val capability = AndroidCrossUserSharingCapability(
            available = true,
            environmentAllowed = true,
            enabled = true,
            canonicalRoomManagerBound = true,
        )
        val state = AndroidCrossUserSharingState(
            available = true,
            enabled = true,
            selfAuthId = "self",
            pendingJoinRequests = JSONArray().put(
                JSONObject()
                    .put("requestId", "request-a")
                    .put("roomId", "room-a")
                    .put("roomName", "Alpha")
                    .put("requesterAuthId", "peer-a")
                    .put("requesterName", "Peer A")
                    .put("createdAtMs", 42L),
            ),
            rooms = rooms,
            typingUsers = JSONArray().put(JSONObject()).put(JSONObject()),
            canonicalRoomManagerBound = true,
        )

        val projected = projectMobileSharedRooms(capability, state)

        assertTrue(projected.available)
        assertTrue(projected.enabled)
        assertTrue(projected.canonicalRoomManagerBound)
        assertEquals(2, projected.rooms.size)
        assertEquals("Alpha", projected.rooms[0].name)
        assertEquals(2, projected.rooms[0].memberCount)
        assertEquals(listOf("one", "two"), projected.rooms[0].members.map { it.authId })
        assertEquals("host-a", projected.rooms[0].hostAuthId)
        assertEquals("Beta", projected.rooms[1].name)
        assertEquals(3, projected.rooms[1].memberCount)
        assertEquals(1, projected.pendingJoinRequests.size)
        assertEquals("request-a", projected.pendingJoinRequests.single().requestId)
        assertEquals("Peer A", projected.pendingJoinRequests.single().requesterName)
        assertEquals(2, projected.typingUserCount)
    }

    @Test
    fun projection_never_claims_enabled_or_bound_when_host_state_disagrees() {
        val capability = AndroidCrossUserSharingCapability(
            available = true,
            environmentAllowed = true,
            enabled = true,
            canonicalRoomManagerBound = true,
        )
        val state = AndroidCrossUserSharingState(
            available = true,
            enabled = false,
            selfAuthId = null,
            pendingJoinRequests = JSONArray(),
            rooms = JSONArray(),
            typingUsers = JSONArray(),
            canonicalRoomManagerBound = false,
        )

        val projected = projectMobileSharedRooms(capability, state)

        assertTrue(projected.available)
        assertFalse(projected.enabled)
        assertFalse(projected.canonicalRoomManagerBound)
    }

    @Test
    fun malformed_or_duplicate_join_requests_are_not_actionable() {
        val requests = JSONArray()
            .put("not-an-object")
            .put(JSONObject().put("requestId", "missing-room"))
            .put(
                JSONObject()
                    .put("requestId", " request-a ")
                    .put("roomId", " room-a ")
                    .put("roomName", "Alpha")
                    .put("requesterAuthId", " peer-a ")
                    .put("requesterName", "Peer A")
                    .put("createdAtMs", 7L),
            )
            .put(
                JSONObject()
                    .put("requestId", "request-a")
                    .put("roomId", "room-b")
                    .put("requesterAuthId", "peer-b"),
            )

        val parsed = parseMobileSharedJoinRequests(requests)

        assertEquals(1, parsed.size)
        assertEquals("request-a", parsed.single().requestId)
        assertEquals("room-a", parsed.single().roomId)
        assertEquals("peer-a", parsed.single().requesterAuthId)
        assertEquals("Peer A", parsed.single().requesterName)
        assertEquals(7L, parsed.single().createdAtMs)
    }

    @Test
    fun malformed_or_duplicate_room_rows_do_not_create_second_presentation_owner() {
        val rooms = JSONArray()
            .put("not-an-object")
            .put(JSONObject().put("name", "missing-id"))
            .put(JSONObject().put("roomId", " room-c ").put("name", "Gamma"))
            .put(JSONObject().put("id", "room-c").put("name", "duplicate"))

        val parsed = parseMobileSharedRooms(rooms)

        assertEquals(1, parsed.size)
        assertEquals("room-c", parsed.single().id)
        assertEquals("Gamma", parsed.single().name)
    }
}
