package com.ombhrum.fabushi.androidpreload
object PasskeyStall {
    const val DEFAULT_STALL_MS: Long = 30_000L
    fun isStalled(startedAtMs: Long, nowMs: Long, deadlineMs: Long = DEFAULT_STALL_MS): Boolean =
        nowMs >= startedAtMs && nowMs - startedAtMs >= deadlineMs
}
