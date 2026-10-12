package com.ombhrum.fabushi.androidmain.media

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.Base64
import java.io.ByteArrayOutputStream
import java.io.IOException
import kotlin.math.max
import kotlin.math.roundToInt

internal const val MAX_AGENT_AVATAR_SOURCE_BYTES = 25 * 1024 * 1024
internal const val MAX_AGENT_AVATAR_SOURCE_DIMENSION = 4096
internal const val MAX_AGENT_AVATAR_OUTPUT_DIMENSION = 1024
internal const val MAX_AGENT_AVATAR_OUTPUT_BYTES = 4_000_000

internal fun avatarDimensionsAllowed(width: Int, height: Int): Boolean =
    width in 1..MAX_AGENT_AVATAR_SOURCE_DIMENSION &&
        height in 1..MAX_AGENT_AVATAR_SOURCE_DIMENSION

internal fun avatarDecodeSampleSize(width: Int, height: Int): Int {
    var sample = 1
    while (max(width / sample, height / sample) > MAX_AGENT_AVATAR_OUTPUT_DIMENSION) {
        sample *= 2
    }
    return sample
}

internal fun avatarPngDataUrl(bytes: ByteArray): String =
    "data:image/png;base64," + Base64.encodeToString(bytes, Base64.NO_WRAP)

internal object AndroidAvatarImageProcessor {
    fun normalizePickedImage(context: Context, rawUri: String): String {
        val uri = Uri.parse(rawUri)
        require(uri.scheme == "content") { "Avatar image must come from the Android content picker" }
        val resolver = context.contentResolver
        val mime = resolver.getType(uri)?.lowercase().orEmpty()
        require(mime.startsWith("image/")) { "Selected file is not an image" }

        resolver.openAssetFileDescriptor(uri, "r")?.use { descriptor ->
            val length = descriptor.length
            require(length < 0 || length <= MAX_AGENT_AVATAR_SOURCE_BYTES) {
                "Avatar image exceeds the 25 MB source limit"
            }
        }

        val bytes = resolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            var total = 0
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                total += read
                require(total <= MAX_AGENT_AVATAR_SOURCE_BYTES) {
                    "Avatar image exceeds the 25 MB source limit"
                }
                output.write(buffer, 0, read)
            }
            output.toByteArray()
        } ?: throw IOException("Unable to read selected avatar image")
        require(bytes.isNotEmpty()) { "Selected avatar image is empty" }

        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(avatarDimensionsAllowed(bounds.outWidth, bounds.outHeight)) {
            "Avatar image dimensions exceed " + MAX_AGENT_AVATAR_SOURCE_DIMENSION + "px"
        }

        val options = BitmapFactory.Options().apply {
            inSampleSize = avatarDecodeSampleSize(bounds.outWidth, bounds.outHeight)
        }
        val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            ?: throw IOException("Android could not decode the selected avatar image")
        val normalized = decoded.scaleWithin(MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
        if (normalized !== decoded) decoded.recycle()

        return try {
            var current = normalized
            while (true) {
                val encoded = ByteArrayOutputStream().also { stream ->
                    check(current.compress(Bitmap.CompressFormat.PNG, 100, stream)) {
                        "Android could not encode the selected avatar image"
                    }
                }.toByteArray()
                if (encoded.size <= MAX_AGENT_AVATAR_OUTPUT_BYTES) {
                    return avatarPngDataUrl(encoded)
                }
                val nextWidth = (current.width * 0.85f).roundToInt().coerceAtLeast(1)
                val nextHeight = (current.height * 0.85f).roundToInt().coerceAtLeast(1)
                check(nextWidth < current.width || nextHeight < current.height) {
                    "Normalized avatar image exceeds the durable avatar limit"
                }
                val next = Bitmap.createScaledBitmap(current, nextWidth, nextHeight, true)
                if (current !== normalized) current.recycle()
                current = next
            }
        } finally {
            normalized.recycle()
        }
    }

    private fun Bitmap.scaleWithin(maxDimension: Int): Bitmap {
        val largest = max(width, height)
        if (largest <= maxDimension) return this
        val scale = maxDimension.toFloat() / largest.toFloat()
        return Bitmap.createScaledBitmap(
            this,
            (width * scale).roundToInt().coerceAtLeast(1),
            (height * scale).roundToInt().coerceAtLeast(1),
            true,
        )
    }
}
