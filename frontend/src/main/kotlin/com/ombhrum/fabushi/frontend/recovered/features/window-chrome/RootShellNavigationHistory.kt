package com.ombhrum.fabushi

import android.content.Context
import android.content.SharedPreferences
import org.json.JSONArray
import org.json.JSONObject

internal enum class RootShellNavigationDirection { BACK, FORWARD }

internal data class RootShellNavigationScope(
    val accountEpoch: Long,
    val accountIdentity: String,
)

internal data class RootShellNavigationState(
    val entries: List<String> = emptyList(),
    val cursor: Int = -1,
    val pendingTarget: String? = null,
    val appliedAgentId: String? = null,
)

internal interface RootShellNavigationPersistence {
    fun load(scope: RootShellNavigationScope): RootShellNavigationState?
    fun save(scope: RootShellNavigationScope, state: RootShellNavigationState)
    fun clear()
}

internal class SharedPreferencesRootShellNavigationPersistence(
    private val preferences: SharedPreferences,
) : RootShellNavigationPersistence {
    override fun load(scope: RootShellNavigationScope): RootShellNavigationState? {
        if (preferences.getLong(KEY_ACCOUNT_EPOCH, -1L) != scope.accountEpoch) return null
        if (preferences.getString(KEY_ACCOUNT_IDENTITY, null) != scope.accountIdentity) return null
        val raw = preferences.getString(KEY_STATE, null) ?: return null
        return runCatching {
            val objectValue = JSONObject(raw)
            val array = objectValue.optJSONArray("entries") ?: JSONArray()
            val entries = buildList {
                for (index in 0 until array.length()) {
                    array.optString(index).trim().takeIf(String::isNotEmpty)?.let(::add)
                }
            }
            val rawCursor = objectValue.optInt("cursor", -1)
            RootShellNavigationState(
                entries = entries,
                cursor = rawCursor.coerceIn(-1, entries.lastIndex),
                pendingTarget = (objectValue.opt("pendingTarget") as? String)
                    ?.trim()
                    ?.takeIf(String::isNotEmpty),
                appliedAgentId = (objectValue.opt("appliedAgentId") as? String)
                    ?.trim()
                    ?.takeIf(String::isNotEmpty),
            )
        }.getOrNull()
    }

    override fun save(scope: RootShellNavigationScope, state: RootShellNavigationState) {
        val payload = JSONObject()
            .put("entries", JSONArray(state.entries))
            .put("cursor", state.cursor)
            .put("pendingTarget", state.pendingTarget ?: JSONObject.NULL)
            .put("appliedAgentId", state.appliedAgentId ?: JSONObject.NULL)
        preferences.edit()
            .putLong(KEY_ACCOUNT_EPOCH, scope.accountEpoch)
            .putString(KEY_ACCOUNT_IDENTITY, scope.accountIdentity)
            .putString(KEY_STATE, payload.toString())
            .apply()
    }

    override fun clear() {
        preferences.edit()
            .remove(KEY_ACCOUNT_EPOCH)
            .remove(KEY_ACCOUNT_IDENTITY)
            .remove(KEY_STATE)
            .apply()
    }

    private companion object {
        const val KEY_ACCOUNT_EPOCH = "accountEpoch"
        const val KEY_ACCOUNT_IDENTITY = "accountIdentity"
        const val KEY_STATE = "state"
    }
}

internal fun createRootShellNavigationOwner(context: Context): RootShellNavigationOwner =
    RootShellNavigationOwner(
        SharedPreferencesRootShellNavigationPersistence(
            context.applicationContext.getSharedPreferences(
                "fabushi.root-shell-navigation",
                Context.MODE_PRIVATE,
            ),
        ),
    )

/**
 * Presentation-only owner matching Desktop root-shell-state Agent history semantics.
 *
 * The Agent roster and active Agent remain canonical in MobileBotViewModel/Host. This owner keeps
 * only navigation history. Account epoch/identity fence restored history, while accountGeneration
 * rejects stale same-process projections without making process generation part of durable scope.
 */
internal class RootShellNavigationOwner(
    private val persistence: RootShellNavigationPersistence,
) {
    private var scope: RootShellNavigationScope? = null
    private var accountGeneration: Long = -1L
    private var state = RootShellNavigationState()

    @Synchronized
    fun observeAccount(
        loggedIn: Boolean,
        nextAccountGeneration: Long,
        nextAccountEpoch: Long,
        nextAccountIdentity: String?,
    ) {
        val identity = nextAccountIdentity?.trim().orEmpty()
        if (!loggedIn || nextAccountEpoch <= 0L || identity.isEmpty()) {
            scope = null
            accountGeneration = nextAccountGeneration
            state = RootShellNavigationState()
            persistence.clear()
            return
        }

        val current = scope
        if (current != null) {
            if (nextAccountEpoch < current.accountEpoch) return
            if (
                nextAccountEpoch == current.accountEpoch &&
                identity == current.accountIdentity &&
                nextAccountGeneration < accountGeneration
            ) {
                return
            }
        }

        val nextScope = RootShellNavigationScope(nextAccountEpoch, identity)
        if (nextScope != current) {
            state = persistence.load(nextScope) ?: RootShellNavigationState()
            scope = nextScope
        }
        accountGeneration = nextAccountGeneration
        persist()
    }

    @Synchronized
    fun observeAppliedAgent(agentId: String?) {
        val activeScope = scope ?: return
        val normalized = agentId?.trim()?.takeIf(String::isNotEmpty) ?: return
        state = when {
            normalized == state.appliedAgentId -> state
            normalized == state.pendingTarget -> state.copy(
                pendingTarget = null,
                appliedAgentId = normalized,
            )
            else -> {
                val prefix = if (state.cursor >= 0) {
                    state.entries.take(state.cursor + 1)
                } else {
                    emptyList()
                }
                val entries = prefix + normalized
                RootShellNavigationState(
                    entries = entries,
                    cursor = entries.lastIndex,
                    pendingTarget = null,
                    appliedAgentId = normalized,
                )
            }
        }
        persistence.save(activeScope, state)
    }

    @Synchronized
    fun navigate(
        direction: RootShellNavigationDirection,
        availableAgentIds: Set<String>,
    ): String? {
        val activeScope = scope ?: return null
        val step = if (direction == RootShellNavigationDirection.BACK) -1 else 1
        val current = state.entries.getOrNull(state.cursor)
        var index = state.cursor + step
        while (index in state.entries.indices) {
            val candidate = state.entries[index]
            if (candidate in availableAgentIds && candidate != current) {
                state = state.copy(cursor = index, pendingTarget = candidate)
                persistence.save(activeScope, state)
                return candidate
            }
            index += step
        }
        return null
    }

    @Synchronized
    fun snapshotForTest(): RootShellNavigationState = state

    private fun persist() {
        scope?.let { persistence.save(it, state) }
    }
}
