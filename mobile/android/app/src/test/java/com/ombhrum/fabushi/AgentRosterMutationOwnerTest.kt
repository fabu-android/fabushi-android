package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AgentRosterMutationOwner
import com.ombhrum.fabushi.androidmain.coordinator.AgentRosterMutationStore
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentRosterMutationOwnerTest {
    private class MemoryStore(
        var value: String? = null,
    ) : AgentRosterMutationStore {
        override fun read(): String? = value
        override fun write(value: String): Boolean {
            this.value = value
            return true
        }
    }

    @Test
    fun pendingMutationSurvivesOwnerRecreationAndIsAccountFenced() {
        val store = MemoryStore()
        val first = AgentRosterMutationOwner(store)
        val pending = first.begin(
            "session:account-a",
            JSONObject()
                .put("kind", "duplicate")
                .put("id", "agent-1"),
        )

        val reopened = AgentRosterMutationOwner(store)
        val sameAccount = reopened.pendingFor("session:account-a")
        assertEquals(1, sameAccount.size)
        assertEquals(pending.operationId, sameAccount.single().operationId)
        assertEquals("duplicate", sameAccount.single().mutation.getString("kind"))
        assertTrue(reopened.pendingFor("session:account-b").isEmpty())

        reopened.settle(pending.operationId)
        assertTrue(AgentRosterMutationOwner(store).pendingFor("session:account-a").isEmpty())
    }

    @Test
    fun eachUserIntentGetsAStableDistinctOperationIdentity() {
        val store = MemoryStore()
        val owner = AgentRosterMutationOwner(store)
        val mutation = JSONObject().put("kind", "delete").put("id", "agent-1")
        val first = owner.begin("session:account-a", mutation)
        val second = owner.begin("session:account-a", mutation)

        assertTrue(first.operationId != second.operationId)
        assertEquals(
            listOf(first.operationId, second.operationId),
            owner.pendingFor("session:account-a").map { it.operationId },
        )
    }
}
