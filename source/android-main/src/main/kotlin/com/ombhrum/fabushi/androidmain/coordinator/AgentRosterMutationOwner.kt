package com.ombhrum.fabushi.androidmain.coordinator

import android.app.Application
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

internal data class PendingAgentRosterMutation(
    val operationId: String,
    val accountFence: String,
    val mutation: JSONObject,
)

/**
 * Process-death-safe owner for presentation-originated Agent roster mutations.
 *
 * A pending operation is synchronously persisted before Host dispatch. It is removed only after the
 * Host reports a durable completed/rejected settlement. If Android dies after the Host committed but
 * before the reply reaches Kotlin, the same operation id is replayed and the Host roster journal
 * returns the already committed result instead of repeating the side effect.
 */
internal interface AgentRosterMutationStore {
    fun read(): String?
    fun write(value: String): Boolean
}

internal class SharedPreferencesAgentRosterMutationStore(application: Application) :
    AgentRosterMutationStore {
    private val preferences =
        application.getSharedPreferences("fabushi-agent-roster-mutations", 0)

    override fun read(): String? = preferences.getString("pending", null)

    override fun write(value: String): Boolean =
        preferences.edit().putString("pending", value).commit()
}

internal class AgentRosterMutationOwner(
    private val store: AgentRosterMutationStore,
) {
    private val lock = Any()

    fun begin(accountFence: String, mutation: JSONObject): PendingAgentRosterMutation {
        require(accountFence.isNotBlank()) { "Agent roster mutation requires an account fence" }
        val operation = PendingAgentRosterMutation(
            operationId = "agent-roster:${UUID.randomUUID()}",
            accountFence = accountFence,
            mutation = JSONObject(mutation.toString()),
        )
        synchronized(lock) {
            val rows = readLocked().toMutableList()
            rows += operation
            persistLocked(rows.takeLast(MAX_PENDING_OPERATIONS))
        }
        return operation
    }

    fun pendingFor(accountFence: String): List<PendingAgentRosterMutation> =
        synchronized(lock) {
            readLocked()
                .filter { it.accountFence == accountFence }
                .map { it.copy(mutation = JSONObject(it.mutation.toString())) }
        }

    fun settle(operationId: String) {
        synchronized(lock) {
            val rows = readLocked()
            val remaining = rows.filterNot { it.operationId == operationId }
            if (remaining.size != rows.size) persistLocked(remaining)
        }
    }

    private fun readLocked(): List<PendingAgentRosterMutation> {
        val raw = store.read().orEmpty()
        if (raw.isBlank()) return emptyList()
        val array = runCatching { JSONArray(raw) }.getOrElse { return emptyList() }
        val seen = linkedSetOf<String>()
        return buildList {
            for (index in 0 until array.length()) {
                val row = array.optJSONObject(index) ?: continue
                val operationId = row.optString("operationId").trim()
                val accountFence = row.optString("accountFence").trim()
                val mutation = row.optJSONObject("mutation") ?: continue
                if (operationId.isBlank() || accountFence.isBlank() || !seen.add(operationId)) continue
                add(
                    PendingAgentRosterMutation(
                        operationId = operationId,
                        accountFence = accountFence,
                        mutation = JSONObject(mutation.toString()),
                    ),
                )
            }
        }
    }

    private fun persistLocked(rows: List<PendingAgentRosterMutation>) {
        val array = JSONArray()
        rows.forEach { row ->
            array.put(
                JSONObject()
                    .put("operationId", row.operationId)
                    .put("accountFence", row.accountFence)
                    .put("mutation", JSONObject(row.mutation.toString())),
            )
        }
        check(store.write(array.toString())) {
            "Failed to persist Agent roster mutation journal"
        }
    }

    private companion object {
        const val MAX_PENDING_OPERATIONS = 64
    }
}
