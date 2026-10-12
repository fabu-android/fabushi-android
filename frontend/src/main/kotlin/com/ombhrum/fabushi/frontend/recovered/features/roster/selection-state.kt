package com.ombhrum.fabushi

import android.content.SharedPreferences
import org.json.JSONObject
import java.nio.charset.StandardCharsets

internal data class RosterSelectionState(
    val currentAgentId: String? = null,
    val isLoadPending: Boolean = false,
)

private const val ROSTER_SELECTION_SCHEMA_VERSION = 1
private const val ROSTER_SELECTION_SLICE = "selection.last-agent"

internal interface RosterSelectionPersistence {
    fun read(accountSlot: String): StoredRosterSelection
    fun write(accountSlot: String, state: RosterSelectionState)
    fun clear(accountSlot: String)
}

internal sealed interface StoredRosterSelection {
    data object Absent : StoredRosterSelection
    data object Corrupt : StoredRosterSelection
    data class Envelope(
        val schemaVersion: Int,
        val agentId: String?,
    ) : StoredRosterSelection
}

internal class SharedPreferencesRosterSelectionPersistence(
    private val preferences: SharedPreferences,
) : RosterSelectionPersistence {
    override fun read(accountSlot: String): StoredRosterSelection {
        val raw = preferences.getString(rosterSelectionPersistenceKey(accountSlot), null)
            ?: return StoredRosterSelection.Absent
        return runCatching {
            val envelope = JSONObject(raw)
            if (!envelope.has("schemaVersion") || !envelope.has("value")) {
                return@runCatching StoredRosterSelection.Corrupt
            }
            val schemaVersion = envelope.getInt("schemaVersion")
            val value = envelope.optJSONObject("value")
                ?: return@runCatching StoredRosterSelection.Corrupt
            StoredRosterSelection.Envelope(
                schemaVersion = schemaVersion,
                agentId = value.optString("agentId").trim().takeIf(String::isNotEmpty),
            )
        }.getOrDefault(StoredRosterSelection.Corrupt)
    }

    override fun write(accountSlot: String, state: RosterSelectionState) {
        preferences.edit()
            .putString(
                rosterSelectionPersistenceKey(accountSlot),
                JSONObject()
                    .put("schemaVersion", ROSTER_SELECTION_SCHEMA_VERSION)
                    .put(
                        "value",
                        JSONObject().apply {
                            state.currentAgentId?.let { put("agentId", it) }
                        },
                    )
                    .toString(),
            )
            .apply()
    }

    override fun clear(accountSlot: String) {
        preferences.edit().remove(rosterSelectionPersistenceKey(accountSlot)).apply()
    }
}

internal class RosterSelectionStore(
    private val persistence: RosterSelectionPersistence,
) {
    private var state = RosterSelectionState()
    private var completeRosterAgentIds: List<String>? = null
    private var accountSlot: String? = null
    private var generation = 0L
    private var disposed = false

    @Synchronized
    fun get(): RosterSelectionState = state

    @Synchronized
    fun select(agentId: String?): Boolean {
        if (disposed) return false
        val next = agentId?.trim()?.takeIf(String::isNotEmpty)
        if (next == state.currentAgentId) {
            return next != null && !state.isLoadPending
        }
        state = RosterSelectionState(
            currentAgentId = next,
            isLoadPending = next != null,
        )
        persist()
        return next != null
    }

    @Synchronized
    fun settle(agentId: String) {
        if (
            disposed ||
            state.currentAgentId != agentId ||
            !state.isLoadPending
        ) {
            return
        }
        val complete = completeRosterAgentIds
        val next = if (complete == null || agentId in complete) {
            agentId
        } else {
            complete.firstOrNull()
        }
        state = RosterSelectionState(currentAgentId = next, isLoadPending = false)
        if (next != agentId) persist()
    }

    @Synchronized
    fun reconcile(agentIds: List<String>, isRosterComplete: Boolean) {
        if (disposed || !isRosterComplete) return
        completeRosterAgentIds = agentIds.toList()
        val current = state.currentAgentId
        if (current != null && current !in agentIds && state.isLoadPending) return
        if (current != null && current in agentIds) return
        state = RosterSelectionState(
            currentAgentId = agentIds.firstOrNull(),
            isLoadPending = false,
        )
        persist()
    }

    @Synchronized
    fun restore(nextAccountSlot: String?) {
        generation += 1
        val expectedGeneration = generation
        accountSlot = nextAccountSlot
        completeRosterAgentIds = null
        state = RosterSelectionState()
        if (disposed || nextAccountSlot == null) return

        val stored = persistence.read(nextAccountSlot)
        if (
            disposed ||
            expectedGeneration != generation ||
            accountSlot != nextAccountSlot
        ) {
            return
        }
        when (stored) {
            StoredRosterSelection.Absent -> Unit
            StoredRosterSelection.Corrupt -> persistence.clear(nextAccountSlot)
            is StoredRosterSelection.Envelope -> {
                if (
                    stored.schemaVersion != ROSTER_SELECTION_SCHEMA_VERSION ||
                    stored.agentId.isNullOrBlank()
                ) {
                    persistence.clear(nextAccountSlot)
                } else {
                    state = RosterSelectionState(
                        currentAgentId = stored.agentId,
                        isLoadPending = false,
                    )
                }
            }
        }
    }

    @Synchronized
    fun reset() {
        generation += 1
        accountSlot = null
        completeRosterAgentIds = null
        state = RosterSelectionState()
    }

    @Synchronized
    fun dispose() {
        if (disposed) return
        disposed = true
        generation += 1
        accountSlot = null
        completeRosterAgentIds = null
        state = RosterSelectionState()
    }

    private fun persist() {
        accountSlot?.let { persistence.write(it, state) }
    }
}

internal fun rosterSelectionPersistenceKey(accountSlot: String): String {
    require(accountSlot.isNotEmpty()) { "accountSlot must not be empty" }
    val encoded = buildString {
        for (byte in accountSlot.toByteArray(StandardCharsets.UTF_8)) {
            val unsigned = byte.toInt() and 0xff
            val char = unsigned.toChar()
            val keep =
                char in 'A'..'Z' ||
                    char in 'a'..'z' ||
                    char in '0'..'9' ||
                    char == '-' ||
                    char == '_' ||
                    char == '!' ||
                    char == '~' ||
                    char == '*' ||
                    char == '\'' ||
                    char == '(' ||
                    char == ')'
            if (keep) {
                append(char)
            } else {
                append('%')
                append("0123456789ABCDEF"[unsigned ushr 4])
                append("0123456789ABCDEF"[unsigned and 0x0f])
            }
        }
    }
    return "sand.client.slice.account.$encoded.$ROSTER_SELECTION_SLICE"
}
