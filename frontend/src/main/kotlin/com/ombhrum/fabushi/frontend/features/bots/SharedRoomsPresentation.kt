package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCrossUserSharingCapability
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCrossUserSharingState
import org.json.JSONArray
import org.json.JSONObject

internal data class MobileSharedRoomMember(
    val authId: String,
    val agentId: String?,
    val name: String,
)

internal data class MobileSharedRoom(
    val id: String,
    val name: String,
    val hostAuthId: String,
    val hostName: String?,
    val members: List<MobileSharedRoomMember>,
    val memberCount: Int = members.size,
)

internal data class MobileSharedJoinRequest(
    val requestId: String,
    val roomId: String,
    val roomName: String,
    val requesterAuthId: String,
    val requesterName: String,
    val createdAtMs: Long,
)

internal data class MobileSharedRoomsSnapshot(
    val available: Boolean = false,
    val environmentAllowed: Boolean = false,
    val enabled: Boolean = false,
    val canonicalRoomManagerBound: Boolean = false,
    val selfAuthId: String? = null,
    val rooms: List<MobileSharedRoom> = emptyList(),
    val pendingJoinRequests: List<MobileSharedJoinRequest> = emptyList(),
    val typingUserCount: Int = 0,
)

internal fun projectMobileSharedRooms(
    capability: AndroidCrossUserSharingCapability,
    state: AndroidCrossUserSharingState,
): MobileSharedRoomsSnapshot =
    MobileSharedRoomsSnapshot(
        available = capability.available && state.available,
        environmentAllowed = capability.environmentAllowed,
        enabled = capability.enabled && state.enabled,
        canonicalRoomManagerBound =
            capability.canonicalRoomManagerBound && state.canonicalRoomManagerBound,
        selfAuthId = state.selfAuthId?.trim()?.takeIf(String::isNotEmpty),
        rooms = parseMobileSharedRooms(state.rooms),
        pendingJoinRequests = parseMobileSharedJoinRequests(state.pendingJoinRequests),
        typingUserCount = state.typingUsers.length(),
    )

internal fun parseMobileSharedJoinRequests(rows: JSONArray): List<MobileSharedJoinRequest> =
    buildList {
        val seen = linkedSetOf<String>()
        for (index in 0 until rows.length()) {
            val row = rows.optJSONObject(index) ?: continue
            val requestId = row.optString("requestId").trim()
            val roomId = row.optString("roomId").trim()
            val requesterAuthId = row.optString("requesterAuthId").trim()
            if (
                requestId.isEmpty() ||
                roomId.isEmpty() ||
                requesterAuthId.isEmpty() ||
                !seen.add(requestId)
            ) continue
            add(
                MobileSharedJoinRequest(
                    requestId = requestId,
                    roomId = roomId,
                    roomName = row.optString("roomName").trim().ifBlank { "Shared room" },
                    requesterAuthId = requesterAuthId,
                    requesterName = row.optString("requesterName").trim().ifBlank { "Someone" },
                    createdAtMs = row.optLong("createdAtMs", 0L).coerceAtLeast(0L),
                ),
            )
        }
    }

internal fun parseMobileSharedRooms(rows: JSONArray): List<MobileSharedRoom> =
    buildList {
        val seen = linkedSetOf<String>()
        for (index in 0 until rows.length()) {
            val row = rows.optJSONObject(index) ?: continue
            val id = row.optString("id")
                .ifBlank { row.optString("roomId") }
                .trim()
            if (id.isEmpty() || !seen.add(id)) continue
            val members = row.optJSONArray("members")
            val projectedMembers = buildList {
                if (members != null) {
                    val seenMembers = linkedSetOf<String>()
                    for (memberIndex in 0 until members.length()) {
                        val member = members.optJSONObject(memberIndex) ?: continue
                        val authId = member.optString("authId").trim()
                        val agentId = member.optString("agentId").trim().takeIf(String::isNotEmpty)
                        if (authId.isEmpty()) continue
                        val memberKey = "$authId:${agentId.orEmpty()}"
                        if (!seenMembers.add(memberKey)) continue
                        add(
                            MobileSharedRoomMember(
                                authId = authId,
                                agentId = agentId,
                                name = member.optString("name").trim().ifBlank {
                                    if (agentId == null) "Member" else "Agent"
                                },
                            ),
                        )
                    }
                }
            }
            add(
                MobileSharedRoom(
                    id = id,
                    name = row.optString("name")
                        .ifBlank { row.optString("displayName") }
                        .ifBlank { "Shared room" },
                    hostAuthId = row.optString("hostAuthId").trim(),
                    hostName = row.optString("hostName").trim().takeIf(String::isNotEmpty),
                    members = projectedMembers,
                    memberCount = if (members == null) {
                        row.optInt("memberCount", 0).coerceAtLeast(0)
                    } else {
                        projectedMembers.size
                    },
                ),
            )
        }
    }

/**
 * Presentation-only shared-room façade.
 *
 * Canonical room state remains in Rust Host. This owner never caches or retries mutations; every
 * mutation is followed by a fresh typed Coordinator read so process death/account fencing stays
 * authoritative below Presentation.
 */
internal class SharedRoomsPresentationOwner(
    private val coordinator: AndroidCoordinatorPort,
) {
    fun refresh(): MobileSharedRoomsSnapshot =
        projectMobileSharedRooms(
            capability = coordinator.crossUserSharingCapability(),
            state = coordinator.crossUserSharingState(),
        )

    fun setEnabled(enabled: Boolean): MobileSharedRoomsSnapshot {
        coordinator.crossUserSharingSetEnabled(enabled)
        return refresh()
    }

    fun reconcile(): MobileSharedRoomsSnapshot {
        coordinator.crossUserSharingReconcile()
        return refresh()
    }

    fun createRoomFromAgent(
        agentId: String,
        agentName: String,
        avatarDataUrl: String? = null,
    ): JSONObject {
        val result = coordinator.crossUserSharingCreateFromAgent(agentId, agentName, avatarDataUrl)
        refresh()
        return result
    }

    fun invite(roomId: String): JSONObject {
        val result = coordinator.crossUserSharingCreateRoomInvite(roomId)
        refresh()
        return result
    }

    fun join(link: String): JSONObject {
        val result = coordinator.crossUserSharingJoin(link)
        refresh()
        return result
    }

    fun respond(requestId: String, isApproved: Boolean): MobileSharedRoomsSnapshot {
        val normalizedRequestId = requestId.trim()
        require(normalizedRequestId.isNotEmpty()) { "Shared-room join request id is required" }
        coordinator.crossUserSharingRespond(
            JSONObject()
                .put("requestId", normalizedRequestId)
                .put("isApproved", isApproved),
        )
        return refresh()
    }

    fun addAgent(roomId: String, agent: MobileBotSummaryAndroid): MobileSharedRoomsSnapshot {
        val normalizedRoomId = roomId.trim()
        require(normalizedRoomId.isNotEmpty()) { "Shared-room id is required" }
        require(agent.id.isNotBlank() && agent.name.isNotBlank()) { "Agent identity is required" }
        coordinator.crossUserSharingAddAgent(
            JSONObject()
                .put("roomId", normalizedRoomId)
                .put("agentId", agent.id)
                .put("agentName", agent.name),
        )
        return refresh()
    }

    fun removeAgent(roomId: String, agentId: String): MobileSharedRoomsSnapshot {
        coordinator.crossUserSharingRemoveAgent(roomId, agentId)
        return refresh()
    }

    fun leave(roomId: String, targetAuthId: String? = null): MobileSharedRoomsSnapshot {
        coordinator.crossUserSharingLeave(roomId, targetAuthId)
        return refresh()
    }

    fun setTyping(roomId: String, isTyping: Boolean): Boolean =
        coordinator.crossUserSharingSetTyping(roomId, isTyping)
}
