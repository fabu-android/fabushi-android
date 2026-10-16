package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal class AndroidPluginVariableSecretStore(context: Context) {
    private val secretFile = File(context.applicationContext.noBackupFilesDir, SECRET_FILE_NAME)

    @Synchronized
    fun replace(accountKey: String, pluginId: String, values: JSONObject) {
        validateIdentity(accountKey, "account key")
        validateIdentity(pluginId, "plugin id")
        val sanitized = JSONObject()
        val keys = values.keys()
        var count = 0
        while (keys.hasNext()) {
            val key = keys.next()
            validateIdentity(key, "variable key")
            val value = values.opt(key)
            require(value is String) { "plugin secret value must be a string" }
            require(value.toByteArray(StandardCharsets.UTF_8).size <= MAX_VALUE_BYTES) {
                "plugin secret value exceeds bounded payload"
            }
            sanitized.put(key, value)
            count += 1
            require(count <= MAX_FIELDS) { "plugin secret field count exceeds bound" }
        }
        val root = readDocument()
        val accounts = root.optJSONObject("accounts") ?: JSONObject().also { root.put("accounts", it) }
        val account = accounts.optJSONObject(accountKey) ?: JSONObject().also { accounts.put(accountKey, it) }
        if (sanitized.length() == 0) {
            account.remove(pluginId)
            if (account.length() == 0) accounts.remove(accountKey)
        } else {
            account.put(pluginId, sanitized)
        }
        writeDocument(root)
    }

    @Synchronized
    fun read(accountKey: String, pluginId: String, requestedKeys: JSONArray): JSONObject {
        validateIdentity(accountKey, "account key")
        validateIdentity(pluginId, "plugin id")
        val source = readDocument().optJSONObject("accounts")
            ?.optJSONObject(accountKey)?.optJSONObject(pluginId) ?: JSONObject()
        val result = JSONObject()
        for (index in 0 until requestedKeys.length()) {
            val key = requestedKeys.optString(index)
            validateIdentity(key, "variable key")
            val value = source.opt(key)
            check(value is String) { "protected plugin secret is missing for $key" }
            result.put(key, value)
        }
        return result
    }

    @Synchronized
    fun clearAccount(accountKey: String) {
        validateIdentity(accountKey, "account key")
        val root = readDocument()
        root.optJSONObject("accounts")?.remove(accountKey)
        writeDocument(root)
    }

    private fun readDocument(): JSONObject {
        val encoded = runCatching { secretFile.readText(StandardCharsets.UTF_8) }.getOrNull()
            ?: return freshDocument()
        val lines = encoded.split('\n')
        if (lines.size != 3 || lines[0] != FORMAT_VERSION) {
            clearCorrupt()
            return freshDocument()
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16 && ciphertext.isNotEmpty())
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            val plaintext = cipher.doFinal(ciphertext)
            require(plaintext.size <= MAX_DOCUMENT_BYTES)
            JSONObject(String(plaintext, StandardCharsets.UTF_8)).also {
                require(it.optInt("version", -1) == DOCUMENT_VERSION)
            }
        }.getOrElse {
            clearCorrupt()
            freshDocument()
        }
    }

    private fun writeDocument(root: JSONObject) {
        if ((root.optJSONObject("accounts")?.length() ?: 0) == 0) {
            runCatching { secretFile.delete() }
            return
        }
        val plaintext = root.toString().toByteArray(StandardCharsets.UTF_8)
        require(plaintext.size <= MAX_DOCUMENT_BYTES)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(plaintext)
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }
        secretFile.parentFile?.mkdirs()
        val temporary = File(secretFile.parentFile, "${secretFile.name}.tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            if (!temporary.renameTo(secretFile)) {
                secretFile.delete()
                check(temporary.renameTo(secretFile))
            }
        } finally {
            temporary.delete()
        }
    }

    private fun freshDocument(): JSONObject =
        JSONObject().put("version", DOCUMENT_VERSION).put("accounts", JSONObject())

    private fun clearCorrupt() {
        runCatching { secretFile.delete() }
    }

    private fun validateIdentity(value: String, label: String) {
        require(value.isNotBlank() && value.length <= 512 && value.none(Char::isISOControl)) {
            "invalid plugin-secret $label"
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
            ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val ANDROID_KEY_STORE = "AndroidKeyStore"
        const val KEY_ALIAS = "fabushi.plugin.variables.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-plugin-secrets-v1"
        const val SECRET_FILE_NAME = "fabushi-plugin-secrets.v1"
        const val DOCUMENT_VERSION = 1
        const val GCM_TAG_BITS = 128
        const val MAX_FIELDS = 256
        const val MAX_VALUE_BYTES = 64 * 1024
        const val MAX_DOCUMENT_BYTES = 2 * 1024 * 1024
    }
}
