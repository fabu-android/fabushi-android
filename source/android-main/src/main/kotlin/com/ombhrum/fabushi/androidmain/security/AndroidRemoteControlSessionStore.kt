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

internal data class RemoteControlSessionCredential(
    val deviceId: String,
    val clientId: String,
    val sessionId: String,
    val mobileToken: String,
    val accountFence: String,
    val accountEpoch: Long,
    val expiresAt: Long,
    val lastAcknowledgedSignalId: Long = 0L,
    val highestDrainedSignalId: Long = 0L,
) {
    fun toSecretJson(): String = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("sessionId", sessionId)
        .put("mobileToken", mobileToken)
        .put("accountFence", accountFence)
        .put("accountEpoch", accountEpoch)
        .put("expiresAt", expiresAt)
        .put("lastAcknowledgedSignalId", lastAcknowledgedSignalId)
        .put("highestDrainedSignalId", highestDrainedSignalId)
        .toString()

    fun publicProjection(): JSONObject = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("sessionId", sessionId)
        .put("accountEpoch", accountEpoch)
        .put("expiresAt", expiresAt)
        .put("lastAcknowledgedSignalId", lastAcknowledgedSignalId)
        .put("highestDrainedSignalId", highestDrainedSignalId)

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
            val currentKeys = legacyKeys + setOf(
                "lastAcknowledgedSignalId",
                "highestDrainedSignalId",
            )
            val observedKeys = json.keys().asSequence().toSet()
            require(observedKeys == legacyKeys || observedKeys == currentKeys) {
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
            return RemoteControlSessionCredential(
                deviceId = identity("deviceId", 160),
                clientId = identity("clientId", 160),
                sessionId = identity("sessionId", 160),
                mobileToken = token,
                accountFence = identity("accountFence", 512),
                accountEpoch = epoch,
                expiresAt = expiresAt,
                lastAcknowledgedSignalId = lastAcknowledgedSignalId,
                highestDrainedSignalId = highestDrainedSignalId,
            )
        }
    }
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
