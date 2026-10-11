package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.notifications.AndroidDockBadgeManager
import com.ombhrum.fabushi.androidmain.notifications.AndroidOsNotificationManager
import com.ombhrum.fabushi.androidmain.notifications.DockBadgeAgent
import com.ombhrum.fabushi.androidmain.notifications.DockBadgeRosterAgent
import com.ombhrum.fabushi.androidmain.notifications.computeDockBadgeTotal
import com.ombhrum.fabushi.androidmain.notifications.isStaleDockBadgeRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidNotificationsTest {
    @Test
    fun dockBadgeCountsUnreadAndIgnoresHiddenAgents() {
        assertEquals(
            4,
            computeDockBadgeTotal(
                listOf(
                    DockBadgeAgent(hasUnread = true, unreadCount = 3),
                    DockBadgeAgent(hasUnread = true, unreadCount = 0),
                    DockBadgeAgent(hasUnread = true, isHiddenFromSidebar = true, unreadCount = 99),
                    DockBadgeAgent(hasUnread = false, unreadCount = 10),
                ),
            ),
        )
    }

    @Test
    fun staleRowsCannotRollBadgeStateBack() {
        val incoming = DockBadgeRosterAgent(
            id = "agent-a",
            hasUnread = false,
            snapshotEpoch = "epoch-a",
            snapshotSeq = 9,
        )
        assertTrue(isStaleDockBadgeRow("epoch-a", 10, incoming))
        assertFalse(isStaleDockBadgeRow("epoch-b", 10, incoming))
        assertFalse(isStaleDockBadgeRow("epoch-a", 9, incoming))
    }

    @Test
    fun badgeManagerDoesNotProjectDuplicateOrStaleTotals() {
        val totals = mutableListOf<Int>()
        val manager = AndroidDockBadgeManager(totals::add)
        manager.seedRoster(
            listOf(
                DockBadgeRosterAgent(
                    id = "agent-a",
                    hasUnread = true,
                    unreadCount = 2,
                    snapshotEpoch = "e",
                    snapshotSeq = 10,
                ),
            ),
        )
        manager.handleAgentUpserted(
            DockBadgeRosterAgent(
                id = "agent-a",
                hasUnread = false,
                unreadCount = 0,
                snapshotEpoch = "e",
                snapshotSeq = 9,
            ),
        )
        assertEquals(listOf(2), totals)
        manager.handleAgentUpserted(
            DockBadgeRosterAgent(
                id = "agent-a",
                hasUnread = false,
                snapshotEpoch = "e",
                snapshotSeq = 11,
            ),
        )
        assertEquals(listOf(2, 0), totals)
    }

    @Test
    fun notificationBodyCollapsesAndBoundsUntrustedText() {
        assertEquals(
            "hello world",
            AndroidOsNotificationManager.boundBody("  hello\n\tworld  "),
        )
        val bounded = AndroidOsNotificationManager.boundBody("x".repeat(200))
        assertEquals(140, bounded.length)
        assertTrue(bounded.endsWith("…"))
    }
}
