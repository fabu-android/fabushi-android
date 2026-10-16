package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class CanonicalMessagingContactProjectionTest {
    @Test
    fun latestReplacementWinsWithoutDuplicateParticipantRoots() {
        val rows = JSONArray()
            .put(JSONObject().put("id", "human:ada").put("displayName", "Ada (stale)").put("kind", "human").put("isHidden", true))
            .put(JSONObject().put("id", "human:grace").put("displayName", "Grace").put("kind", "human"))
            .put(JSONObject().put("id", "human:ada").put("displayName", "Ada").put("kind", "human").put("isHidden", false))

        val contacts = canonicalMessagingContacts(rows, currentActorId = "human:self")

        assertEquals(listOf("human:ada", "human:grace"), contacts.map { it.id })
        assertEquals("Ada", contacts.first().displayName)
        assertFalse(contacts.first().isHidden)
    }

    @Test
    fun currentActorIsNeverProjectedAsParallelContact() {
        val rows = JSONArray()
            .put(JSONObject().put("id", "human:self").put("displayName", "Me").put("kind", "human"))
            .put(JSONObject().put("id", "human:peer").put("displayName", "Peer").put("kind", "human"))

        assertEquals(
            listOf("human:peer"),
            canonicalMessagingContacts(rows, currentActorId = "human:self").map { it.id },
        )
    }
}
