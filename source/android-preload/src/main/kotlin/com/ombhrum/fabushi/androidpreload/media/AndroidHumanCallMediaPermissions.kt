package com.ombhrum.fabushi.androidpreload.media

enum class AndroidCallMediaPermissionStatus {
    GRANTED,
    DENIED,
    PROMPT,
    NOT_REQUESTED,
}

data class AndroidHumanCallMediaPermissionProjection(
    val microphone: AndroidCallMediaPermissionStatus,
    val camera: AndroidCallMediaPermissionStatus,
)

internal fun projectAndroidHumanCallMediaPermissions(
    audioRequested: Boolean,
    videoRequested: Boolean,
    microphoneGranted: Boolean,
    cameraGranted: Boolean,
    afterSystemRequest: Boolean,
): AndroidHumanCallMediaPermissionProjection {
    fun status(requested: Boolean, granted: Boolean): AndroidCallMediaPermissionStatus =
        when {
            !requested -> AndroidCallMediaPermissionStatus.NOT_REQUESTED
            granted -> AndroidCallMediaPermissionStatus.GRANTED
            afterSystemRequest -> AndroidCallMediaPermissionStatus.DENIED
            else -> AndroidCallMediaPermissionStatus.PROMPT
        }
    return AndroidHumanCallMediaPermissionProjection(
        microphone = status(audioRequested, microphoneGranted),
        camera = status(videoRequested, cameraGranted),
    )
}
