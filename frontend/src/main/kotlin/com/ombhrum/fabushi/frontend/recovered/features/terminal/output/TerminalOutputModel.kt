package com.ombhrum.fabushi

import org.json.JSONObject

internal enum class TerminalOutputStatus {
    IDLE,
    RUNNING,
    EXITED,
    ERROR,
}

internal data class TerminalOutputSnapshot(
    val sessionId: String,
    val command: String,
    val cwd: String?,
    val output: String,
    val status: TerminalOutputStatus,
    val exitCode: Double?,
)

private fun terminalField(value: Any?, key: String): Any? =
    when (value) {
        is JSONObject -> value.opt(key).takeUnless { it === JSONObject.NULL }
        is Map<*, *> -> value[key]
        else -> null
    }

private fun terminalRecord(value: Any?): Any? =
    when (value) {
        is JSONObject -> value
        is Map<*, *> -> value
        else -> null
    }

private fun terminalString(value: Any?): String? =
    (value as? String)?.takeIf(String::isNotEmpty)

private fun terminalNumber(value: Any?): Double? =
    (value as? Number)?.toDouble()?.takeIf(Double::isFinite)

internal fun normalizeTerminalOutputAndroid(value: Any?): String =
    (value as? String)
        ?.replace("\r\n", "\n")
        ?.replace("\r", "\n")
        .orEmpty()

internal fun projectTerminalOutputAndroid(value: Any?): TerminalOutputSnapshot? {
    val root = terminalRecord(value) ?: return null
    val metadata = terminalRecord(terminalField(root, "metadata"))
    val currentCommand =
        terminalRecord(terminalField(root, "currentCommand"))
            ?: terminalRecord(terminalField(metadata, "currentCommand"))

    val command =
        terminalString(terminalField(root, "command"))
            ?: terminalString(terminalField(currentCommand, "command"))

    val numericId =
        terminalNumber(terminalField(root, "terminalInstanceId"))
            ?: terminalNumber(terminalField(root, "terminal_instance_id"))
    val sessionId =
        terminalString(terminalField(root, "sessionId"))
            ?: numericId?.let { number ->
                if (number % 1.0 == 0.0) number.toLong().toString() else number.toString()
            }
            ?: terminalString(terminalField(root, "terminalInstancePath"))
            ?: terminalString(terminalField(root, "terminal_instance_path"))

    if (command == null || sessionId == null) return null

    val exitCode =
        terminalNumber(
            terminalField(root, "exitCode") ?: terminalField(root, "exit_code"),
        )
    val running =
        terminalField(root, "status") == "running" ||
            terminalField(root, "isRunning") == true ||
            terminalField(root, "isRunningInBackground") == true
    val failed =
        terminalField(root, "status") == "error" ||
            terminalField(root, "rejected") == true ||
            (exitCode != null && exitCode != 0.0)
    val status =
        when {
            running -> TerminalOutputStatus.RUNNING
            failed -> TerminalOutputStatus.ERROR
            terminalField(root, "status") == "idle" -> TerminalOutputStatus.IDLE
            else -> TerminalOutputStatus.EXITED
        }

    return TerminalOutputSnapshot(
        sessionId = sessionId,
        command = command,
        cwd =
            terminalString(terminalField(root, "cwd"))
                ?: terminalString(terminalField(root, "cwdFull"))
                ?: terminalString(terminalField(metadata, "cwd")),
        output = normalizeTerminalOutputAndroid(
            terminalField(root, "outputRaw")
                ?: terminalField(root, "output_raw")
                ?: terminalField(root, "output")
                ?: terminalField(root, "contents"),
        ),
        status = status,
        exitCode = exitCode,
    )
}

internal fun projectTerminalOutputCardAndroid(card: MobileToolResultCard): TerminalOutputSnapshot? {
    if (card.kind != "shell" || card.toolCallId.isBlank() || card.command.isNullOrBlank()) return null
    val running =
        card.isStreaming ||
            card.status.equals("running", ignoreCase = true) ||
            card.status.equals("background", ignoreCase = true)
    return projectTerminalOutputAndroid(
        mapOf(
            "sessionId" to card.toolCallId,
            "command" to card.command,
            "cwd" to card.workingDirectory,
            "output" to card.output.ifBlank { card.summary },
            "status" to card.status.lowercase(),
            "isRunning" to running,
        ),
    )
}
