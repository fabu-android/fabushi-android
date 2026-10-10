package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.net.URI
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Platform protected-storage owner for one outbound Remote execution binding.
 * Presentation never receives the credential or chooses the execution target.
 */
internal class AndroidRemoteBindingStore(context: Context) {
    private val applicationContext = context.applicationContext
    private val bindingFile = File(applicationContext.noBackupFilesDir, FILE_NAME)

    fun readBindingJson(): String? {
        val encoded = runCatching { bindingFile.readText(StandardCharsets.UTF_8) }.getOrNull()
            ?: return null
        val lines = encoded.split('\n')
        if (lines.size != 3 || lines[0] != FORMAT_VERSION) {
            clear()
            return null
        }
        return runCatching {
            val iv = Base64.decode(lines[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(lines[2], Base64.NO_WRAP)
            require(iv.size in 12..16) { "invalid Remote binding IV" }
            require(ciphertext.isNotEmpty()) { "empty Remote binding ciphertext" }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            String(cipher.doFinal(ciphertext), StandardCharsets.UTF_8)
                .also(RemoteBindingCredentialContract::validate)
        }.getOrElse {
            clear()
            null
        }
    }

    /**
     * Return the protected binding only when it is still fenced to the canonical
     * account session currently owned by the Rust Host. A stale account/session
     * binding is destroyed before it can be reinstalled after login, token
     * refresh, account switch, or process restart.
     */
    fun readBindingJsonForAccountFence(currentAccountFence: String): String? {
        require(currentAccountFence.isNotBlank()) { "current account fence must not be blank" }
        val value = readBindingJson() ?: return null
        if (!RemoteBindingFencePolicy.matches(value, currentAccountFence)) {
            clear()
            return null
        }
        return value
    }

    /** Protected pairing boundary only; no presentation-facing owner calls this. */
    fun writeBindingJson(value: String) {
        RemoteBindingCredentialContract.validate(value)
        require(value.toByteArray(StandardCharsets.UTF_8).size <= MAX_BINDING_BYTES) {
            "Remote binding exceeds bounded secret payload"
        }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(value.toByteArray(StandardCharsets.UTF_8))
        val document = buildString {
            append(FORMAT_VERSION).append('\n')
            append(Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).append('\n')
            append(Base64.encodeToString(ciphertext, Base64.NO_WRAP))
        }
        bindingFile.parentFile?.mkdirs()
        val temporary = File(bindingFile.parentFile, "${bindingFile.name}.tmp")
        try {
            FileOutputStream(temporary).use { stream ->
                stream.write(document.toByteArray(StandardCharsets.UTF_8))
                stream.fd.sync()
            }
            if (!temporary.renameTo(bindingFile)) {
                bindingFile.delete()
                check(temporary.renameTo(bindingFile)) { "failed to atomically replace Remote binding" }
            }
        } finally {
            temporary.delete()
        }
    }

    fun clear() {
        runCatching { bindingFile.delete() }
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
        const val KEY_ALIAS = "fabushi.remote.outbound.binding.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val FORMAT_VERSION = "fabushi-remote-binding-v1"
        const val FILE_NAME = "fabushi-remote-outbound-binding.v1"
        const val GCM_TAG_BITS = 128
        const val MAX_BINDING_BYTES = 32 * 1024
    }
}

/**
 * Exact credential schema for outbound Remote Runner execution.
 *
 * This plane is deliberately distinct from /v1/computers clientToken/mobileToken/deviceSecret
 * and Codex app-server remote_control_token. Those credentials cannot be relabelled as an
 * executor bearer without a real Authorized Remote Runner enrollment contract.
 */
internal object RemoteBindingCredentialContract {
    const val EXECUTOR_CREDENTIAL_PLANE = "authorized-remote-runner-v1"

    fun validate(value: String) {
        require(value.isNotBlank()) { "Remote binding must not be blank" }
        val json = JSONObject(value)
        val allowedKeys = setOf(
            "credentialPlane",
            "endpoint",
            "bearerCredential",
            "deviceId",
            "accountFence",
            "accountEpoch",
            "executors",
        )
        require(json.keys().asSequence().toSet() == allowedKeys) {
            "Remote binding must contain only the canonical executor credential contract"
        }
        require(json.getString("credentialPlane") == EXECUTOR_CREDENTIAL_PLANE) {
            "Remote binding credential plane is not an authorized Remote Runner enrollment"
        }
        val endpoint = URI(json.getString("endpoint"))
        val loopback = endpoint.host == "127.0.0.1" || endpoint.host == "::1" || endpoint.host == "localhost"
        require(endpoint.scheme.equals("https", true) || (endpoint.scheme.equals("http", true) && loopback)) {
            "Remote binding endpoint must use HTTPS outside loopback"
        }
        require(endpoint.userInfo == null && endpoint.fragment == null) { "Remote binding endpoint is invalid" }
        val credential = json.getString("bearerCredential")
        require(
            credential.length in 16..(16 * 1024) &&
                credential.none(Char::isWhitespace) &&
                credential.none(Char::isISOControl),
        ) {
            "Remote binding credential is invalid"
        }
        for (key in listOf("deviceId", "accountFence")) {
            val identity = json.getString(key)
            require(identity.isNotBlank() && identity.length <= 512 && identity.none(Char::isISOControl)) {
                "Remote binding $key is invalid"
            }
        }
        require(json.getLong("accountEpoch") > 0L) { "Remote binding account epoch must be positive" }
        val allowedExecutors = setOf(
            "shell",
            "read",
            "computer",
            "screenshot",
            "external-shell",
            "external-read",
        )
        val executors = json.getJSONArray("executors")
        require(executors.length() in 1..allowedExecutors.size) {
            "Remote binding must declare bounded executors"
        }
        val observedExecutors = mutableSetOf<String>()
        repeat(executors.length()) { index ->
            val executor = executors.getString(index)
            require(executor in allowedExecutors) { "Remote binding executor is unsupported" }
            require(observedExecutors.add(executor)) { "Remote binding executor is duplicated" }
        }
    }
}

/**
 * Pure fence comparison used by the protected binding owner and JVM tests.
 * It intentionally returns only a boolean and never exposes the bearer credential.
 */
internal object RemoteBindingFencePolicy {
    fun matches(bindingJson: String, currentAccountFence: String): Boolean {
        if (currentAccountFence.isBlank() || currentAccountFence.length > 512) return false
        if (currentAccountFence.any(Char::isISOControl)) return false
        return runCatching {
            val binding = JSONObject(bindingJson)
            binding.getString("accountFence") == currentAccountFence
        }.getOrDefault(false)
    }
}
