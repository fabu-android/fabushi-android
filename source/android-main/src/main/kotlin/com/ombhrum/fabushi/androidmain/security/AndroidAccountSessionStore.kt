package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.io.File
import java.io.FileOutputStream
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Platform-only secret resource for the canonical Rust account service.
 *
 * The Rust Host owns account/session business state. This class owns only Android credential
 * protection: a Keystore AES key plus a no-backup ciphertext file. Decrypted session JSON is never
 * exposed to Compose/ViewModel state and is exchanged only with the process-owned native Host
 * boundary.
 */
internal class AndroidAccountSessionStore(context: Context) {
    private val applicationContext = context.applicationContext
    private val sessionFile = File(applicationContext.noBackupFilesDir, SESSION_FILE_NAME)

    fun readSessionJson(): String? {
        val encoded = runCatching { sessionFile.readText(StandardCharsets.UTF_8) }.getOrNull()
            ?: return null
        val lines = encoded.split('\n')
        if (lines.size != 3 || lines[0] != FORMAT_VERSION) {
            clear()
            return null
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16) { "invalid account-session IV" }
            require(ciphertext.isNotEmpty()) { "empty account-session ciphertext" }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            String(cipher.doFinal(ciphertext), StandardCharsets.UTF_8)
                .takeIf { it.isNotBlank() }
                ?: error("empty decrypted account session")
        }.getOrElse {
            clear()
            null
        }
    }

    fun writeSessionJson(value: String) {
        require(value.isNotBlank()) { "account session must not be blank" }
        require(value.toByteArray(StandardCharsets.UTF_8).size <= MAX_SESSION_BYTES) {
            "account session exceeds bounded secret payload"
        }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(value.toByteArray(StandardCharsets.UTF_8))
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }

        sessionFile.parentFile?.mkdirs()
        val temporary = File(sessionFile.parentFile, "${sessionFile.name}.tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            if (!temporary.renameTo(sessionFile)) {
                sessionFile.delete()
                check(temporary.renameTo(sessionFile)) { "failed to atomically replace account session" }
            }
        } finally {
            temporary.delete()
        }
    }

    fun clear() {
        runCatching { sessionFile.delete() }
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
        const val KEY_ALIAS = "fabushi.account.session.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-account-session-v1"
        const val SESSION_FILE_NAME = "fabushi-account-session.v1"
        const val GCM_TAG_BITS = 128
        const val MAX_SESSION_BYTES = 64 * 1024
    }
}
