package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal enum class RemoteControlSessionLifecycle {
    PENDING,
    NEGOTIATING,
    READY,
    HUMAN_TAKEOVER,
    RECONNECTING,
    CLOSING,
    OUTCOME_UNKNOWN,
}

internal data class RemoteControlSessionCredential(
    val deviceId: String,
    val clientId: String,
    val sessionId: String,
    val mobileToken: String,
    val requestId: String? = null,
    val iceServersJson: String = "[]",
    val accountFence: String,
    val accountEpoch: Long,
    val expiresAt: Long,
    val lastAcknowledgedSignalId: Long = 0L,
    val highestDrainedSignalId: Long = 0L,
    val provider: String? = null,
    val routePolicy: String? = null,
    val selectedRoute: String? = null,
    val relayRegion: String? = null,
    val transportUpdatedAt: Long = 0L,
    val processGeneration: Long = 0L,
    val viewportRevision: Long = 0L,
    val humanTakeover: Boolean = false,
    val lifecycle: RemoteControlSessionLifecycle = RemoteControlSessionLifecycle.PENDING,
    val reconnectCount: Int = 0,
    val reconcileRequired: Boolean = false,
) {
    fun toSecretJson(): String = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("sessionId", sessionId)
        .put("mobileToken", mobileToken)
        .put("requestId", requestId ?: JSONObject.NULL)
        .put("iceServersJson", iceServersJson)
        .put("accountFence", accountFence)
        .put("accountEpoch", accountEpoch)
        .put("expiresAt", expiresAt)
        .put("lastAcknowledgedSignalId", lastAcknowledgedSignalId)
        .put("highestDrainedSignalId", highestDrainedSignalId)
        .put("provider", provider ?: JSONObject.NULL)
        .put("routePolicy", routePolicy ?: JSONObject.NULL)
        .put("selectedRoute", selectedRoute ?: JSONObject.NULL)
        .put("relayRegion", relayRegion ?: JSONObject.NULL)
        .put("transportUpdatedAt", transportUpdatedAt)
        .put("processGeneration", processGeneration)
        .put("viewportRevision", viewportRevision)
        .put("humanTakeover", humanTakeover)
        .put("lifecycle", lifecycle.name)
        .put("reconnectCount", reconnectCount)
        .put("reconcileRequired", reconcileRequired)
        .toString()

    fun publicProjection(): JSONObject = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("sessionId", sessionId)
        .put("accountEpoch", accountEpoch)
        .put("expiresAt", expiresAt)
        .put("lastAcknowledgedSignalId", lastAcknowledgedSignalId)
        .put("highestDrainedSignalId", highestDrainedSignalId)
        .put("provider", provider ?: JSONObject.NULL)
        .put("routePolicy", routePolicy ?: JSONObject.NULL)
        .put("selectedRoute", selectedRoute ?: JSONObject.NULL)
        .put("relayRegion", relayRegion ?: JSONObject.NULL)
        .put("transportUpdatedAt", transportUpdatedAt)
        .put("processGeneration", processGeneration)
        .put("viewportRevision", viewportRevision)
        .put("humanTakeover", humanTakeover)
        .put("lifecycle", lifecycle.name.lowercase())
        .put("reconnectCount", reconnectCount)
        .put("reconcileRequired", reconcileRequired)

    companion object {
        fun parse(value: String): RemoteControlSessionCredential {
            val json = JSONObject(value)
            val legacyKeys = setOf(
                "deviceId",
                "clientId",
                "sessionId",
                "mobileToken",
                "accountFence",
                "accountEpoch",
                "expiresAt",
            )
            val cursorKeys = legacyKeys + setOf(
                "lastAcknowledgedSignalId",
                "highestDrainedSignalId",
            )
            val currentKeys = cursorKeys + setOf(
                "provider",
                "routePolicy",
                "selectedRoute",
                "relayRegion",
                "transportUpdatedAt",
            )
            val lifecycleKeys = currentKeys + setOf(
                "processGeneration",
                "viewportRevision",
                "humanTakeover",
                "lifecycle",
                "reconnectCount",
                "reconcileRequired",
            )
            val dataPlaneKeys = lifecycleKeys + setOf("requestId", "iceServersJson")
            val observedKeys = json.keys().asSequence().toSet()
            require(
                observedKeys == legacyKeys ||
                    observedKeys == cursorKeys ||
                    observedKeys == currentKeys ||
                    observedKeys == lifecycleKeys ||
                    observedKeys == dataPlaneKeys
            ) {
                "Remote control session credential contains unsupported fields"
            }
            fun identity(key: String, max: Int): String =
                json.getString(key).trim().also {
                    require(it.isNotEmpty() && it.length <= max && it.none(Char::isISOControl)) {
                        "Remote control session " + key + " is invalid"
                    }
                }
            val token = json.getString("mobileToken")
            require(token.length in 48..256 && token.none(Char::isWhitespace) && token.none(Char::isISOControl)) {
                "Remote control session mobileToken is invalid"
            }
            val requestId = if (!json.has("requestId") || json.isNull("requestId")) {
                null
            } else {
                json.getString("requestId").trim().also {
                    require(
                        it.length in 16..160 &&
                            it.all { character ->
                                character.isLetterOrDigit() || character in setOf('-', '_', ':', '.')
                            },
                    ) { "Remote control session requestId is invalid" }
                }
            }
            val iceServersJson = json.optString("iceServersJson", "[]")
            require(iceServersJson.toByteArray(StandardCharsets.UTF_8).size <= 32 * 1024) {
                "Remote control ICE configuration exceeds protected-store bounds"
            }
            runCatching { org.json.JSONArray(iceServersJson) }.getOrElse {
                throw IllegalArgumentException("Remote control ICE configuration is invalid", it)
            }
            val epoch = json.getLong("accountEpoch")
            require(epoch > 0L) { "Remote control session account epoch must be positive" }
            val expiresAt = json.getLong("expiresAt")
            require(expiresAt > 0L) { "Remote control session expiry must be positive" }
            val lastAcknowledgedSignalId = json.optLong("lastAcknowledgedSignalId", 0L)
            val highestDrainedSignalId = json.optLong("highestDrainedSignalId", 0L)
            require(lastAcknowledgedSignalId >= 0L) {
                "Remote control acknowledged signal cursor must not be negative"
            }
            require(highestDrainedSignalId >= lastAcknowledgedSignalId) {
                "Remote control drained signal cursor must not precede acknowledged cursor"
            }
            fun optionalIdentity(key: String, max: Int): String? {
                if (!json.has(key) || json.isNull(key)) return null
                return json.getString(key).trim().also {
                    require(it.isNotEmpty() && it.length <= max && it.none(Char::isISOControl)) {
                        "Remote control session " + key + " is invalid"
                    }
                }
            }
            val provider = optionalIdentity("provider", 80)
            val routePolicy = optionalIdentity("routePolicy", 40)
            require(routePolicy == null || routePolicy in setOf("direct-preferred", "relay-only")) {
                "Remote control route policy is invalid"
            }
            val selectedRoute = optionalIdentity("selectedRoute", 20)
            require(selectedRoute == null || selectedRoute in setOf("direct", "relay")) {
                "Remote control selected route is invalid"
            }
            val relayRegion = optionalIdentity("relayRegion", 32)
            require(selectedRoute == "relay" || relayRegion == null) {
                "Remote control relay region requires relay route"
            }
            val transportUpdatedAt = json.optLong("transportUpdatedAt", 0L)
            require(transportUpdatedAt >= 0L) { "Remote control transport timestamp is invalid" }
            require(selectedRoute == null || transportUpdatedAt > 0L) {
                "Remote control selected route requires a transport timestamp"
            }
            val processGeneration = json.optLong("processGeneration", 0L)
            require(processGeneration >= 0L) { "Remote control process generation is invalid" }
            val viewportRevision = json.optLong("viewportRevision", 0L)
            require(viewportRevision >= 0L) { "Remote control viewport revision is invalid" }
            val lifecycle = json.optString("lifecycle")
                .takeIf(String::isNotBlank)
                ?.let { RemoteControlSessionLifecycle.valueOf(it) }
                ?: RemoteControlSessionLifecycle.PENDING
            val reconnectCount = json.optInt("reconnectCount", 0)
            require(reconnectCount >= 0) { "Remote control reconnect count is invalid" }
            val reconcileRequired = json.optBoolean("reconcileRequired", false)
            return RemoteControlSessionCredential(
                deviceId = identity("deviceId", 160),
                clientId = identity("clientId", 160),
                sessionId = identity("sessionId", 160),
                mobileToken = token,
                requestId = requestId,
                iceServersJson = iceServersJson,
                accountFence = identity("accountFence", 512),
                accountEpoch = epoch,
                expiresAt = expiresAt,
                lastAcknowledgedSignalId = lastAcknowledgedSignalId,
                highestDrainedSignalId = highestDrainedSignalId,
                provider = provider,
                routePolicy = routePolicy,
                selectedRoute = selectedRoute,
                relayRegion = relayRegion,
                transportUpdatedAt = transportUpdatedAt,
                processGeneration = processGeneration,
                viewportRevision = viewportRevision,
                humanTakeover = json.optBoolean("humanTakeover", false),
                lifecycle = lifecycle,
                reconnectCount = reconnectCount,
                reconcileRequired = reconcileRequired,
            )
        }
    }
}

internal object RemoteControlTransportPolicy {
    fun record(
        value: RemoteControlSessionCredential,
        sessionId: String,
        provider: String,
        routePolicy: String,
        selectedRoute: String,
        relayRegion: String?,
        transportUpdatedAt: Long,
    ): RemoteControlSessionCredential {
        require(value.sessionId == sessionId) { "Remote control transport session changed" }
        require(provider.isNotBlank() && provider.length <= 80 && provider.none(Char::isISOControl)) {
            "Remote control provider is invalid"
        }
        require(routePolicy in setOf("direct-preferred", "relay-only")) {
            "Remote control route policy is invalid"
        }
        require(selectedRoute in setOf("direct", "relay")) {
            "Remote control selected route is invalid"
        }
        require(selectedRoute == "relay" || relayRegion == null) {
            "Remote control direct route must not retain relay region"
        }
        require(
            relayRegion == null ||
                (relayRegion.length <= 32 &&
                    relayRegion.all { it.isLetterOrDigit() || it == '-' || it == '_' }),
        ) { "Remote control relay region is invalid" }
        require(transportUpdatedAt > 0L && transportUpdatedAt >= value.transportUpdatedAt) {
            "Remote control transport timestamp regressed"
        }
        require(value.provider == null || value.provider == provider) {
            "Remote control provider changed during session"
        }
        require(value.routePolicy == null || value.routePolicy == routePolicy) {
            "Remote control route policy changed during session"
        }
        require(!(value.selectedRoute == "relay" && selectedRoute == "direct")) {
            "Remote control relay route cannot upgrade back to direct"
        }
        if (routePolicy == "relay-only") {
            require(selectedRoute == "relay") { "Relay-only session selected a direct route" }
        }
        return value.copy(
            provider = provider,
            routePolicy = routePolicy,
            selectedRoute = selectedRoute,
            relayRegion = relayRegion,
            transportUpdatedAt = transportUpdatedAt,
        )
    }
}

internal object RemoteControlSessionStatePolicy {
    fun bindProcess(
        value: RemoteControlSessionCredential,
        processGeneration: Long,
    ): RemoteControlSessionCredential {
        require(processGeneration > 0L) { "Remote control process generation must be positive" }
        if (value.processGeneration == processGeneration) return value
        val recoveredLifecycle = when (value.lifecycle) {
            RemoteControlSessionLifecycle.CLOSING,
            RemoteControlSessionLifecycle.OUTCOME_UNKNOWN -> RemoteControlSessionLifecycle.OUTCOME_UNKNOWN
            else -> RemoteControlSessionLifecycle.RECONNECTING
        }
        return value.copy(
            processGeneration = processGeneration,
            viewportRevision = value.viewportRevision + 1,
            humanTakeover = false,
            lifecycle = recoveredLifecycle,
            reconnectCount = value.reconnectCount + 1,
            reconcileRequired = true,
        )
    }

    fun transportReady(
        value: RemoteControlSessionCredential,
        processGeneration: Long,
    ): RemoteControlSessionCredential {
        require(value.lifecycle !in setOf(
            RemoteControlSessionLifecycle.CLOSING,
            RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
        )) { "Remote control session cannot negotiate after local close" }
        require(processGeneration > 0L) { "Remote control process generation must be positive" }
        return value.copy(
            processGeneration = processGeneration,
            lifecycle = RemoteControlSessionLifecycle.NEGOTIATING,
            reconcileRequired = false,
        )
    }

    fun setHumanTakeover(
        value: RemoteControlSessionCredential,
        expectedViewportRevision: Long,
        active: Boolean,
    ): RemoteControlSessionCredential {
        require(expectedViewportRevision == value.viewportRevision) {
            "Remote control human takeover used a stale viewport revision"
        }
        require(value.lifecycle !in setOf(
            RemoteControlSessionLifecycle.CLOSING,
            RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
        )) { "Remote control session is closing or outcome-unknown" }
        val nextLifecycle = if (active) {
            RemoteControlSessionLifecycle.HUMAN_TAKEOVER
        } else if (value.selectedRoute == null) {
            RemoteControlSessionLifecycle.PENDING
        } else {
            RemoteControlSessionLifecycle.NEGOTIATING
        }
        return value.copy(
            viewportRevision = value.viewportRevision + 1,
            humanTakeover = active,
            lifecycle = nextLifecycle,
        )
    }

    fun advanceViewport(
        value: RemoteControlSessionCredential,
        expectedViewportRevision: Long,
    ): RemoteControlSessionCredential {
        require(expectedViewportRevision == value.viewportRevision) {
            "Remote control viewport revision is stale"
        }
        require(value.lifecycle !in setOf(
            RemoteControlSessionLifecycle.CLOSING,
            RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
        )) { "Remote control session cannot advance a closed viewport" }
        return value.copy(viewportRevision = value.viewportRevision + 1)
    }

    fun reconcileAfterDrain(
        value: RemoteControlSessionCredential,
        sawReady: Boolean,
        sawClose: Boolean,
    ): RemoteControlSessionCredential {
        if (sawClose) {
            return value.copy(
                viewportRevision = value.viewportRevision + 1,
                humanTakeover = false,
                lifecycle = RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
                reconcileRequired = true,
            )
        }
        val reconciledLifecycle = when {
            value.humanTakeover -> RemoteControlSessionLifecycle.HUMAN_TAKEOVER
            sawReady && value.selectedRoute != null -> RemoteControlSessionLifecycle.READY
            value.selectedRoute != null -> RemoteControlSessionLifecycle.NEGOTIATING
            else -> RemoteControlSessionLifecycle.PENDING
        }
        return value.copy(
            lifecycle = reconciledLifecycle,
            reconcileRequired = false,
        )
    }

    fun markReconnectRequired(
        value: RemoteControlSessionCredential,
        processGeneration: Long,
        expectedViewportRevision: Long,
    ): RemoteControlSessionCredential {
        require(processGeneration == value.processGeneration) {
            "Remote control reconnect crossed process generation"
        }
        require(expectedViewportRevision == value.viewportRevision) {
            "Remote control reconnect used a stale viewport revision"
        }
        require(value.lifecycle !in setOf(
            RemoteControlSessionLifecycle.CLOSING,
            RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
        )) { "Remote control session cannot reconnect while closing" }
        return value.copy(
            lifecycle = RemoteControlSessionLifecycle.RECONNECTING,
            reconnectCount = value.reconnectCount + 1,
            reconcileRequired = true,
            humanTakeover = false,
        )
    }

    fun beginClosing(value: RemoteControlSessionCredential): RemoteControlSessionCredential =
        value.copy(
            viewportRevision = value.viewportRevision + 1,
            humanTakeover = false,
            lifecycle = RemoteControlSessionLifecycle.CLOSING,
            reconcileRequired = true,
        )

    fun markCloseOutcomeUnknown(value: RemoteControlSessionCredential): RemoteControlSessionCredential =
        value.copy(
            humanTakeover = false,
            lifecycle = RemoteControlSessionLifecycle.OUTCOME_UNKNOWN,
            reconcileRequired = true,
        )
}

internal object RemoteControlSignalCursorPolicy {
    fun recordDrain(
        value: RemoteControlSessionCredential,
        sessionId: String,
        afterSignalId: Long,
        lastSignalId: Long,
    ): RemoteControlSessionCredential {
        require(value.sessionId == sessionId) { "Remote control signal session changed during drain" }
        require(afterSignalId == value.lastAcknowledgedSignalId) {
            "Remote signal drain must start at the durable acknowledged cursor"
        }
        require(lastSignalId >= afterSignalId) { "Remote signal drain cursor regressed" }
        require(lastSignalId >= value.highestDrainedSignalId) {
            "Remote unacknowledged signals were not redelivered; reconnect is required"
        }
        return value.copy(
            highestDrainedSignalId = maxOf(value.highestDrainedSignalId, lastSignalId),
        )
    }

    fun acknowledge(
        value: RemoteControlSessionCredential,
        sessionId: String,
        lastSignalId: Long,
    ): RemoteControlSessionCredential {
        require(value.sessionId == sessionId) { "Remote control signal session changed before acknowledgement" }
        require(lastSignalId >= value.lastAcknowledgedSignalId) {
            "Remote signal acknowledgement regressed"
        }
        require(lastSignalId == value.highestDrainedSignalId) {
            "Remote signal acknowledgement must match the most recent fully drained batch"
        }
        return value.copy(lastAcknowledgedSignalId = lastSignalId)
    }
}

/**
 * Android protected owner for one short-lived /v1/computers control session.
 *
 * mobileToken authorizes only the paired mobile actor after the target computer activates the
 * session with its independent deviceSecret. It is never exposed to Presentation, never persisted
 * as a RemoteDispatchBinding bearer, and is fenced to the current account epoch and account fence.
 */
internal class AndroidRemoteControlSessionStore(context: Context) {
    private val credentialFile = File(context.applicationContext.noBackupFilesDir, FILE_NAME)

    @Synchronized
    fun readForAccountFence(
        currentAccountFence: String,
        currentAccountEpoch: Long,
    ): RemoteControlSessionCredential? {
        val value = read() ?: return null
        if (value.accountFence != currentAccountFence || value.accountEpoch != currentAccountEpoch) {
            clear()
            return null
        }
        return value
    }

    @Synchronized
    fun recordTransport(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        provider: String,
        routePolicy: String,
        selectedRoute: String,
        relayRegion: String?,
        transportUpdatedAt: Long,
        processGeneration: Long,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        val updated = RemoteControlSessionStatePolicy.transportReady(
            RemoteControlTransportPolicy.record(
                current,
                sessionId,
                provider,
                routePolicy,
                selectedRoute,
                relayRegion,
                transportUpdatedAt,
            ),
            processGeneration,
        )
        write(updated)
        return updated
    }

    @Synchronized
    fun bindProcess(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        processGeneration: Long,
    ): RemoteControlSessionCredential? {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch) ?: return null
        val updated = RemoteControlSessionStatePolicy.bindProcess(current, processGeneration)
        if (updated != current) write(updated)
        return updated
    }

    @Synchronized
    fun setHumanTakeover(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        expectedViewportRevision: Long,
        active: Boolean,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.setHumanTakeover(
            current,
            expectedViewportRevision,
            active,
        )
        write(updated)
        return updated
    }

    @Synchronized
    fun advanceViewport(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        expectedViewportRevision: Long,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.advanceViewport(current, expectedViewportRevision)
        write(updated)
        return updated
    }

    @Synchronized
    fun markReconnectRequired(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        processGeneration: Long,
        expectedViewportRevision: Long,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.markReconnectRequired(
            current,
            processGeneration,
            expectedViewportRevision,
        )
        write(updated)
        return updated
    }

    @Synchronized
    fun beginClosing(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.beginClosing(current)
        write(updated)
        return updated
    }

    @Synchronized
    fun markCloseOutcomeUnknown(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.markCloseOutcomeUnknown(current)
        write(updated)
        return updated
    }

    @Synchronized
    fun recordSignalDrain(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        afterSignalId: Long,
        lastSignalId: Long,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        val updated = RemoteControlSignalCursorPolicy.recordDrain(
            current,
            sessionId,
            afterSignalId,
            lastSignalId,
        )
        write(updated)
        return updated
    }

    @Synchronized
    fun reconcileAfterSignalDrain(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        sawReady: Boolean,
        sawClose: Boolean,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        require(current.sessionId == sessionId) { "Remote control session identity changed" }
        val updated = RemoteControlSessionStatePolicy.reconcileAfterDrain(current, sawReady, sawClose)
        write(updated)
        return updated
    }

    @Synchronized
    fun acknowledgeSignals(
        currentAccountFence: String,
        currentAccountEpoch: Long,
        sessionId: String,
        lastSignalId: Long,
    ): RemoteControlSessionCredential {
        val current = readForAccountFence(currentAccountFence, currentAccountEpoch)
            ?: error("Remote control session is unavailable")
        val updated = RemoteControlSignalCursorPolicy.acknowledge(current, sessionId, lastSignalId)
        write(updated)
        return updated
    }

    @Synchronized
    fun write(value: RemoteControlSessionCredential) {
        RemoteControlSessionCredential.parse(value.toSecretJson())
        val plaintext = value.toSecretJson().toByteArray(StandardCharsets.UTF_8)
        require(plaintext.size <= MAX_BYTES) { "Remote control session credential exceeds bounded secret payload" }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(plaintext)
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }
        credentialFile.parentFile?.mkdirs()
        val temporary = File(credentialFile.parentFile, credentialFile.name + ".tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            if (!temporary.renameTo(credentialFile)) {
                credentialFile.delete()
                check(temporary.renameTo(credentialFile)) {
                    "failed to atomically replace Remote control session credential"
                }
            }
        } finally {
            temporary.delete()
        }
    }

    @Synchronized
    fun clear() {
        runCatching { credentialFile.delete() }
    }

    private fun read(): RemoteControlSessionCredential? {
        val encoded = runCatching { credentialFile.readText(StandardCharsets.UTF_8) }.getOrNull()
            ?: return null
        val lines = encoded.split('\n')
        if (lines.size != 3 || lines[0] != FORMAT_VERSION) {
            clear()
            return null
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16 && ciphertext.isNotEmpty()) {
                "invalid Remote control session ciphertext"
            }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            RemoteControlSessionCredential.parse(
                String(cipher.doFinal(ciphertext), StandardCharsets.UTF_8),
            )
        }.getOrElse {
            clear()
            null
        }
    }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEY_STORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val ANDROID_KEY_STORE = "AndroidKeyStore"
        const val KEY_ALIAS = "fabushi.remote.control.session.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-remote-control-session-v1"
        const val FILE_NAME = "fabushi-remote-control-session.v1"
        const val GCM_TAG_BITS = 128
        const val MAX_BYTES = 8 * 1024
    }
}
