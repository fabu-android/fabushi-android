package com.ombhrum.fabushi

internal data class RosterRefreshToken(
    val generation: Long,
)

internal class RosterRefreshFence {
    private var generation: Long = 0L

    fun begin(): RosterRefreshToken =
        RosterRefreshToken(generation = ++generation)

    fun invalidate() {
        generation += 1L
    }

    fun isCurrent(token: RosterRefreshToken): Boolean =
        token.generation == generation

    fun accepts(
        token: RosterRefreshToken,
        expectedAccountSlot: String?,
        canonicalAccountSlot: String?,
    ): Boolean =
        isCurrent(token) && expectedAccountSlot == canonicalAccountSlot
}
