package com.ombhrum.fabushi

import android.net.Uri
import com.ombhrum.fabushi.androidpreload.runtime.ANDROID_AGENT_AVATAR_SOURCE_MAX_BYTES
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort

internal const val AGENT_AVATAR_SOURCE_MAX_BYTES: Int = ANDROID_AGENT_AVATAR_SOURCE_MAX_BYTES

internal fun importAgentAvatarImageDataUrl(
    coordinator: AndroidCoordinatorPort,
    uri: Uri,
): Result<String> = runCatching {
    coordinator.stagePickedAgentAvatarImage(uri.toString()).dataUrl
}

internal fun stageAgentAvatarImage(
    coordinator: AndroidCoordinatorPort,
    uri: Uri,
): Result<AndroidAvatarImageSource> = runCatching {
    coordinator.stagePickedAgentAvatarImage(uri.toString())
}

internal fun encodeAgentAvatarCrop(
    coordinator: AndroidCoordinatorPort,
    source: AndroidAvatarImageSource,
    crop: AndroidAvatarCrop,
): Result<String> = runCatching {
    coordinator.encodeAgentAvatarCrop(source, crop)
}

internal fun stageGeneratedAgentAvatarImage(
    coordinator: AndroidCoordinatorPort,
    imageData: String,
    mimeType: String,
): Result<AndroidAvatarImageSource> = runCatching {
    coordinator.stageGeneratedAgentAvatarImage(imageData, mimeType)
}
