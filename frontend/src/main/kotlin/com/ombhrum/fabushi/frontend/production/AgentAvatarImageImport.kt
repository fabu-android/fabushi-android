package com.ombhrum.fabushi

import android.content.Context
import android.net.Uri
import com.ombhrum.fabushi.androidmain.media.AndroidAvatarImageProcessor
import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_SOURCE_BYTES

internal const val AGENT_AVATAR_SOURCE_MAX_BYTES: Int = MAX_AGENT_AVATAR_SOURCE_BYTES

internal fun importAgentAvatarImageDataUrl(
    context: Context,
    uri: Uri,
): Result<String> = runCatching {
    AndroidAvatarImageProcessor.normalizePickedImage(context, uri.toString())
}
