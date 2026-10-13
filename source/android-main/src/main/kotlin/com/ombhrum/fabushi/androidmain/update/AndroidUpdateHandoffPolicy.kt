package com.ombhrum.fabushi

internal enum class AndroidUpdateHandoffPreparation {
    READY,
    HOST_NOT_IDLE,
    HOST_NOT_IDLE_RESUME_FAILED,
    HOST_QUIESCE_FAILED,
    MARKER_PERSIST_FAILED,
    MARKER_PERSIST_FAILED_RESUME_FAILED,
}

/**
 * Establishes the only safe precondition for handing a verified package to Android's installer:
 * the Host must be idle, must reject new Agent/automation work, must still be idle after that
 * fence is installed, and the durable REQUESTED marker must already be committed.
 *
 * The second idle observation closes the race between the first idle check and quiesce. Any
 * failure after quiescing resumes the Host before returning whenever possible. A failed resume is
 * surfaced separately so callers can require process restart instead of silently leaving the
 * current Host permanently quiesced.
 */
internal fun prepareAndroidUpdateInstallerHandoff(
    isHostIdle: () -> Boolean,
    setQuiescing: (Boolean) -> Boolean,
    persistRequested: () -> Unit,
): AndroidUpdateHandoffPreparation {
    if (!runCatching { isHostIdle() }.getOrDefault(false)) {
        return AndroidUpdateHandoffPreparation.HOST_NOT_IDLE
    }
    if (!runCatching { setQuiescing(true) }.getOrDefault(false)) {
        return AndroidUpdateHandoffPreparation.HOST_QUIESCE_FAILED
    }
    if (!runCatching { isHostIdle() }.getOrDefault(false)) {
        return if (runCatching { setQuiescing(false) }.getOrDefault(false)) {
            AndroidUpdateHandoffPreparation.HOST_NOT_IDLE
        } else {
            AndroidUpdateHandoffPreparation.HOST_NOT_IDLE_RESUME_FAILED
        }
    }
    return try {
        persistRequested()
        AndroidUpdateHandoffPreparation.READY
    } catch (_: Exception) {
        if (runCatching { setQuiescing(false) }.getOrDefault(false)) {
            AndroidUpdateHandoffPreparation.MARKER_PERSIST_FAILED
        } else {
            AndroidUpdateHandoffPreparation.MARKER_PERSIST_FAILED_RESUME_FAILED
        }
    }
}
