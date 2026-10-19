package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.io.File
import java.io.FileOutputStream
import java.net.URI
import java.nio.charset.StandardCharsets
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONArray
import org.json.JSONObject

/** Account-scoped no-reveal credential vault backed by Android Keystore. */
internal class AndroidCredentialVaultStore(
    context: Context,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {
    private val vaultFile = File(context.applicationContext.noBackupFilesDir, FILE_NAME)

    @Synchronized fun listMetadata(accountKey: String): JSONArray {
        validateIdentity(accountKey, "account key")
        val records = accountRecords(readDocument(), accountKey, false) ?: JSONObject()
        return JSONArray().also { result ->
            records.keys().asSequence().sorted().forEach { name -> result.put(metadata(name, records.getJSONObject(name))) }
        }
    }

    @Synchronized fun upsert(accountKey: String, name: String, label: String, secretValue: String, allowedOrigins: List<String>) =
        write(accountKey, name, label, secretValue, allowedOrigins, false)

    @Synchronized fun rotate(accountKey: String, name: String, label: String, secretValue: String, allowedOrigins: List<String>) =
        write(accountKey, name, label, secretValue, allowedOrigins, true)

    @Synchronized fun revoke(accountKey: String, name: String): Boolean {
        validateIdentity(accountKey, "account key"); validateIdentity(name, "secret reference")
        val root = readDocument(); val accounts = root.getJSONObject("accounts")
        val records = accountRecords(root, accountKey, false) ?: return false
        if (!records.has(name)) return false
        records.remove(name); if (records.length() == 0) accounts.remove(accountKey); writeDocument(root); return true
    }

    /** Execution-boundary access only. There is deliberately no UI/bridge reveal route. */
    @Synchronized internal fun resolveForOrigin(accountKey: String, name: String, origin: String): String {
        validateIdentity(accountKey, "account key"); validateIdentity(name, "secret reference")
        val normalized = normalizeOrigin(origin)
        val record = accountRecords(readDocument(), accountKey, false)?.optJSONObject(name) ?: error("credential is not configured")
        val allowed = record.getJSONArray("allowedOrigins")
        check((0 until allowed.length()).any { allowed.optString(it) == normalized }) { "credential origin is not authorized" }
        return record.getString("secret")
    }

    private fun write(accountKey: String, name: String, label: String, secretValue: String, allowedOrigins: List<String>, requireExisting: Boolean): JSONObject {
        validateIdentity(accountKey, "account key"); validateIdentity(name, "secret reference")
        val cleanLabel = label.trim(); require(cleanLabel.length <= MAX_LABEL_LENGTH && cleanLabel.none(Char::isISOControl))
        require(secretValue.isNotEmpty()); require(secretValue.toByteArray(StandardCharsets.UTF_8).size <= MAX_SECRET_BYTES)
        val origins = allowedOrigins.map(::normalizeOrigin).distinct().sorted(); require(origins.isNotEmpty() && origins.size <= MAX_ORIGINS)
        val root = readDocument(); val records = requireNotNull(accountRecords(root, accountKey, true)); val existing = records.optJSONObject(name)
        if (requireExisting) check(existing != null) { "credential does not exist" }
        val now = nowMs().coerceAtLeast(0L); val createdAt = existing?.optLong("createdAtMs", now) ?: now
        val record = JSONObject().put("label", cleanLabel).put("secret", secretValue).put("allowedOrigins", JSONArray(origins))
            .put("createdAtMs", createdAt).put("updatedAtMs", now.coerceAtLeast(createdAt))
        records.put(name, record); writeDocument(root); return metadata(name, record)
    }

    private fun metadata(name: String, record: JSONObject) = JSONObject().put("name", name).put("label", record.optString("label"))
        .put("configured", true).put("revealable", false)
        .put("binding", JSONObject().put("allowedOrigins", JSONArray(record.getJSONArray("allowedOrigins").toString())))
        .put("createdAtMs", record.optLong("createdAtMs")).put("updatedAtMs", record.optLong("updatedAtMs"))

    private fun accountRecords(root: JSONObject, accountKey: String, create: Boolean): JSONObject? {
        val accounts = root.getJSONObject("accounts")
        return accounts.optJSONObject(accountKey) ?: if (create) JSONObject().also { accounts.put(accountKey, it) } else null
    }

    private fun readDocument(): JSONObject {
        if (!vaultFile.exists()) return freshDocument()
        val lines = vaultFile.readText(StandardCharsets.UTF_8).split('\n'); check(lines.size == 3 && lines[0] == FORMAT_VERSION)
        return try {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP); val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP); check(iv.size in 12..16 && ciphertext.isNotEmpty())
            val cipher = Cipher.getInstance(TRANSFORMATION); cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            val plaintext = cipher.doFinal(ciphertext); check(plaintext.size <= MAX_DOCUMENT_BYTES)
            JSONObject(String(plaintext, StandardCharsets.UTF_8)).also { check(it.optInt("version", -1) == DOCUMENT_VERSION && it.optJSONObject("accounts") != null) }
        } catch (error: Exception) { throw IllegalStateException("credential vault is unreadable", error) }
    }

    private fun writeDocument(root: JSONObject) {
        if (root.getJSONObject("accounts").length() == 0) { if (vaultFile.exists()) check(vaultFile.delete()); return }
        val plaintext = root.toString().toByteArray(StandardCharsets.UTF_8); require(plaintext.size <= MAX_DOCUMENT_BYTES)
        val cipher = Cipher.getInstance(TRANSFORMATION); cipher.init(Cipher.ENCRYPT_MODE, secretKey()); val ciphertext = cipher.doFinal(plaintext)
        val document = buildString { append(FORMAT_VERSION).append('\n'); append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n'); append(Base64.encodeToString(ciphertext, Base64.NO_WRAP)) }
        vaultFile.parentFile?.mkdirs(); val temporary = File(vaultFile.parentFile, "${vaultFile.name}.tmp-${android.os.Process.myPid()}")
        try { FileOutputStream(temporary).use { it.write(document.toByteArray(StandardCharsets.UTF_8)); it.fd.sync() }; Files.move(temporary.toPath(), vaultFile.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING) } finally { temporary.delete() }
    }

    private fun normalizeOrigin(raw: String): String {
        val uri = URI(raw.trim()); val scheme = uri.scheme?.lowercase() ?: error("credential origin requires a scheme"); val host = uri.host?.lowercase() ?: error("credential origin requires a host")
        require(uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null); require(uri.rawPath.isNullOrEmpty() || uri.rawPath == "/")
        require(scheme == "https" || (scheme == "http" && host in LOCAL_HOSTS))
        val defaultPort = (scheme == "https" && uri.port == 443) || (scheme == "http" && uri.port == 80); val authority = if (uri.port < 0 || defaultPort) host else "$host:${uri.port}"
        return "$scheme://$authority"
    }

    private fun validateIdentity(value: String, label: String) { require(value.isNotBlank() && value.length <= MAX_ID_LENGTH && value.none(Char::isISOControl)) { "invalid credential $label" } }
    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply { load(null) }; (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEY_STORE).run { init(KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setRandomizedEncryptionRequired(true).build()); generateKey() }
    }
    private fun freshDocument() = JSONObject().put("version", DOCUMENT_VERSION).put("accounts", JSONObject())
    private companion object {
        const val ANDROID_KEY_STORE = "AndroidKeyStore"; const val KEY_ALIAS = "fabushi.credential-vault.v1"; const val TRANSFORMATION = "AES/GCM/NoPadding"; const val FORMAT_VERSION = "fabushi-credential-vault-v1"; const val FILE_NAME = "fabushi-credential-vault.v1"; const val DOCUMENT_VERSION = 1; const val GCM_TAG_BITS = 128; const val MAX_ID_LENGTH = 512; const val MAX_LABEL_LENGTH = 200; const val MAX_ORIGINS = 32; const val MAX_SECRET_BYTES = 64 * 1024; const val MAX_DOCUMENT_BYTES = 2 * 1024 * 1024
        val LOCAL_HOSTS = setOf("localhost", "127.0.0.1", "::1")
    }
}
