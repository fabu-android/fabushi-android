package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidContactGroup
import com.ombhrum.fabushi.androidpreload.runtime.AndroidContactGroupsSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ContactGroupProjectionTest {
    private val snapshot = AndroidContactGroupsSnapshot(
        accountFence = "account-device:test",
        sessionFence = "session:test",
        revision = 3,
        groups = listOf(
            AndroidContactGroup("__pinned__", "Pinned", listOf("human:a"), false),
            AndroidContactGroup("ops", "Ops", listOf("human:b"), false),
            AndroidContactGroup("qa", "QA", listOf("human:c"), true),
            AndroidContactGroup("__unassigned__", "Unassigned", listOf("human:d"), false),
        ),
    )
    private val contacts = listOf(
        MessagingContact("human:a", "Alice", null, "human"),
        MessagingContact("human:b", "Bob", null, "human"),
        MessagingContact("human:c", "Carol", null, "human"),
        MessagingContact("human:d", "Dan", null, "human"),
    )

    @Test
    fun editableProjectionExcludesReservedSyntheticGroups() {
        assertEquals(listOf("ops", "qa"), editableContactGroups(snapshot).map { it.id })
    }

    @Test
    fun assignmentIsExclusiveAcrossNamedGroups() {
        val next = assignContactToGroup(editableContactGroups(snapshot), "human:b", "qa")
        assertTrue(next.first { it.id == "ops" }.peerIds.isEmpty())
        assertEquals(listOf("human:c", "human:b"), next.first { it.id == "qa" }.peerIds)
    }

    @Test
    fun collapseTogglePreservesMembershipAndTargetsOnlyOneGroup() {
        val base = editableContactGroups(snapshot)
        val next = toggleContactGroupCollapsed(base, "ops")
        assertTrue(next.first { it.id == "ops" }.isCollapsed)
        assertTrue(next.first { it.id == "qa" }.isCollapsed)
        assertEquals(base.first { it.id == "ops" }.peerIds, next.first { it.id == "ops" }.peerIds)
    }

    @Test
    fun searchExpandsMatchesAndOmitsEmptyNamedGroups() {
        val next = searchedContactGroups(snapshot.groups, contacts, "car")
        assertEquals(listOf("qa"), next.map { it.id })
        assertEquals(listOf("human:c"), next.single().peerIds)
        assertFalse(next.single().isCollapsed)
    }
}
