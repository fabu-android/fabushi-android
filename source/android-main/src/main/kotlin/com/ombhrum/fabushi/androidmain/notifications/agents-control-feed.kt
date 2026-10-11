package com.ombhrum.fabushi.androidmain.notifications

import org.json.JSONArray
import org.json.JSONObject

internal class AndroidAgentsControlFeed(
    private val osNotifications: AndroidOsNotificationManager,
    private val dockBadge: AndroidDockBadgeManager,
) {
    fun handleFeatureEvent(payload: JSONObject) {
        val kind = payload.optString("kind")
        val event = payload.optJSONObject("event") ?: payload

        when (kind.ifBlank { event.optString("kind") }) {
            "agents" -> {
                val agents = parseAgents(event.optJSONArray("agents")) ?: return
                osNotifications.handleAgents(agents.map(::notificationAgent))
                dockBadge.handleAgents(agents.map(::badgeAgent))
            }
            "agent-upserted" -> {
                val agent = event.optJSONObject("agent")?.let(::parseAgent) ?: return
                osNotifications.handleAgentUpserted(notificationAgent(agent))
                dockBadge.handleAgentUpserted(badgeAgent(agent))
            }
            "agents-roster-seed" -> {
                val agents = parseAgents(event.optJSONArray("agents")) ?: return
                osNotifications.seedBaseline(agents.map(::notificationAgent))
                dockBadge.seedRoster(agents.map(::badgeAgent))
            }
        }
    }

    private data class AgentProjection(
        val id: String,
        val name: String,
        val isRunning: Boolean,
        val awaitingReason: String?,
        val notifyEnabled: Boolean,
        val hidden: Boolean,
        val lastMessageId: String?,
        val lastMessagePreview: String?,
        val hasUnread: Boolean,
        val unreadCount: Int?,
        val snapshotEpoch: String,
        val snapshotSeq: Long,
    )

    private fun parseAgents(array: JSONArray?): List<AgentProjection>? {
        array ?: return null
        return buildList {
            for (index in 0 until array.length()) {
                val value = array.optJSONObject(index) ?: continue
                parseAgent(value)?.let(::add)
            }
        }
    }

    private fun parseAgent(value: JSONObject): AgentProjection? {
        val id = value.optString("id").trim()
        if (id.isEmpty()) return null
        val awaitingReason = value.optJSONObject("awaitingUserResponse")
            ?.optString("reason")
            ?.takeIf(String::isNotBlank)
            ?: value.optString("awaitingReason").takeIf(String::isNotBlank)
        return AgentProjection(
            id = id,
            name = value.optString("name").trim().ifBlank { "Your agent" },
            isRunning = value.optBoolean("isRunning"),
            awaitingReason = awaitingReason,
            notifyEnabled = value.optBoolean("notifyOnUpdatesEnabled", true),
            hidden = value.optBoolean("isHiddenFromSidebar"),
            lastMessageId = value.optString("lastMessageId").takeIf(String::isNotBlank),
            lastMessagePreview = value.optString("lastMessagePreview").takeIf(String::isNotBlank),
            hasUnread = value.optBoolean("hasUnread"),
            unreadCount = if (value.has("unreadCount") && !value.isNull("unreadCount")) {
                value.optInt("unreadCount")
            } else {
                null
            },
            snapshotEpoch = value.optString("snapshotEpoch"),
            snapshotSeq = value.optLong("snapshotSeq"),
        )
    }

    private fun notificationAgent(agent: AgentProjection) = AndroidNotificationAgent(
        id = agent.id,
        name = agent.name,
        isRunning = agent.isRunning,
        awaitingReason = agent.awaitingReason,
        notifyEnabled = agent.notifyEnabled,
        isHiddenFromSidebar = agent.hidden,
        lastMessageId = agent.lastMessageId,
        lastMessagePreview = agent.lastMessagePreview,
    )

    private fun badgeAgent(agent: AgentProjection) = DockBadgeRosterAgent(
        id = agent.id,
        hasUnread = agent.hasUnread,
        isHiddenFromSidebar = agent.hidden,
        unreadCount = agent.unreadCount,
        snapshotEpoch = agent.snapshotEpoch,
        snapshotSeq = agent.snapshotSeq,
    )
}
