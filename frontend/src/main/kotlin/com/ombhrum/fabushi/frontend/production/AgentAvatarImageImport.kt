package com.ombhrum.fabushi

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.Base64
import java.io.ByteArrayOutputStream

internal const val AGENT_AVATAR_SOURCE_MAX_BYTES: Int = 25 * 1024 * 1024
private const val AGENT_AVATAR_DECODE_MAX_DIMENSION = 4096
private const val AGENT_AVATAR_OUTPUT_MAX_DIMENSION = 1024

internal fun importAgentAvatarImageDataUrl(
    context: Context,
    uri: Uri,
): Result<String> = runCatching {
    val source = context.contentResolver.openInputStream(uri)?.use { input ->
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        var total = 0
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            total += count
            require(total <= AGENT_AVATAR_SOURCE_MAX_BYTES) {
                "Avatar image must be 25 MB or smaller"
            }
            output.write(buffer, 0, count)
        }
        output.toByteArray()
    } ?: error("Unable to open the selected avatar image")
    require(source.isNotEmpty()) { "Avatar image is empty" }

    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(source, 0, source.size, bounds)
    require(bounds.outWidth > 0 && bounds.outHeight > 0) { "Selected file is not a supported image" }

    var sampleSize = 1
    while (
        bounds.outWidth / sampleSize > AGENT_AVATAR_DECODE_MAX_DIMENSION ||
        bounds.outHeight / sampleSize > AGENT_AVATAR_DECODE_MAX_DIMENSION
    ) {
        sampleSize *= 2
    }
    val decoded = BitmapFactory.decodeByteArray(
        source,
        0,
        source.size,
        BitmapFactory.Options().apply { inSampleSize = sampleSize },
    ) ?: error("Selected image could not be decoded")

    val longest = maxOf(decoded.width, decoded.height)
    val outputBitmap = if (longest > AGENT_AVATAR_OUTPUT_MAX_DIMENSION) {
        val scale = AGENT_AVATAR_OUTPUT_MAX_DIMENSION.toFloat() / longest.toFloat()
        Bitmap.createScaledBitmap(
            decoded,
            maxOf(1, (decoded.width * scale).toInt()),
            maxOf(1, (decoded.height * scale).toInt()),
            true,
        )
    } else {
        decoded
    }

    try {
        val png = ByteArrayOutputStream()
        check(outputBitmap.compress(Bitmap.CompressFormat.PNG, 100, png)) {
            "Avatar image could not be encoded"
        }
        "data:image/png;base64," + Base64.encodeToString(png.toByteArray(), Base64.NO_WRAP)
    } finally {
        if (outputBitmap !== decoded) outputBitmap.recycle()
        decoded.recycle()
    }
}
