package com.ombhrum.fabushi.androidmain.media

import com.ombhrum.fabushi.androidpreload.runtime.ANDROID_AGENT_AVATAR_MAX_ZOOM
import com.ombhrum.fabushi.androidpreload.runtime.ANDROID_AGENT_AVATAR_MIN_ZOOM
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidpreload.runtime.clampAndroidAvatarCrop

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
internal const val AGENT_AVATAR_CROP_OUTPUT_SIZE = 256
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
    fun stagePickedImage(context: Context, rawUri: String): AndroidAvatarImageSource {
        val dataUrl = normalizePickedImage(context, rawUri)
        val separator = dataUrl.indexOf(',')
        check(separator > 0 && separator < dataUrl.lastIndex) { "Normalized avatar image is invalid" }
        val bytes = Base64.decode(dataUrl.substring(separator + 1), Base64.DEFAULT)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        check(bounds.outWidth > 0 && bounds.outHeight > 0) { "Normalized avatar image is invalid" }
        return AndroidAvatarImageSource(dataUrl, bounds.outWidth, bounds.outHeight)
    }

    fun stageGeneratedImage(imageData: String, mimeType: String): AndroidAvatarImageSource {
        val mime = mimeType.trim().lowercase()
        require(mime.startsWith("image/")) { "Generated avatar has an invalid MIME type" }
        val bytes = try {
            Base64.decode(imageData.trim(), Base64.DEFAULT)
        } catch (_: IllegalArgumentException) {
            throw IOException("Generated avatar image is not valid base64")
        }
        require(bytes.isNotEmpty()) { "Generated avatar image is empty" }
        require(bytes.size <= MAX_AGENT_AVATAR_SOURCE_BYTES) {
            "Generated avatar image exceeds the 25 MB source limit"
        }

        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(avatarDimensionsAllowed(bounds.outWidth, bounds.outHeight)) {
            "Generated avatar image has invalid dimensions"
        }
        val options = BitmapFactory.Options().apply {
            inSampleSize = avatarDecodeSampleSize(bounds.outWidth, bounds.outHeight)
        }
        val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            ?: throw IOException("Android could not decode the generated avatar image")
        val normalized = decoded.scaleWithin(MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
        if (normalized !== decoded) decoded.recycle()

        var current = normalized
        try {
            while (true) {
                val encoded = ByteArrayOutputStream().also { stream ->
                    check(current.compress(Bitmap.CompressFormat.PNG, 100, stream)) {
                        "Android could not encode the generated avatar image"
                    }
                }.toByteArray()
                if (encoded.size <= MAX_AGENT_AVATAR_OUTPUT_BYTES) {
                    val dataUrl = avatarPngDataUrl(encoded)
                    return AndroidAvatarImageSource(dataUrl, current.width, current.height)
                }
                val nextWidth = (current.width * 0.85f).roundToInt().coerceAtLeast(1)
                val nextHeight = (current.height * 0.85f).roundToInt().coerceAtLeast(1)
                check(nextWidth < current.width || nextHeight < current.height) {
                    "Generated avatar image exceeds the durable avatar limit"
                }
                val previous = current
                current = Bitmap.createScaledBitmap(previous, nextWidth, nextHeight, true)
                if (previous !== normalized) previous.recycle()
            }
        } finally {
            if (current !== normalized) current.recycle()
            normalized.recycle()
        }
    }

    fun encodeCrop(source: AndroidAvatarImageSource, crop: AndroidAvatarCrop): String {
        val separator = source.dataUrl.indexOf(',')
        require(separator > 0 && separator < source.dataUrl.lastIndex) { "Avatar source is invalid" }
        val bytes = Base64.decode(source.dataUrl.substring(separator + 1), Base64.DEFAULT)
        val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            ?: throw IOException("Android could not decode the avatar crop source")
        try {
            val bounded = clampAndroidAvatarCrop(source, crop)
            val side = (minOf(source.width, source.height) / bounded.zoom)
                .roundToInt()
                .coerceIn(1, minOf(bitmap.width, bitmap.height))
            val left = (bounded.centerX - side / 2f)
                .roundToInt()
                .coerceIn(0, bitmap.width - side)
            val top = (bounded.centerY - side / 2f)
                .roundToInt()
                .coerceIn(0, bitmap.height - side)
            val cropped = Bitmap.createBitmap(bitmap, left, top, side, side)
            val output = if (
                cropped.width != AGENT_AVATAR_CROP_OUTPUT_SIZE ||
                cropped.height != AGENT_AVATAR_CROP_OUTPUT_SIZE
            ) {
                Bitmap.createScaledBitmap(
                    cropped,
                    AGENT_AVATAR_CROP_OUTPUT_SIZE,
                    AGENT_AVATAR_CROP_OUTPUT_SIZE,
                    true,
                )
            } else {
                cropped
            }
            try {
                val encoded = ByteArrayOutputStream().also { stream ->
                    check(output.compress(Bitmap.CompressFormat.PNG, 100, stream)) {
                        "Android could not encode the avatar crop"
                    }
                }.toByteArray()
                check(encoded.isNotEmpty() && encoded.size <= MAX_AGENT_AVATAR_OUTPUT_BYTES) {
                    "Cropped avatar exceeds the durable avatar limit"
                }
                return avatarPngDataUrl(encoded)
            } finally {
                if (output !== cropped) output.recycle()
                if (cropped !== bitmap) cropped.recycle()
            }
        } finally {
            bitmap.recycle()
        }
    }

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
            "Selected avatar image has invalid dimensions"
        }

        val options = BitmapFactory.Options().apply {
            inSampleSize = avatarDecodeSampleSize(bounds.outWidth, bounds.outHeight)
        }
        val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            ?: throw IOException("Android could not decode the selected avatar image")
        val normalized = decoded.scaleWithin(MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
        if (normalized !== decoded) decoded.recycle()

        var current = normalized
        try {
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
                val previous = current
                current = Bitmap.createScaledBitmap(previous, nextWidth, nextHeight, true)
                if (previous !== normalized) previous.recycle()
            }
        } finally {
            if (current !== normalized) current.recycle()
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
