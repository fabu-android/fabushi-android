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

internal data class RemotePairingCredential(
    val deviceId: String,
    val clientId: String,
    val clientToken: String,
    val accountFence: String,
    val accountEpoch: Long,
) {
    fun toSecretJson(): String = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("clientToken", clientToken)
        .put("accountFence", accountFence)
        .put("accountEpoch", accountEpoch)
        .toString()

    fun publicProjection(): JSONObject = JSONObject()
        .put("deviceId", deviceId)
        .put("clientId", clientId)
        .put("accountEpoch", accountEpoch)

    companion object {
        fun parse(value: String): RemotePairingCredential {
            val json = JSONObject(value)
            require(
                json.keys().asSequence().toSet() ==
                    setOf("deviceId", "clientId", "clientToken", "accountFence", "accountEpoch"),
            ) { "Remote pairing credential contains unsupported fields" }
            fun identity(key: String, max: Int): String =
                json.getString(key).trim().also {
                    require(it.isNotEmpty() && it.length <= max && it.none(Char::isISOControl)) {
                        "Remote pairing $key is invalid"
                    }
                }
            val token = json.getString("clientToken")
            require(token.length in 48..256 && token.none(Char::isWhitespace) && token.none(Char::isISOControl)) {
                "Remote pairing clientToken is invalid"
            }
            val epoch = json.getLong("accountEpoch")
            require(epoch > 0L) { "Remote pairing account epoch must be positive" }
            return RemotePairingCredential(
                deviceId = identity("deviceId", 160),
                clientId = identity("clientId", 160),
                clientToken = token,
                accountFence = identity("accountFence", 512),
                accountEpoch = epoch,
            )
        }
    }
}

/**
 * Android protected owner for the /v1/computers paired-client credential.
 *
 * clientToken belongs only to the Remote Computer control plane. It is never converted into
 * RemoteDispatchBinding.bearerCredential and never leaves this store through public projections.
 */
internal class AndroidRemotePairingStore(context: Context) {
    private val credentialFile = File(context.applicationContext.noBackupFilesDir, FILE_NAME)

    fun readForAccountFence(currentAccountFence: String, currentAccountEpoch: Long): RemotePairingCredential? {
        val value = read() ?: return null
        if (value.accountFence != currentAccountFence || value.accountEpoch != currentAccountEpoch) {
            clear()
            return null
        }
        return value
    }

    fun write(value: RemotePairingCredential) {
        RemotePairingCredential.parse(value.toSecretJson())
        val plaintext = value.toSecretJson().toByteArray(StandardCharsets.UTF_8)
        require(plaintext.size <= MAX_BYTES) { "Remote pairing credential exceeds bounded secret payload" }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(plaintext)
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }
        credentialFile.parentFile?.mkdirs()
        val temporary = File(credentialFile.parentFile, "${credentialFile.name}.tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            if (!temporary.renameTo(credentialFile)) {
                credentialFile.delete()
                check(temporary.renameTo(credentialFile)) { "failed to atomically replace Remote pairing credential" }
            }
        } finally {
            temporary.delete()
        }
    }

    fun clear() {
        runCatching { credentialFile.delete() }
    }

    private fun read(): RemotePairingCredential? {
        val encoded = runCatching { credentialFile.readText(StandardCharsets.UTF_8) }.getOrNull() ?: return null
        val lines = encoded.split('\n')
        if (lines.size != 3 || lines[0] != FORMAT_VERSION) {
            clear()
            return null
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16 && ciphertext.isNotEmpty()) { "invalid Remote pairing ciphertext" }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            RemotePairingCredential.parse(String(cipher.doFinal(ciphertext), StandardCharsets.UTF_8))
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
        const val KEY_ALIAS = "fabushi.remote.control.pairing.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-remote-pairing-v1"
        const val FILE_NAME = "fabushi-remote-pairing.v1"
        const val GCM_TAG_BITS = 128
        const val MAX_BYTES = 8 * 1024
    }
}
