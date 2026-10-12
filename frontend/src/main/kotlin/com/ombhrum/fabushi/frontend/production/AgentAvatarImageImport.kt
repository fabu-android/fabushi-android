package com.ombhrum.fabushi

import android.content.Context
import android.net.Uri
import com.ombhrum.fabushi.androidmain.media.AndroidAvatarCrop
import com.ombhrum.fabushi.androidmain.media.AndroidAvatarImageProcessor
import com.ombhrum.fabushi.androidmain.media.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_SOURCE_BYTES

internal const val AGENT_AVATAR_SOURCE_MAX_BYTES: Int = MAX_AGENT_AVATAR_SOURCE_BYTES

internal fun importAgentAvatarImageDataUrl(
    context: Context,
    uri: Uri,
): Result<String> = runCatching {
    AndroidAvatarImageProcessor.normalizePickedImage(context, uri.toString())
}


internal fun stageAgentAvatarImage(
    context: Context,
    uri: Uri,
): Result<AndroidAvatarImageSource> = runCatching {
    AndroidAvatarImageProcessor.stagePickedImage(context, uri.toString())
}

internal fun encodeAgentAvatarCrop(
    source: AndroidAvatarImageSource,
    crop: AndroidAvatarCrop,
): Result<String> = runCatching {
    AndroidAvatarImageProcessor.encodeCrop(source, crop)
}
