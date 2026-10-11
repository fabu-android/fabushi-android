package com.ombhrum.fabushi

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TerminalOutputProjectionTest {
    @Test
    fun normalizesLegacyFieldVariantsAndStatusPriority() {
        assertEquals("a\nb\nc", normalizeTerminalOutputAndroid("a\r\nb\rc"))
        assertNull(projectTerminalOutputAndroid(null))
        assertNull(projectTerminalOutputAndroid(JSONObject().put("sessionId", "s")))

        assertEquals(
            TerminalOutputSnapshot(
                sessionId = "42",
                command = "npm test",
                cwd = "/repo",
                output = "one\ntwo",
                status = TerminalOutputStatus.EXITED,
                exitCode = 0.0,
            ),
            projectTerminalOutputAndroid(
                JSONObject()
                    .put("terminal_instance_id", 42)
                    .put(
                        "metadata",
                        JSONObject()
                            .put("cwd", "/repo")
                            .put("currentCommand", JSONObject().put("command", "npm test")),
                    )
                    .put("output_raw", "one\r\ntwo")
                    .put("exit_code", 0),
            ),
        )

        assertEquals(
            TerminalOutputStatus.RUNNING,
            projectTerminalOutputAndroid(
                JSONObject()
                    .put("sessionId", "s")
                    .put("command", "build")
                    .put("status", "error")
                    .put("isRunning", true)
                    .put("exitCode", 3),
            )!!.status,
        )
        assertEquals(
            TerminalOutputStatus.ERROR,
            projectTerminalOutputAndroid(
                mapOf(
                    "terminalInstancePath" to "pty-1",
                    "command" to "false",
                    "contents" to "failed",
                    "exitCode" to 1,
                ),
            )!!.status,
        )
    }
}
