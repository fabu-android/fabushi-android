package com.ombhrum.fabushi.androidmain.coordinator

import android.content.Context
import com.ombhrum.fabushi.androidpreload.runtime.AccountRebuildState
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
    val processGeneration: Long = 0L,
    val kind: ComputerRebuildKind? = null,
    val operationId: String? = null,
    val requestId: String? = null,
    val migrationOffsetKey: String = "",
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
                processGeneration = value.optLong("processGeneration", 0L),
                kind = value.optString("kind").takeIf(String::isNotBlank)?.let { ComputerRebuildKind.valueOf(it) },
                operationId = value.optString("operationId").takeIf(String::isNotBlank),
                requestId = value.optString("requestId").takeIf(String::isNotBlank),
                migrationOffsetKey = value.optString("migrationOffsetKey"),
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
            .put("processGeneration", snapshot.processGeneration)
            .put("kind", snapshot.kind?.name ?: "")
            .put("operationId", snapshot.operationId ?: "")
            .put("requestId", snapshot.requestId ?: "")
            .put("migrationOffsetKey", snapshot.migrationOffsetKey)
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
    private val processGeneration: Long = 1L,
) {
    private var snapshot: ComputerRebuildSnapshot? = store.read()?.let { persisted ->
        val rebound = persisted.copy(processGeneration = processGeneration)
        if (persisted.kind != null || persisted.pending || persisted.outcomeUnknown) {
            rebound.copy(outcomeUnknown = true)
        } else {
            rebound
        }
    }

    init {
        require(processGeneration > 0L) { "process generation must be positive" }
        snapshot?.let(store::write)
    }

    @Synchronized
    fun observeAccount(accountEpoch: Long) {
        require(accountEpoch >= 0L)
        val current = snapshot
        if (current == null || current.accountEpoch != accountEpoch) {
            snapshot = ComputerRebuildSnapshot(
                accountEpoch = accountEpoch,
                processGeneration = processGeneration,
            )
            store.write(snapshot)
        }
    }

    @Synchronized
    fun reserveRequest(accountEpoch: Long, requestId: String): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val normalized = requestId.trim()
        require(normalized.isNotEmpty() && normalized.length <= 240 && normalized.none(Char::isISOControl)) {
            "computer rebuild request identity is invalid"
        }
        val current = requireSnapshot(accountEpoch)
        if (current.requestId != null) {
            require(current.requestId == normalized) { "computer rebuild request already active" }
            return current
        }
        require(current.kind == null && !current.pending && !current.outcomeUnknown) {
            "computer rebuild episode is not settled"
        }
        return persist(
            current.copy(
                requestId = normalized,
                migrationOffsetKey = "",
                pending = true,
                acknowledged = false,
                outcomeUnknown = false,
                lastResolution = null,
            ),
        )
    }

    @Synchronized
    fun acceptRequest(
        accountEpoch: Long,
        requestId: String,
        operationId: String,
        kind: ComputerRebuildKind,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        require(kind == ComputerRebuildKind.UPDATE || kind == ComputerRebuildKind.RESET) {
            "backend request may start only update/reset rebuild"
        }
        val current = requireSnapshot(accountEpoch)
        require(current.requestId == requestId) { "stale computer rebuild request identity" }
        val normalizedOperation = operationId.trim()
        require(normalizedOperation.isNotEmpty()) { "computer rebuild backend omitted operation identity" }
        begin(accountEpoch, kind, normalizedOperation, ComputerRebuildSource.REQUEST)
        acknowledge(accountEpoch, normalizedOperation)
        return persist(requireSnapshot(accountEpoch).copy(pending = false, outcomeUnknown = false))
    }

    @Synchronized
    fun markRequestOutcomeUnknown(accountEpoch: Long, requestId: String): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        require(current.requestId == requestId) { "stale computer rebuild request identity" }
        return persist(current.copy(pending = false, outcomeUnknown = true))
    }

    @Synchronized
    fun rejectRequest(accountEpoch: Long, requestId: String): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        require(current.requestId == requestId) { "stale computer rebuild request identity" }
        return clear(current, ComputerRebuildResolution.FAILED)
    }

    @Synchronized
    fun recordMigrationOffset(
        accountEpoch: Long,
        operationId: String?,
        offsetKey: String,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        require(offsetKey.length <= 4096 && offsetKey.none(Char::isISOControl)) {
            "computer rebuild migration offset is invalid"
        }
        val normalizedOperation = operationId?.trim()?.takeIf(String::isNotEmpty)
        if (current.operationId != null && current.operationId != normalizedOperation) return current
        return persist(current.copy(migrationOffsetKey = offsetKey))
    }

    @Synchronized
    fun begin(
        accountEpoch: Long,
        kind: ComputerRebuildKind,
        operationId: String?,
        source: ComputerRebuildSource?,
    ): ComputerRebuildSnapshot {
        requireCurrentAccount(accountEpoch)
        val current = snapshot ?: ComputerRebuildSnapshot(
            accountEpoch = accountEpoch,
            processGeneration = processGeneration,
        )
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
                operationId = when {
                    kind == ComputerRebuildKind.RESET || kind == ComputerRebuildKind.RECOVER -> normalizedOperation
                    kind == ComputerRebuildKind.UPDATE && source != ComputerRebuildSource.AUTO -> normalizedOperation
                    else -> null
                },
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
                operationId = when {
                    kind == ComputerRebuildKind.RESET || kind == ComputerRebuildKind.RECOVER -> normalizedOperation
                    kind == ComputerRebuildKind.UPDATE && source != ComputerRebuildSource.AUTO -> normalizedOperation
                    else -> null
                },
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
        if (normalized != current.operationId && (normalized != null || current.operationId != null)) return current
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
        var current = requireSnapshot(accountEpoch)
        val normalized = operationId?.trim()?.takeIf(String::isNotEmpty)
        if (current.operationId != null && current.operationId != normalized) return current
        if (current.outcomeUnknown && normalized != null) {
            current = persist(current.copy(outcomeUnknown = false))
        }
        if (phase == ComputerRebuildMigrationPhase.FAILED) {
            if (current.kind == null) return current
            if (current.operationId != null && current.operationId != normalized) return current
            return clear(current, ComputerRebuildResolution.FAILED)
        }
        if (phase == ComputerRebuildMigrationPhase.DONE) {
            val eligible =
                current.kind == ComputerRebuildKind.RESET ||
                    current.kind == ComputerRebuildKind.RECOVER ||
                    (current.kind == ComputerRebuildKind.UPDATE && current.source == ComputerRebuildSource.MIGRATION)
            if (!eligible || current.terminalMigration) return current
            if (current.operationId != null && current.operationId != normalized) return current
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

    /**
     * Account/access projection reads rebuild truth only from this durable owner.
     * Remote binding readiness is deliberately not an input.
     */
    @Synchronized
    fun accountProjection(accountEpoch: Long): AccountRebuildState {
        requireCurrentAccount(accountEpoch)
        val current = requireSnapshot(accountEpoch)
        return when {
            current.outcomeUnknown -> AccountRebuildState.OUTCOME_UNKNOWN
            current.kind != null || current.pending -> AccountRebuildState.RECONNECTING
            else -> AccountRebuildState.IDLE
        }
    }

    private fun clear(
        current: ComputerRebuildSnapshot,
        resolution: ComputerRebuildResolution,
    ): ComputerRebuildSnapshot = persist(
        current.copy(
            kind = null,
            operationId = null,
            requestId = null,
            migrationOffsetKey = "",
            source = null,
            lockBoxId = null,
            pending = false,
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
        snapshot ?: ComputerRebuildSnapshot(
            accountEpoch = accountEpoch,
            processGeneration = processGeneration,
        ).also {
            snapshot = it
            store.write(it)
        }

    private fun persist(next: ComputerRebuildSnapshot): ComputerRebuildSnapshot {
        snapshot = next
        store.write(next)
        return next
    }
}


/**
 * Desktop Forever Box -> Android rebuild adapter. Invalid or unrelated payloads are ignored rather
 * than manufacturing a rebuild transition. A pull takes precedence over the coarse box state.
 */
internal fun projectForeverBoxRebuildEvent(value: JSONObject): Pair<String, String>? {
    val payload = value.optJSONObject("payload") ?: value
    val boxId = payload.optString("agentId").trim().takeIf(String::isNotEmpty) ?: return null
    val state = payload.optString("state").trim().takeIf(String::isNotEmpty) ?: return null
    val phase = when {
        payload.has("pull") && !payload.isNull("pull") -> "pulling"
        state == "running" && payload.optString("vncUrl").isNotBlank() -> "running"
        state == "running" -> "local"
        state == "hibernated" -> "sleeping"
        else -> "off"
    }
    return boxId to phase
}


internal data class ComputerRebuildMigrationIngress(
    val operationId: String?,
    val phase: ComputerRebuildMigrationPhase,
)

internal fun projectBoxMigrationRebuildEvent(value: JSONObject): ComputerRebuildMigrationIngress? {
    val payload = value.optJSONObject("payload") ?: value
    val phase = when (payload.optString("phase")) {
        "backing-up" -> ComputerRebuildMigrationPhase.BACKING_UP
        "creating" -> ComputerRebuildMigrationPhase.CREATING
        "moving" -> ComputerRebuildMigrationPhase.MOVING
        "cleaning-up" -> ComputerRebuildMigrationPhase.CLEANING_UP
        "wiping" -> ComputerRebuildMigrationPhase.WIPING
        "done" -> ComputerRebuildMigrationPhase.DONE
        "failed" -> ComputerRebuildMigrationPhase.FAILED
        else -> return null
    }
    val rawOperation = payload.opt("operationId")
    val operationId = when (rawOperation) {
        null, JSONObject.NULL -> null
        is JSONObject -> rawOperation.optString("value").trim().takeIf(String::isNotEmpty) ?: return null
        else -> return null
    }
    return ComputerRebuildMigrationIngress(operationId = operationId, phase = phase)
}

internal fun isDevBoxRebuildStartEvent(value: JSONObject): Boolean {
    val payload = value.optJSONObject("payload") ?: return false
    return payload.optString("type") == "start"
}
