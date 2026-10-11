package com.ombhrum.fabushi

import android.content.Context

enum class AndroidBackendUpdateSignal {
    REQUIRED,
    CLEAR,
    UNCHANGED,
}

data class AndroidMandatoryUpdateProducerState(
    val accountEpoch: Long,
    val minimumVersion: String?,
    val backendRequiresUpdate: Boolean,
)

data class AndroidMandatoryUpdateFacts(
    val minimumVersionObserved: Boolean,
    val minimumVersion: String?,
    val backendSignal: AndroidBackendUpdateSignal,
)

interface AndroidMandatoryUpdateProducerStateStore {
    fun read(): AndroidMandatoryUpdateProducerState?
    fun write(state: AndroidMandatoryUpdateProducerState)
}

class SharedPreferencesMandatoryUpdateStateStore(context: Context) : AndroidMandatoryUpdateProducerStateStore {
    private val preferences =
        context.getSharedPreferences("fabushi-mandatory-update-producer-v1", Context.MODE_PRIVATE)

    override fun read(): AndroidMandatoryUpdateProducerState? {
        if (!preferences.contains(KEY_ACCOUNT_EPOCH)) return null
        return AndroidMandatoryUpdateProducerState(
            accountEpoch = preferences.getLong(KEY_ACCOUNT_EPOCH, 0L),
            minimumVersion = preferences.getString(KEY_MINIMUM_VERSION, null),
            backendRequiresUpdate = preferences.getBoolean(KEY_BACKEND_REQUIRED, false),
        )
    }

    override fun write(state: AndroidMandatoryUpdateProducerState) {
        preferences.edit()
            .putLong(KEY_ACCOUNT_EPOCH, state.accountEpoch)
            .apply {
                if (state.minimumVersion == null) remove(KEY_MINIMUM_VERSION)
                else putString(KEY_MINIMUM_VERSION, state.minimumVersion)
            }
            .putBoolean(KEY_BACKEND_REQUIRED, state.backendRequiresUpdate)
            .apply()
    }

    private companion object {
        const val KEY_ACCOUNT_EPOCH = "accountEpoch"
        const val KEY_MINIMUM_VERSION = "minimumVersion"
        const val KEY_BACKEND_REQUIRED = "backendRequiresUpdate"
    }
}

/**
 * Coordinator-owned durable projection of the two Desktop mandatory-update producers.
 *
 * Network callbacks may finish after an account switch or after a newer refresh. Tokens are fenced
 * by both account epoch and monotonically increasing request sequence. A transport failure uses
 * UNCHANGED so a previously asserted backend requirement survives reconnect/process death until a
 * successful EnsureSandBox explicitly clears it.
 */
class AndroidMandatoryUpdateOwner(
    private val store: AndroidMandatoryUpdateProducerStateStore,
    private val processGeneration: Long,
) {
    data class Token(
        val accountEpoch: Long,
        val processGeneration: Long,
        val sequence: Long,
    )

    private var sequence = 0L
    private var state = store.read() ?: AndroidMandatoryUpdateProducerState(
        accountEpoch = 0L,
        minimumVersion = null,
        backendRequiresUpdate = false,
    )

    init {
        require(processGeneration > 0L) { "process generation must be positive" }
    }

    @Synchronized
    fun observeAccount(accountEpoch: Long): AndroidMandatoryUpdateProducerState {
        require(accountEpoch >= 0L) { "account epoch must be non-negative" }
        if (state.accountEpoch != accountEpoch) {
            state = AndroidMandatoryUpdateProducerState(
                accountEpoch = accountEpoch,
                minimumVersion = null,
                backendRequiresUpdate = false,
            )
            store.write(state)
        }
        return state
    }

    @Synchronized
    fun begin(accountEpoch: Long): Token {
        observeAccount(accountEpoch)
        sequence += 1
        return Token(
            accountEpoch = accountEpoch,
            processGeneration = processGeneration,
            sequence = sequence,
        )
    }

    @Synchronized
    fun settle(
        token: Token,
        facts: AndroidMandatoryUpdateFacts,
        currentAccountEpoch: Long,
    ): AndroidMandatoryUpdateProducerState {
        if (
            token.processGeneration != processGeneration ||
            token.sequence != sequence ||
            token.accountEpoch != currentAccountEpoch
        ) {
            return observeAccount(currentAccountEpoch)
        }
        val previous = observeAccount(currentAccountEpoch)
        state = previous.copy(
            minimumVersion = if (facts.minimumVersionObserved) {
                facts.minimumVersion?.trim()?.takeIf(String::isNotEmpty)
            } else {
                previous.minimumVersion
            },
            backendRequiresUpdate = when (facts.backendSignal) {
                AndroidBackendUpdateSignal.REQUIRED -> true
                AndroidBackendUpdateSignal.CLEAR -> false
                AndroidBackendUpdateSignal.UNCHANGED -> previous.backendRequiresUpdate
            },
        )
        store.write(state)
        return state
    }

    @Synchronized
    fun clearForLoggedOutAccount(accountEpoch: Long): AndroidMandatoryUpdateProducerState {
        sequence += 1
        state = AndroidMandatoryUpdateProducerState(
            accountEpoch = accountEpoch,
            minimumVersion = null,
            backendRequiresUpdate = false,
        )
        store.write(state)
        return state
    }

    @Synchronized
    fun snapshot(accountEpoch: Long): AndroidMandatoryUpdateProducerState = observeAccount(accountEpoch)
}
