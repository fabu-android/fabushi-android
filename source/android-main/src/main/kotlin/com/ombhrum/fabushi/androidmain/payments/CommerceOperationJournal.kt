package com.ombhrum.fabushi

import java.io.File
import java.util.Properties
import java.util.UUID

internal enum class CommerceOperationState { PENDING, CONFIRMED }

internal data class CommerceOperationRecord(
    val accountFence: String,
    val pluginId: String,
    val sku: String,
    val idempotencyKey: String,
    val state: CommerceOperationState,
)

internal class CommerceOperationJournal(private val file: File) {
    private val lock = Any()
    private val records = linkedMapOf<String, CommerceOperationRecord>()

    init {
        load()
    }

    fun stableIdempotencyKey(accountFence: String, pluginId: String, sku: String): String =
        synchronized(lock) {
            require(accountFence.isNotBlank()) { "commerce_account_fence_required" }
            require(pluginId.isNotBlank()) { "commerce_plugin_id_required" }
            require(sku.isNotBlank()) { "commerce_sku_required" }
            val scope = scopeKey(accountFence, pluginId, sku)
            val existing = records[scope]
            if (existing != null) return@synchronized existing.idempotencyKey

            val key = "android-commerce-" + UUID.randomUUID().toString()
            records[scope] = CommerceOperationRecord(
                accountFence = accountFence,
                pluginId = pluginId,
                sku = sku,
                idempotencyKey = key,
                state = CommerceOperationState.PENDING,
            )
            persist()
            key
        }

    fun markConfirmed(
        accountFence: String,
        pluginId: String,
        sku: String,
        idempotencyKey: String,
    ) = synchronized(lock) {
        val scope = scopeKey(accountFence, pluginId, sku)
        val existing = records[scope] ?: error("commerce_operation_missing")
        require(existing.idempotencyKey == idempotencyKey) { "commerce_idempotency_key_mismatch" }
        records[scope] = existing.copy(state = CommerceOperationState.CONFIRMED)
        persist()
    }

    fun record(accountFence: String, pluginId: String, sku: String): CommerceOperationRecord? =
        synchronized(lock) { records[scopeKey(accountFence, pluginId, sku)] }

    private fun scopeKey(accountFence: String, pluginId: String, sku: String): String =
        accountFence + "\u001f" + pluginId + "\u001f" + sku

    private fun load() = synchronized(lock) {
        if (!file.exists()) return@synchronized
        val properties = Properties()
        file.inputStream().use(properties::load)
        properties.stringPropertyNames()
            .filter { it.endsWith(".accountFence") }
            .map { it.removeSuffix(".accountFence") }
            .forEach { key ->
                val accountFence = properties.getProperty(key + ".accountFence") ?: return@forEach
                val pluginId = properties.getProperty(key + ".pluginId") ?: return@forEach
                val sku = properties.getProperty(key + ".sku") ?: return@forEach
                val idempotencyKey = properties.getProperty(key + ".idempotencyKey") ?: return@forEach
                val state = runCatching {
                    CommerceOperationState.valueOf(properties.getProperty(key + ".state"))
                }.getOrDefault(CommerceOperationState.PENDING)
                records[scopeKey(accountFence, pluginId, sku)] = CommerceOperationRecord(
                    accountFence = accountFence,
                    pluginId = pluginId,
                    sku = sku,
                    idempotencyKey = idempotencyKey,
                    state = state,
                )
            }
    }

    private fun persist() {
        file.parentFile?.mkdirs()
        val properties = Properties()
        records.values.forEachIndexed { index, record ->
            val key = "record." + index
            properties.setProperty(key + ".accountFence", record.accountFence)
            properties.setProperty(key + ".pluginId", record.pluginId)
            properties.setProperty(key + ".sku", record.sku)
            properties.setProperty(key + ".idempotencyKey", record.idempotencyKey)
            properties.setProperty(key + ".state", record.state.name)
        }
        val temp = File(file.parentFile, file.name + ".tmp")
        temp.outputStream().use { properties.store(it, "Fabushi commerce operation journal") }
        if (!temp.renameTo(file)) {
            temp.copyTo(file, overwrite = true)
            temp.delete()
        }
    }
}
