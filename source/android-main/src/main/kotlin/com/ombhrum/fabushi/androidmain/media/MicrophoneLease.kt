package com.ombhrum.fabushi

import java.util.concurrent.atomic.AtomicReference

/** Single process-wide microphone owner shared by voice-message capture and offline ASR. */
internal object MicrophoneLease {
    private val owner = AtomicReference<String?>(null)

    fun acquire(ownerId: String): Boolean {
        require(ownerId.isNotBlank()) { "Microphone owner id is required" }
        return owner.compareAndSet(null, ownerId)
    }

    fun release(ownerId: String) {
        owner.compareAndSet(ownerId, null)
    }

    fun currentOwnerForTest(): String? = owner.get()
}
