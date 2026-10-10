package com.ombhrum.fabushi.androidmain.coordinator

import android.content.Context
import org.json.JSONObject

internal enum class ComputerRebuildKind { UPDATE, RESET, RECOVER, RECONNECTING }
internal enum class ComputerRebuildSource { AUTO, REQUEST, MIGRATION }
internal enum class ComputerRebuildTeardown { NONE, TRANSPORT, BOX }
internal enum class ComputerRebuildResolution { SETTLED, FAILED, CANCELLED }
internal enum class ComputerRebuildMigrationPhase {
    BACKING_UP, CREATING, MOVING, CLEANING_UP, WIPING, DONE, FAILED
}

internal data class ComputerRebuildSnapshot(
    val accountEpoch: Long,
    val kind: ComputerRebuildKind? = null,
    val operationId: String? = null,
    val source: ComputerRebuildSource? = null,
    val lockBoxId: String? = null,
    val pending: Boolean = false,
    val acknowledged: Boolean = false,
    val observedBoxId: String? = null,
    val boxPhase: String? = null,
    val lastHealthyBoxId: String? = null,
    val leftHealthy: Boolean = false,
    val teardown: ComputerRebuildTeardown = ComputerRebuildTeardown.NONE,
    val reconnectedSinceLeft: Boolean = false,
    val connected: Boolean = true,
    val terminalMigration: Boolean = false,
    val outcomeUnknown: Boolean = false,
    val lastResolution: ComputerRebuildResolution? = null,
)

internal interface ComputerRebuildStateStore {
    fun read(): ComputerRebuildSnapshot?
    fun write(snapshot: ComputerRebuildSnapshot?)
}

internal class SharedPreferencesComputerRebuildStateStore(context: Context) : ComputerRebuildStateStore {
    private val preferences = context.applicationContext.getSharedPreferences("fabushi-computer-rebuild", 0)

    override fun read(): ComputerRebuildSnapshot? {
        val raw = preferences.getString("snapshot", null) ?: return null
        return runCatching {
            val value = JSONObject(raw)
            ComputerRebuildSnapshot(
                accountEpoch = value.getLong("accountEpoch"),
                kind = value.optString("kind").takeIf(String::isNotBlank)?.let { ComputerRebuildKind.valueOf(it) },
                operationId = value.optString("operationId").takeIf(String::isNotBlank),
                source = value.optString("source").takeIf(String::isNotBlank)?.let { ComputerRebuildSource.valueOf(it) },
                lockBoxId = value.optString("lockBoxId").takeIf(String::isNotBlank),
                pending = value.optBoolean("pending", false),
                acknowledged = value.optBoolean("acknowledged", false),
                observedBoxId = value.optString("observedBoxId").takeIf(String::isNotBlank),
                boxPhase = value.optString("boxPhase").takeIf(String::isNotBlank),
                lastHealthyBoxId = value.optString("lastHealthyBoxId").takeIf(String::isNotBlank),
                leftHealthy = value.optBoolean("leftHealthy", false),
                teardown = value.optString("teardown")
                    .takeIf(String::isNotBlank)
                    ?.let { ComputerRebuildTeardown.valueOf(it) }
                    ?: ComputerRebuildTeardown.NONE,
                reconnectedSinceLeft = value.optBoolean("reconnectedSinceLeft", false),
                connected = value.optBoolean("connected", true),
                terminalMigration = value.optBoolean("terminalMigration", false),
                outcomeUnknown = value.optBoolean("outcomeUnknown", false),
                lastResolution = value.optString("lastResolution")
                    .takeIf(String::isNotBlank)
                    ?.let { ComputerRebuildResolution.valueOf(it) },
            )
        }.getOrNull()
    }

    override fun write(snapshot: ComputerRebuildSnapshot?) {
        if (snapshot == null) {
            preferences.edit().remove("snapshot").apply()
            return
        }
        val value = JSONObject()
            .put("accountEpoch", snapshot.accountEpoch)
            .put("kind", snapshot.kind?.name ?: "")
            .put("operationId", snapshot.operationId ?: "")
            .put("source", snapshot.source?.name ?: "")
            .put("lockBoxId", snapshot.lockBoxId ?: "")
            .put("pending", snapshot.pending)
            .put("acknowledged", snapshot.acknowledged)
            .put("observedBoxId", snapshot.observedBoxId ?: "")
            .put("boxPhase", snapshot.boxPhase ?: "")
            .put("lastHealthyBoxId", snapshot.lastHealthyBoxId ?: "")
            .put("leftHealthy", snapshot.leftHealthy)
            .put("teardown", snapshot.teardown.name)
            .put("reconnectedSinceLeft", snapshot.reconnectedSinceLeft)
            .put("connected", snapshot.connected)
            .put("terminalMigration", snapshot.terminalMigration)
            .put("outcomeUnknown", snapshot.outcomeUnknown)
            .put("lastResolution", snapshot.lastResolution?.name ?: "")
        preferences.edit().putString("snapshot", value.toString()).apply()
    }
}

/**
 * Coordinator-owned durable Computer Rebuild state.
 *
 * This mirrors the Desktop reducer contract without inventing a backend endpoint. Callers may feed
 * only facts received from canonical request, transport, Forever Box, and migration owners.
 */
internal class ComputerRebuildStateOwner(
    private val store: ComputerRebuildStateStore,
) {
    private var snapshot: ComputerRebuildSnapshot? = store.read()?.let { persisted ->
        if (persisted.kind != null || persisted.pending) {
            persisted.copy(outcomeUnknown = true)
        } else {
            persisted
        }
    }

    init {
        snapshot?.let(store::write)
    }

    @Synchronized
    fun observeAccount(accountEpoch: Long) {
        require(accountEpoch >= 0L)
        val current = snapshot
        if (current == null || current.accountEpoch != accountEpoch) {
            snapshot = ComputerRebuildSnapshot(accountEpoch = accountEpoch)
            store.write(snapshot)
        }
    }

    @Synchronized
    fun begin(
        accountEpoch: Long,
        kind: ComputerRebuildKind,
        operationId: String?,
        source: ComputerRebuildSource?,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = snapshot ?: ComputerRebuildSnapshot(accountEpoch)
        val normalizedOperation = operationId?.trim()?.takeIf(String::isNotEmpty)
        if ((kind == ComputerRebuildKind.RESET || kind == ComputerRebuildKind.RECOVER) && normalizedOperation == null) {
            throw IllegalArgumentException("reset/recover rebuild requires operation identity")
        }
        val replacesActive =
            current.kind != null &&
                (kind == ComputerRebuildKind.RESET || kind == ComputerRebuildKind.RECOVER) &&
                (current.kind != ComputerRebuildKind.RESET &&
                    current.kind != ComputerRebuildKind.RECOVER ||
                    current.operationId != normalizedOperation)
        val next = if (replacesActive || current.kind == null) {
            current.copy(
                kind = kind,
                operationId = if (kind == ComputerRebuildKind.RESET || kind == ComputerRebuildKind.RECOVER) normalizedOperation else null,
                source = if (kind == ComputerRebuildKind.UPDATE) source else null,
                lockBoxId = current.observedBoxId,
                acknowledged = false,
                leftHealthy = kind == ComputerRebuildKind.RESET ||
                    kind == ComputerRebuildKind.RECOVER ||
                    kind == ComputerRebuildKind.RECONNECTING,
                teardown = ComputerRebuildTeardown.NONE,
                reconnectedSinceLeft = false,
                terminalMigration = false,
                outcomeUnknown = false,
                lastResolution = null,
            )
        } else if (current.kind == ComputerRebuildKind.RECONNECTING && kind != ComputerRebuildKind.RECONNECTING) {
            current.copy(
                kind = kind,
                operationId = normalizedOperation,
                source = if (kind == ComputerRebuildKind.UPDATE) source else null,
                lockBoxId = current.observedBoxId,
                outcomeUnknown = false,
            )
        } else if (
            current.kind == ComputerRebuildKind.UPDATE &&
            kind == ComputerRebuildKind.UPDATE &&
            current.source == ComputerRebuildSource.AUTO &&
            source != null &&
            source != ComputerRebuildSource.AUTO
        ) {
            current.copy(source = source)
        } else {
            current
        }
        return persist(next)
    }

    @Synchronized
    fun setPending(accountEpoch: Long, pending: Boolean): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        return persist(current.copy(pending = pending))
    }

    @Synchronized
    fun acknowledge(accountEpoch: Long, operationId: String?): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        val normalized = operationId?.trim()?.takeIf(String::isNotEmpty)
        if (normalized != null && current.operationId != null && normalized != current.operationId) return current
        if (current.kind == null) return current
        return persist(current.copy(acknowledged = true, outcomeUnknown = false))
    }

    @Synchronized
    fun observeConnection(accountEpoch: Long, connected: Boolean): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        val next = if (connected) {
            current.copy(
                connected = true,
                reconnectedSinceLeft = current.reconnectedSinceLeft || current.leftHealthy,
            )
        } else {
            current.copy(
                connected = false,
                leftHealthy = current.leftHealthy || current.kind != null,
                teardown = if (current.kind != null && current.teardown == ComputerRebuildTeardown.NONE) {
                    ComputerRebuildTeardown.TRANSPORT
                } else {
                    current.teardown
                },
            )
        }
        return persist(next)
    }

    @Synchronized
    fun observeBox(
        accountEpoch: Long,
        boxId: String?,
        phase: String?,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        val normalizedBox = boxId?.trim()?.takeIf(String::isNotEmpty)
        val healthy = phase == "running" || phase == "local"
        var next = current.copy(
            observedBoxId = normalizedBox,
            boxPhase = phase,
            lastHealthyBoxId = if (healthy) normalizedBox else current.lastHealthyBoxId,
        )
        if (
            next.kind == null &&
            !next.pending &&
            phase == "pulling" &&
            normalizedBox != null &&
            next.lastHealthyBoxId == normalizedBox
        ) {
            next = next.copy(
                kind = ComputerRebuildKind.UPDATE,
                source = ComputerRebuildSource.AUTO,
                lockBoxId = normalizedBox,
                outcomeUnknown = false,
                lastResolution = null,
            )
        }
        if (next.kind != null && next.lockBoxId == null && normalizedBox != null) {
            next = next.copy(lockBoxId = normalizedBox)
        }
        val sameLockedBox = next.lockBoxId == null || normalizedBox == null || next.lockBoxId == normalizedBox
        if (next.kind != null && sameLockedBox && !healthy) {
            next = next.copy(leftHealthy = true, teardown = ComputerRebuildTeardown.BOX)
        }
        return persist(next)
    }

    @Synchronized
    fun observeMigration(
        accountEpoch: Long,
        operationId: String?,
        phase: ComputerRebuildMigrationPhase,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        val normalized = operationId?.trim()?.takeIf(String::isNotEmpty)
        if (phase == ComputerRebuildMigrationPhase.FAILED) {
            return if (current.kind == null) current else clear(current, ComputerRebuildResolution.FAILED)
        }
        if (phase == ComputerRebuildMigrationPhase.DONE) {
            val eligible =
                current.kind == ComputerRebuildKind.RESET ||
                    current.kind == ComputerRebuildKind.RECOVER ||
                    (current.kind == ComputerRebuildKind.UPDATE && current.source == ComputerRebuildSource.MIGRATION)
            if (!eligible || current.terminalMigration) return current
            if (current.operationId != null && normalized != null && current.operationId != normalized) return current
            return persist(current.copy(terminalMigration = true, leftHealthy = true, outcomeUnknown = false))
        }
        if (phase == ComputerRebuildMigrationPhase.WIPING) {
            return begin(accountEpoch, ComputerRebuildKind.RESET, normalized, null)
        }
        return begin(accountEpoch, ComputerRebuildKind.UPDATE, normalized, ComputerRebuildSource.MIGRATION)
    }

    @Synchronized
    fun fail(accountEpoch: Long): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        return if (current.kind == null) current else clear(current, ComputerRebuildResolution.FAILED)
    }

    @Synchronized
    fun deactivate(accountEpoch: Long): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        if (current.kind == null) return current
        return clear(
            current,
            if (current.terminalMigration) ComputerRebuildResolution.SETTLED else ComputerRebuildResolution.CANCELLED,
        )
    }

    @Synchronized
    fun snapshot(accountEpoch: Long): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        return requireSnapshot(accountEpoch)
    }

    private fun clear(
        current: ComputerRebuildSnapshot,
        resolution: ComputerRebuildResolution,
    ): ComputerRebuildSnapshot = persist(
        current.copy(
            kind = null,
            operationId = null,
            source = null,
            lockBoxId = null,
            acknowledged = false,
            leftHealthy = false,
            teardown = ComputerRebuildTeardown.NONE,
            reconnectedSinceLeft = false,
            terminalMigration = false,
            outcomeUnknown = false,
            lastResolution = resolution,
        ),
    )

    private fun requireCurrentAccount(accountEpoch: Long) {
        val current = snapshot
        require(current == null || current.accountEpoch == accountEpoch) { "stale account epoch" }
    }

    private fun requireSnapshot(accountEpoch: Long): ComputerRebuildSnapshot =
        snapshot ?: ComputerRebuildSnapshot(accountEpoch = accountEpoch).also {
            snapshot = it
            store.write(it)
        }

    private fun persist(next: ComputerRebuildSnapshot): ComputerRebuildSnapshot {
        snapshot = next
        store.write(next)
        return next
    }
}
