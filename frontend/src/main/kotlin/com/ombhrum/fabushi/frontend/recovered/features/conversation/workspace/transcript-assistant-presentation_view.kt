package com.ombhrum.fabushi

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.URI
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

private const val MAX_TRANSCRIPT_REMOTE_IMAGE_BYTES = 8 * 1024 * 1024
private const val MAX_TRANSCRIPT_REMOTE_IMAGE_EDGE = 8_192
private const val MAX_TRANSCRIPT_REMOTE_IMAGE_PIXELS = 32_000_000L

private val transcriptRemoteImageClient: OkHttpClient by lazy {
    OkHttpClient.Builder()
        .followRedirects(true)
        .followSslRedirects(true)
        .build()
}

private fun readBoundedTranscriptImageBytes(response: Response): ByteArray? {
    if (!response.isSuccessful) return null
    val body = response.body
    val mediaType = body.contentType()
    if (mediaType != null && !mediaType.type.equals("image", ignoreCase = true)) return null
    val declaredLength = body.contentLength()
    if (declaredLength > MAX_TRANSCRIPT_REMOTE_IMAGE_BYTES) return null

    val output = ByteArrayOutputStream(
        declaredLength
            .takeIf { it in 1..MAX_TRANSCRIPT_REMOTE_IMAGE_BYTES.toLong() }
            ?.toInt()
            ?: 16 * 1024,
    )
    body.byteStream().use { input ->
        val buffer = ByteArray(16 * 1024)
        var total = 0
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            total += count
            if (total > MAX_TRANSCRIPT_REMOTE_IMAGE_BYTES) return null
            output.write(buffer, 0, count)
        }
    }
    return output.toByteArray()
}

private fun decodeBoundedTranscriptBitmap(bytes: ByteArray): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    val width = bounds.outWidth
    val height = bounds.outHeight
    if (
        width <= 0 ||
        height <= 0 ||
        width > MAX_TRANSCRIPT_REMOTE_IMAGE_EDGE ||
        height > MAX_TRANSCRIPT_REMOTE_IMAGE_EDGE ||
        width.toLong() * height.toLong() > MAX_TRANSCRIPT_REMOTE_IMAGE_PIXELS
    ) {
        return null
    }
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
}

private suspend fun loadTrustedTranscriptImage(url: String): Bitmap? =
    suspendCancellableCoroutine { continuation ->
        val request = runCatching { Request.Builder().url(url).get().build() }.getOrNull()
        if (request == null) {
            continuation.resume(null)
            return@suspendCancellableCoroutine
        }
        val call = transcriptRemoteImageClient.newCall(request)
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (continuation.isActive) continuation.resume(null)
                }

                override fun onResponse(call: Call, response: Response) {
                    val bitmap = response.use {
                        val bytes = readBoundedTranscriptImageBytes(it) ?: return@use null
                        decodeBoundedTranscriptBitmap(bytes)
                    }
                    if (continuation.isActive) continuation.resume(bitmap)
                }
            },
        )
    }

@Composable
private fun MobileTrustedTranscriptImage(
    image: MobileTranscriptImage,
    sourceTrusted: Boolean,
) {
    val normalized = remember(image.url, sourceTrusted) {
        if (sourceTrusted) normalizeTranscriptRemoteImageUrl(image.url) else null
    }
    var bitmap by remember(normalized) { mutableStateOf<Bitmap?>(null) }
    var settled by remember(normalized) { mutableStateOf(normalized == null) }

    LaunchedEffect(normalized) {
        bitmap = null
        settled = normalized == null
        if (normalized != null) {
            bitmap = loadTrustedTranscriptImage(normalized)
            settled = true
        }
    }

    val description = image.alt?.takeIf(String::isNotBlank) ?: "Assistant image"
    if (bitmap != null) {
        Image(
            bitmap = bitmap!!.asImageBitmap(),
            contentDescription = description,
            contentScale = ContentScale.Fit,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 320.dp)
                .clip(RoundedCornerShape(10.dp)),
        )
    } else {
        val fallback = when {
            !sourceTrusted -> "$description · source not trusted"
            normalized == null -> "$description · unsupported image URL"
            !settled -> "$description · loading"
            else -> "$description · unavailable"
        }
        Text(
            fallback,
            color = GrokMobileMuted,
            fontSize = 12.sp,
            modifier = Modifier
                .fillMaxWidth()
                .background(Color.Black.copy(alpha = 0.035f), RoundedCornerShape(8.dp))
                .semantics { contentDescription = fallback }
                .padding(horizontal = 10.dp, vertical = 8.dp),
        )
    }
}

@Composable
internal fun MobileAssistantTranscriptImages(
    images: List<MobileTranscriptImage>,
    sourceTrusted: Boolean,
) {
    if (images.isEmpty()) return
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        images.forEach { image ->
            MobileTrustedTranscriptImage(image = image, sourceTrusted = sourceTrusted)
        }
    }
}

@Composable
internal fun MobileAssistantTranscriptChannel(channel: String?) {
    val value = channel?.takeIf(String::isNotBlank) ?: return
    Text(
        value,
        color = GrokMobileMuted,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        modifier = Modifier
            .background(Color.Black.copy(alpha = 0.045f), RoundedCornerShape(999.dp))
            .semantics { contentDescription = "Sent to $value" }
            .padding(horizontal = 8.dp, vertical = 4.dp),
    )
}

@Composable
internal fun MobileTranscriptUrlCard(url: String) {
    val normalized = remember(url) { normalizeTranscriptHttpUrl(url) } ?: return
    val handler = LocalUriHandler.current
    val host = remember(normalized) {
        runCatching { URI(normalized).host }.getOrNull()?.takeIf(String::isNotBlank) ?: normalized
    }
    TextButton(
        onClick = { runCatching { handler.openUri(normalized) } },
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .semantics { contentDescription = "Open link $host" },
    ) {
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Link", color = GrokMobileMuted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
            Text(host, color = GrokMobileInk, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            if (host != normalized) {
                Text(normalized, color = GrokMobileMuted, fontSize = 11.sp)
            }
        }
    }
}
