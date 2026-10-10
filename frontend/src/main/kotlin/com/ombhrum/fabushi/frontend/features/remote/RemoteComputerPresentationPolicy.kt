package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject

internal data class RemoteComputerDevice(
    val deviceId: String,
    val label: String,
    val provider: String,
    val platform: String,
    val appVersion: String,
    val capabilities: Set<String>,
    val activeSessionCount: Int,
    val lastSeenAt: Long,
    val online: Boolean,
)

internal data class RemoteComputerPairing(
    val deviceId: String,
    val clientId: String,
    val accountEpoch: Long,
)

internal data class RemoteComputerSessionProjection(
    val stored: Boolean,
    val deviceId: String = "",
    val clientId: String = "",
    val sessionId: String = "",
    val accountEpoch: Long = 0L,
    val expiresAt: Long = 0L,
    val lastAcknowledgedSignalId: Long = 0L,
    val highestDrainedSignalId: Long = 0L,
    val selectedRoute: String = "",
    val viewportRevision: Long = 0L,
    val humanTakeover: Boolean = false,
    val lifecycle: String = "",
    val reconnectCount: Int = 0,
    val reconcileRequired: Boolean = false,
)

internal data class RemoteComputerNativeState(
    val computers: List<RemoteComputerDevice>,
    val pairing: RemoteComputerPairing?,
    val session: RemoteComputerSessionProjection = RemoteComputerSessionProjection(stored = false),
) {
    companion object {
        val Empty = RemoteComputerNativeState(emptyList(), null)
    }
}

/**
 * Presentation-only parser for Remote Computer public projections.
 *
 * Secrets are intentionally rejected rather than ignored so a future backend/runtime regression
 * cannot silently surface a control or executor credential to Compose or the hosted WebView.
 */
internal object RemoteComputerPresentationPolicy {
    private val forbiddenSecretKeys = setOf(
        "clientToken",
        "mobileToken",
        "deviceSecret",
        "bearerCredential",
        "remote_control_token",
        "remoteControlToken",
        "credential",
    )

    fun parse(
        listProjection: JSONObject,
        pairingProjection: JSONObject,
        sessionProjection: JSONObject = JSONObject().put("stored", false),
    ): RemoteComputerNativeState {
        rejectSecretsDeep(listProjection, "computer list")
        rejectSecretsDeep(pairingProjection, "pairing status")
        rejectSecretsDeep(sessionProjection, "control session")
        val rawComputers = listProjection.optJSONArray("computers") ?: JSONArray()
        require(rawComputers.length() <= 64) { "Remote computer list exceeds server contract" }
        val computers = buildList {
            repeat(rawComputers.length()) { index ->
                val item = rawComputers.optJSONObject(index)
                    ?: throw IllegalArgumentException("Remote computer item is invalid")
                rejectSecretsDeep(item, "computer item")
                add(parseComputer(item))
            }
        }
        val pairing = if (pairingProjection.optBoolean("paired", false)) {
            RemoteComputerPairing(
                deviceId = requiredIdentity(pairingProjection, "deviceId", 160),
                clientId = requiredIdentity(pairingProjection, "clientId", 160),
                accountEpoch = pairingProjection.getLong("accountEpoch").also {
                    require(it > 0L) { "Remote pairing epoch must be positive" }
                },
            )
        } else {
            null
        }
        val session = parseSession(sessionProjection)
        if (session.stored && pairing != null) {
            require(session.deviceId == pairing.deviceId && session.clientId == pairing.clientId) {
                "Remote control session is not owned by the current pairing"
            }
            require(session.accountEpoch == pairing.accountEpoch) {
                "Remote control session account epoch is stale"
            }
        }
        return RemoteComputerNativeState(computers, pairing, session)
    }

    fun normalizePairingCode(raw: String): String? {
        val normalized = raw.trim().uppercase()
        return normalized.takeIf {
            it.length == 12 && it.all { char -> char.isDigit() || char in 'A'..'F' }
        }
    }

    fun normalizePairingLabel(raw: String): String? {
        val normalized = raw.trim()
        return normalized.takeIf {
            it.isNotEmpty() && it.length <= 80 && it.none(Char::isISOControl)
        }
    }

    private fun parseSession(json: JSONObject): RemoteComputerSessionProjection {
        if (!json.optBoolean("stored", false)) {
            return RemoteComputerSessionProjection(stored = false)
        }
        val lifecycle = json.optString("lifecycle").trim()
        require(lifecycle in setOf(
            "pending",
            "negotiating",
            "ready",
            "human_takeover",
            "reconnecting",
            "closing",
            "outcome_unknown",
        )) { "Remote control session lifecycle is invalid" }
        val selectedRoute = optionalText(json, "selectedRoute", 20)
        require(selectedRoute.isEmpty() || selectedRoute in setOf("direct", "relay")) {
            "Remote control selected route is invalid"
        }
        return RemoteComputerSessionProjection(
            stored = true,
            deviceId = requiredIdentity(json, "deviceId", 160),
            clientId = requiredIdentity(json, "clientId", 160),
            sessionId = requiredIdentity(json, "sessionId", 160),
            accountEpoch = json.getLong("accountEpoch").also {
                require(it > 0L) { "Remote control session account epoch must be positive" }
            },
            expiresAt = json.getLong("expiresAt").also {
                require(it > 0L) { "Remote control session expiry must be positive" }
            },
            lastAcknowledgedSignalId = json.optLong("lastAcknowledgedSignalId", 0L).also {
                require(it >= 0L) { "Remote control acknowledged cursor is invalid" }
            },
            highestDrainedSignalId = json.optLong("highestDrainedSignalId", 0L).also {
                require(it >= 0L) { "Remote control drained cursor is invalid" }
            },
            selectedRoute = selectedRoute,
            viewportRevision = json.optLong("viewportRevision", 0L).also {
                require(it >= 0L) { "Remote control viewport revision is invalid" }
            },
            humanTakeover = json.optBoolean("humanTakeover", false),
            lifecycle = lifecycle,
            reconnectCount = json.optInt("reconnectCount", 0).also {
                require(it >= 0) { "Remote control reconnect count is invalid" }
            },
            reconcileRequired = json.optBoolean("reconcileRequired", false),
        ).also {
            require(it.highestDrainedSignalId >= it.lastAcknowledgedSignalId) {
                "Remote control signal cursors are inconsistent"
            }
        }
    }

    private fun parseComputer(json: JSONObject): RemoteComputerDevice {
        val rawCapabilities = json.optJSONArray("capabilities") ?: JSONArray()
        require(rawCapabilities.length() <= 64) { "Remote computer capabilities are unbounded" }
        val capabilities = buildSet {
            repeat(rawCapabilities.length()) { index ->
                val capability = rawCapabilities.optString(index).trim()
                require(capability.isNotEmpty() && capability.length <= 80) {
                    "Remote computer capability is invalid"
                }
                add(capability)
            }
        }
        val activeSessions = json.optInt("activeSessionCount", 0)
        require(activeSessions >= 0) { "Remote active session count is invalid" }
        return RemoteComputerDevice(
            deviceId = requiredIdentity(json, "deviceId", 160),
            label = optionalText(json, "label", 200),
            provider = optionalText(json, "provider", 80),
            platform = optionalText(json, "platform", 80),
            appVersion = optionalText(json, "appVersion", 80),
            capabilities = capabilities,
            activeSessionCount = activeSessions,
            lastSeenAt = json.optLong("lastSeenAt", 0L).also {
                require(it >= 0L) { "Remote computer lastSeenAt is invalid" }
            },
            online = json.optBoolean("online", false),
        )
    }

    private fun requiredIdentity(json: JSONObject, key: String, max: Int): String =
        json.getString(key).trim().also {
            require(it.isNotEmpty() && it.length <= max && it.none(Char::isISOControl)) {
                "Remote computer " + key + " is invalid"
            }
        }

    private fun optionalText(json: JSONObject, key: String, max: Int): String {
        if (!json.has(key) || json.isNull(key)) return ""
        return json.getString(key).trim().also {
            require(it.length <= max && it.none(Char::isISOControl)) {
                "Remote computer " + key + " is invalid"
            }
        }
    }

    private fun rejectSecretsDeep(json: JSONObject, source: String) {
        val leaked = forbiddenSecretKeys.firstOrNull(json::has)
        require(leaked == null) { source + " leaked protected credential field" }
        json.keys().forEach { key ->
            when (val value = json.opt(key)) {
                is JSONObject -> rejectSecretsDeep(value, "$source.$key")
                is JSONArray -> repeat(value.length()) { index ->
                    (value.opt(index) as? JSONObject)?.let {
                        rejectSecretsDeep(it, "$source.$key[$index]")
                    }
                }
            }
        }
    }
}
