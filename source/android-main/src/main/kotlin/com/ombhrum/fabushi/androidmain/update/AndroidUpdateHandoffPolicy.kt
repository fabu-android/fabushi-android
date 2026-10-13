package com.ombhrum.fabushi

internal enum class AndroidUpdateHandoffPreparation {
    READY,
    HOST_QUIESCE_FAILED,
    MARKER_PERSIST_FAILED,
    MARKER_PERSIST_FAILED_RESUME_FAILED,
}

/**
 * Establishes the only safe precondition for handing a verified package to Android's installer:
 * the Host must reject new Agent turns and the durable REQUESTED marker must already be committed.
 *
 * If durable marker persistence fails, the installer is never launched and the Host is resumed
 * immediately. A failed resume is surfaced separately so callers can require process restart
 * instead of silently leaving the current Host permanently quiesced.
 */
internal fun prepareAndroidUpdateInstallerHandoff(
    setQuiescing: (Boolean) -> Boolean,
    persistRequested: () -> Unit,
): AndroidUpdateHandoffPreparation {
    if (!runCatching { setQuiescing(true) }.getOrDefault(false)) {
        return AndroidUpdateHandoffPreparation.HOST_QUIESCE_FAILED
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
