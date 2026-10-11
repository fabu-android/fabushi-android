package com.ombhrum.fabushi

import android.os.Handler
import android.os.Looper

internal fun interface StrictModeDisposable {
    fun dispose()
}

internal class StrictModeDisposalGuard(
    private val defer: (() -> Unit) -> Unit = { action ->
        Handler(Looper.getMainLooper()).post(action)
    },
) {
    private var current: StrictModeDisposable? = null
    private var generation = 0L

    fun attach(resource: StrictModeDisposable?): () -> Unit {
        val previous = current
        if (previous != null && previous !== resource) previous.dispose()
        current = resource
        generation += 1
        val attachedGeneration = generation
        return {
            defer {
                if (generation == attachedGeneration && current === resource) {
                    resource?.dispose()
                    current = null
                }
            }
        }
    }
}
