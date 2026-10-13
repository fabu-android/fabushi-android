package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.system.Os
import android.util.Base64
import java.io.File
import java.io.FileOutputStream
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Android platform owner for the stable machine/device identifier used by the Rust Host.
 *
 * Production never persists the identifier as plaintext. Existing installs migrate the legacy
 * filesDir/machine-id value only after the Keystore-protected replacement is durably committed.
 * Corrupt protected state fails closed instead of silently rotating device identity.
 */
internal class AndroidMachineIdStore(context: Context) {
    private val applicationContext = context.applicationContext
    private val protectedFile = File(applicationContext.noBackupFilesDir, FILE_NAME)
    private val legacyFile = File(applicationContext.filesDir, LEGACY_FILE_NAME)

    fun getOrCreate(): String = synchronized(PROCESS_LOCK) {
        readProtected()?.let { return@synchronized it }

        val legacy = readLegacy()
        val machineId = legacy ?: UUID.randomUUID().toString()
        writeProtected(machineId)

        if (legacy != null && legacyFile.exists() && !legacyFile.delete()) {
            throw IllegalStateException("failed to remove legacy plaintext machine id")
        }
        machineId
    }

    private fun readProtected(): String? {
        if (!protectedFile.exists()) return null
        val encoded = protectedFile.readText(StandardCharsets.UTF_8)
        val lines = encoded.split('\n')
        require(lines.size == 3 && lines[0] == FORMAT_VERSION) {
            "invalid protected machine-id document"
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16 && ciphertext.isNotEmpty()) {
                "invalid protected machine-id ciphertext"
            }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            validateMachineId(String(cipher.doFinal(ciphertext), StandardCharsets.UTF_8))
        }.getOrElse { error ->
            throw IllegalStateException("protected machine id is unreadable", error)
        }
    }

    private fun readLegacy(): String? {
        if (!legacyFile.exists()) return null
        return validateMachineId(legacyFile.readText(StandardCharsets.UTF_8))
    }

    private fun writeProtected(value: String) {
        val machineId = validateMachineId(value)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(machineId.toByteArray(StandardCharsets.UTF_8))
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }

        protectedFile.parentFile?.mkdirs()
        val temporary = File(protectedFile.parentFile, "${protectedFile.name}.tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            // Both paths live in app-private storage on the same filesystem. POSIX rename
            // atomically replaces an existing target, so a crash can expose either the old
            // complete ciphertext or the new complete ciphertext, never a missing/partial owner.
            Os.rename(temporary.absolutePath, protectedFile.absolutePath)
        } finally {
            temporary.delete()
        }
    }

    private fun validateMachineId(raw: String): String {
        val value = raw.trim()
        require(value.length in 1..MAX_MACHINE_ID_LENGTH) { "machine id length is invalid" }
        require(value.none(Char::isWhitespace) && value.none(Char::isISOControl)) {
            "machine id contains unsupported characters"
        }
        return value
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
        const val KEY_ALIAS = "fabushi.machine-id.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-machine-id-v1"
        const val FILE_NAME = "fabushi-machine-id.v1"
        const val LEGACY_FILE_NAME = "machine-id"
        const val GCM_TAG_BITS = 128
        const val MAX_MACHINE_ID_LENGTH = 200
        val PROCESS_LOCK = Any()
    }
}
