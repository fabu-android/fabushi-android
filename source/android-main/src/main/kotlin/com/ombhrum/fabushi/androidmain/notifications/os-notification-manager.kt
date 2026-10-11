package com.ombhrum.fabushi.androidmain.notifications

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build

internal data class AndroidNotificationAgent(
    val id: String,
    val name: String,
    val isRunning: Boolean,
    val awaitingReason: String? = null,
    val notifyEnabled: Boolean,
    val isHiddenFromSidebar: Boolean = false,
    val lastMessageId: String? = null,
    val lastMessagePreview: String? = null,
)

private data class NotificationSnapshot(
    val id: String,
    val name: String,
    val isRunning: Boolean,
    val awaitingReason: String?,
    val notifyEnabled: Boolean,
    val isHiddenFromSidebar: Boolean,
    val lastMessageId: String?,
    val lastMessagePreview: String?,
)

private enum class TransitionKind { NEEDS_INPUT, DONE }

private data class NotificationTransition(
    val agentId: String,
    val agentName: String,
    val kind: TransitionKind,
    val reason: String?,
    val notifyEnabled: Boolean,
    val isHiddenFromSidebar: Boolean,
    val lastMessageId: String?,
    val lastMessagePreview: String?,
)

internal class AndroidOsNotificationManager(
    private val context: Context,
    private val isAppForeground: () -> Boolean,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {
    private val notificationManager =
        context.getSystemService(NotificationManager::class.java)
    private val previous = linkedMapOf<String, NotificationSnapshot>()
    private val lastNotifiedAtMs = linkedMapOf<Pair<String, TransitionKind>, Long>()
    private val accountedMessageId = linkedMapOf<String, String?>()
    private val activeIds = linkedSetOf<Int>()
    private var hasSeededBaseline = false
    private val preSeedDeltas = ArrayDeque<AndroidNotificationAgent>()

    init {
        ensureChannels()
    }

    fun handleAgents(agents: List<AndroidNotificationAgent>) {
        val snapshots = agents.map { snapshotOf(it) }
        val transitions = diff(previous, snapshots)
        previous.clear()
        snapshots.associateByTo(previous) { it.id }
        snapshots.forEach { snapshot ->
            accountedMessageId.putIfAbsent(snapshot.id, snapshot.lastMessageId)
        }
        flushPreSeedDeltas()
        transitions.forEach(::showIfAllowed)
    }

    fun handleAgentUpserted(agent: AndroidNotificationAgent) {
        if (!hasSeededBaseline) {
            preSeedDeltas.addLast(agent)
            return
        }
        processDelta(agent)
    }

    fun seedBaseline(agents: List<AndroidNotificationAgent>) {
        agents.map { snapshotOf(it) }.forEach { snapshot ->
            previous.putIfAbsent(snapshot.id, snapshot)
            accountedMessageId.putIfAbsent(snapshot.id, snapshot.lastMessageId)
        }
        flushPreSeedDeltas()
    }

    fun forget(agentId: String) {
        previous.remove(agentId)
        accountedMessageId.remove(agentId)
        lastNotifiedAtMs.keys.removeAll { it.first == agentId }
    }

    fun reset() {
        previous.clear()
        accountedMessageId.clear()
        lastNotifiedAtMs.clear()
        hasSeededBaseline = false
        preSeedDeltas.clear()
        activeIds.forEach(notificationManager::cancel)
        activeIds.clear()
    }

    private fun processDelta(agent: AndroidNotificationAgent) {
        val snapshot = snapshotOf(agent)
        val before = previous[snapshot.id]
        val transitions = if (before == null) emptyList() else diff(
            mapOf(snapshot.id to before),
            listOf(snapshot),
        )
        transitions.forEach(::showIfAllowed)
        previous[snapshot.id] = snapshot
        accountedMessageId.putIfAbsent(snapshot.id, snapshot.lastMessageId)
    }

    private fun flushPreSeedDeltas() {
        if (hasSeededBaseline) return
        hasSeededBaseline = true
        while (preSeedDeltas.isNotEmpty()) {
            processDelta(preSeedDeltas.removeFirst())
        }
    }

    private fun showIfAllowed(transition: NotificationTransition) {
        if (transition.kind == TransitionKind.DONE) {
            val accounted = accountedMessageId[transition.agentId]
            if (transition.lastMessageId == null || transition.lastMessageId == accounted) return
        }
        accountedMessageId[transition.agentId] = transition.lastMessageId

        val key = transition.agentId to transition.kind
        val last = lastNotifiedAtMs[key]
        val now = nowMs()
        if (
            transition.isHiddenFromSidebar ||
            !transition.notifyEnabled ||
            isAppForeground() ||
            (last != null && now - last < THROTTLE_MS) ||
            !canPostNotifications()
        ) {
            return
        }
        lastNotifiedAtMs[key] = now
        show(transition)
    }

    private fun show(transition: NotificationTransition) {
        val needsInput = transition.kind == TransitionKind.NEEDS_INPUT
        val title = if (needsInput) {
            "${transition.agentName.ifBlank { "Your agent" }} needs you"
        } else {
            transition.agentName.ifBlank { "Your agent" }
        }
        val body = boundBody(
            if (needsInput) {
                transition.reason?.trim().orEmpty().ifBlank { "Waiting for your input." }
            } else {
                transition.lastMessagePreview?.trim().orEmpty()
                    .ifBlank { "Open Fabushi to see what it did." }
            },
        )
        val id = notificationId(transition.agentId, transition.kind)
        val builder = Notification.Builder(
            context,
            if (needsInput) CHANNEL_ATTENTION else CHANNEL_UPDATES,
        )
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(Notification.BigTextStyle().bigText(body))
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(agentPendingIntent(transition.agentId, id))

        notificationManager.notify(id, builder.build())
        activeIds += id
    }

    internal fun setBadgeCount(count: Int) {
        if (!canPostNotifications()) return
        if (count <= 0) {
            notificationManager.cancel(BADGE_NOTIFICATION_ID)
            activeIds.remove(BADGE_NOTIFICATION_ID)
            return
        }
        val notification = Notification.Builder(context, CHANNEL_BADGE)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Fabushi")
            .setContentText("$count unread update${if (count == 1) "" else "s"}")
            .setNumber(count)
            .setOnlyAlertOnce(true)
            .setOngoing(false)
            .build()
        notificationManager.notify(BADGE_NOTIFICATION_ID, notification)
        activeIds += BADGE_NOTIFICATION_ID
    }

    private fun agentPendingIntent(agentId: String, requestCode: Int): PendingIntent? {
        if (!AGENT_ID.matches(agentId)) return null
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?: return null
        intent.data = Uri.parse("fabushi://agent/$agentId")
        intent.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        return PendingIntent.getActivity(
            context,
            requestCode,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun canPostNotifications(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED

    private fun ensureChannels() {
        notificationManager.createNotificationChannels(
            listOf(
                NotificationChannel(
                    CHANNEL_ATTENTION,
                    "Agent attention",
                    NotificationManager.IMPORTANCE_HIGH,
                ).apply { setShowBadge(true) },
                NotificationChannel(
                    CHANNEL_UPDATES,
                    "Agent updates",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ).apply {
                    setShowBadge(true)
                    setSound(null, null)
                    enableVibration(false)
                },
                NotificationChannel(
                    CHANNEL_BADGE,
                    "Unread count",
                    NotificationManager.IMPORTANCE_LOW,
                ).apply { setShowBadge(true); setSound(null, null) },
            ),
        )
    }

    private fun snapshotOf(agent: AndroidNotificationAgent) = NotificationSnapshot(
        id = agent.id,
        name = agent.name,
        isRunning = agent.isRunning,
        awaitingReason = agent.awaitingReason,
        notifyEnabled = agent.notifyEnabled,
        isHiddenFromSidebar = agent.isHiddenFromSidebar,
        lastMessageId = agent.lastMessageId,
        lastMessagePreview = agent.lastMessagePreview,
    )

    companion object {
        private const val THROTTLE_MS = 5_000L
        private const val MAX_BODY_LENGTH = 140
        private const val CHANNEL_ATTENTION = "fabushi-agent-attention"
        private const val CHANNEL_UPDATES = "fabushi-agent-updates"
        private const val CHANNEL_BADGE = "fabushi-unread-badge"
        private const val BADGE_NOTIFICATION_ID = 0x0FAB
        private val AGENT_ID = Regex("^[A-Za-z0-9._:-]{1,200}$")

        private fun notificationId(agentId: String, kind: TransitionKind): Int =
            31 * agentId.hashCode() + kind.ordinal + 1

        internal fun boundBody(text: String): String {
            val collapsed = text.trim().split(Regex("\\s+")).filter(String::isNotEmpty).joinToString(" ")
            if (collapsed.length <= MAX_BODY_LENGTH) return collapsed
            return collapsed.take(MAX_BODY_LENGTH - 1).trimEnd() + "…"
        }

        private fun diff(
            previous: Map<String, NotificationSnapshot>,
            next: List<NotificationSnapshot>,
        ): List<NotificationTransition> = buildList {
            next.forEach { agent ->
                val before = previous[agent.id] ?: return@forEach
                val becameAwaiting =
                    agent.awaitingReason != null && before.awaitingReason == null
                val finishedTurn =
                    before.isRunning && !agent.isRunning && agent.awaitingReason == null
                if (!becameAwaiting && !finishedTurn) return@forEach
                add(
                    NotificationTransition(
                        agentId = agent.id,
                        agentName = agent.name,
                        kind = if (becameAwaiting) TransitionKind.NEEDS_INPUT else TransitionKind.DONE,
                        reason = if (becameAwaiting) agent.awaitingReason else null,
                        notifyEnabled = agent.notifyEnabled,
                        isHiddenFromSidebar = agent.isHiddenFromSidebar,
                        lastMessageId = agent.lastMessageId,
                        lastMessagePreview = agent.lastMessagePreview,
                    ),
                )
            }
        }
    }
}

internal class AndroidNotificationRuntime(
    context: Context,
    isAppForeground: () -> Boolean,
) {
    val os = AndroidOsNotificationManager(context, isAppForeground)
    val badge = AndroidDockBadgeManager(os::setBadgeCount)
    val feed = AndroidAgentsControlFeed(os, badge)

    fun reset() {
        os.reset()
        badge.reset()
    }
}
