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

internal data class RemoteComputerNativeState(
    val computers: List<RemoteComputerDevice>,
    val pairing: RemoteComputerPairing?,
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
    ): RemoteComputerNativeState {
        rejectSecrets(listProjection, "computer list")
        rejectSecrets(pairingProjection, "pairing status")
        val rawComputers = listProjection.optJSONArray("computers") ?: JSONArray()
        require(rawComputers.length() <= 64) { "Remote computer list exceeds server contract" }
        val computers = buildList {
            repeat(rawComputers.length()) { index ->
                val item = rawComputers.optJSONObject(index)
                    ?: throw IllegalArgumentException("Remote computer item is invalid")
                rejectSecrets(item, "computer item")
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
        return RemoteComputerNativeState(computers, pairing)
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

    private fun rejectSecrets(json: JSONObject, source: String) {
        val leaked = forbiddenSecretKeys.firstOrNull(json::has)
        require(leaked == null) { source + " leaked protected credential field" }
    }
}
