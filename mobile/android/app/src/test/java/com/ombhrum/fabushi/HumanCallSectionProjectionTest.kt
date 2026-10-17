package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallSession
import com.ombhrum.fabushi.androidpreload.runtime.AndroidHumanCallState
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class HumanCallSectionProjectionTest {
    private fun call(
        id: String,
        scopeId: String,
        updatedAtMs: Long,
        state: AndroidHumanCallState = AndroidHumanCallState.CONNECTED,
    ) = AndroidHumanCallSession(
        id = id,
        requestId = "request:$id:$updatedAtMs",
        scopeId = scopeId,
        creatorId = "user:creator",
        participantIds = listOf("user:creator", "user:peer"),
        state = state,
        generation = 0L,
        signalSeq = 0L,
        mediaCapabilities = JSONObject(),
        deviceSelection = JSONObject(),
        createdAtMs = updatedAtMs - 1L,
        updatedAtMs = updatedAtMs,
        terminalReason = null,
    )

    @Test
    fun callHistoryKeepsNewestDuplicateAndOrdersNewestFirst() {
        val stale = call("call:shared", "conversation:a", updatedAtMs = 100L)
        val newest = call(
            "call:shared",
            "conversation:a",
            updatedAtMs = 300L,
            state = AndroidHumanCallState.OUTCOME_UNKNOWN,
        )
        val middle = call("call:middle", "conversation:b", updatedAtMs = 200L)

        val projected = humanCallSectionCalls(
            MessagingUiState(
                callsByConversation = linkedMapOf(
                    "conversation:a" to listOf(stale, newest),
                    "conversation:b" to listOf(middle),
                ),
            ),
        )

        assertEquals(listOf("call:shared", "call:middle"), projected.map { it.id })
        assertEquals(300L, projected.first().updatedAtMs)
        assertEquals(AndroidHumanCallState.OUTCOME_UNKNOWN, projected.first().state)
    }
}
