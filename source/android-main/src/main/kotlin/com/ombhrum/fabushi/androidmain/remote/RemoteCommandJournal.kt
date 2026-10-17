package com.ombhrum.fabushi

import java.io.File
import java.util.Properties

internal enum class RemoteCommandState { PENDING, COMPLETED, FAILED, OUTCOME_UNKNOWN }

internal data class RemoteCommandRecord(
    val requestId: String,
    val deviceId: String,
    val sessionId: String,
    val toolName: String,
    val state: RemoteCommandState,
    val responseJson: String?,
    val error: String?,
)

internal sealed interface RemoteCommandAdmission {
    data class Execute(val requestId: String) : RemoteCommandAdmission
    data class Replay(val record: RemoteCommandRecord) : RemoteCommandAdmission
}

internal class RemoteCommandJournal(private val file: File) {
    private val lock = Any()
    private val records = linkedMapOf<String, RemoteCommandRecord>()

    init {
        load()
        var changed = false
        records.entries.toList().forEach { (id, record) ->
            if (record.state == RemoteCommandState.PENDING) {
                records[id] = record.copy(
                    state = RemoteCommandState.OUTCOME_UNKNOWN,
                    error = "process_restarted_with_remote_command_in_flight",
                )
                changed = true
            }
        }
        if (changed) persist()
    }

    fun begin(
        requestId: String,
        deviceId: String,
        sessionId: String,
        toolName: String,
    ): RemoteCommandAdmission = synchronized(lock) {
        val existing = records[requestId]
        if (existing != null) {
            require(existing.deviceId == deviceId && existing.sessionId == sessionId) {
                "remote_command_identity_session_mismatch"
            }
            require(existing.toolName == toolName) { "remote_command_identity_tool_mismatch" }
            return@synchronized when (existing.state) {
                RemoteCommandState.COMPLETED,
                RemoteCommandState.FAILED,
                -> RemoteCommandAdmission.Replay(existing)
                RemoteCommandState.PENDING -> error("remote_command_already_pending")
                RemoteCommandState.OUTCOME_UNKNOWN -> error("remote_command_outcome_unknown_reconcile_required")
            }
        }
        records[requestId] = RemoteCommandRecord(
            requestId = requestId,
            deviceId = deviceId,
            sessionId = sessionId,
            toolName = toolName,
            state = RemoteCommandState.PENDING,
            responseJson = null,
            error = null,
        )
        persist()
        RemoteCommandAdmission.Execute(requestId)
    }

    fun complete(requestId: String, responseJson: String) =
        settle(requestId, RemoteCommandState.COMPLETED, responseJson, null)

    fun fail(requestId: String, error: String) =
        settle(requestId, RemoteCommandState.FAILED, null, error)

    fun markPendingOutcomeUnknown(reason: String) = synchronized(lock) {
        var changed = false
        records.entries.toList().forEach { (id, record) ->
            if (record.state == RemoteCommandState.PENDING) {
                records[id] = record.copy(
                    state = RemoteCommandState.OUTCOME_UNKNOWN,
                    error = reason,
                )
                changed = true
            }
        }
        if (changed) persist()
    }

    fun get(requestId: String): RemoteCommandRecord? = synchronized(lock) { records[requestId] }

    private fun settle(
        requestId: String,
        state: RemoteCommandState,
        responseJson: String?,
        error: String?,
    ) = synchronized(lock) {
        val current = records[requestId] ?: error("remote_command_missing")
        require(current.state == RemoteCommandState.PENDING) { "remote_command_already_terminal" }
        records[requestId] = current.copy(
            state = state,
            responseJson = responseJson,
            error = error,
        )
        persist()
    }

    private fun load() = synchronized(lock) {
        if (!file.exists()) return@synchronized
        val properties = Properties()
        file.inputStream().use(properties::load)
        properties.stringPropertyNames()
            .filter { it.endsWith(".state") }
            .map { it.removeSuffix(".state") }
            .forEach { key ->
                val requestId = properties.getProperty(key + ".requestId") ?: return@forEach
                val state = runCatching {
                    RemoteCommandState.valueOf(properties.getProperty(key + ".state"))
                }.getOrNull() ?: return@forEach
                records[requestId] = RemoteCommandRecord(
                    requestId = requestId,
                    deviceId = properties.getProperty(key + ".deviceId").orEmpty(),
                    sessionId = properties.getProperty(key + ".sessionId").orEmpty(),
                    toolName = properties.getProperty(key + ".toolName").orEmpty(),
                    state = state,
                    responseJson = properties.getProperty(key + ".responseJson"),
                    error = properties.getProperty(key + ".error"),
                )
            }
    }

    private fun persist() {
        file.parentFile?.mkdirs()
        val properties = Properties()
        records.values.forEachIndexed { index, record ->
            val key = "record." + index
            properties.setProperty(key + ".requestId", record.requestId)
            properties.setProperty(key + ".deviceId", record.deviceId)
            properties.setProperty(key + ".sessionId", record.sessionId)
            properties.setProperty(key + ".toolName", record.toolName)
            properties.setProperty(key + ".state", record.state.name)
            record.responseJson?.let { properties.setProperty(key + ".responseJson", it) }
            record.error?.let { properties.setProperty(key + ".error", it) }
        }
        val temp = File(file.parentFile, file.name + ".tmp")
        temp.outputStream().use { properties.store(it, "Fabushi remote command journal") }
        if (!temp.renameTo(file)) {
            temp.copyTo(file, overwrite = true)
            temp.delete()
        }
    }
}
