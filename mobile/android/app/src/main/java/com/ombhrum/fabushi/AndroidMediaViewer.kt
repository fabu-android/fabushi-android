package com.ombhrum.fabushi

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.widget.MediaController
import android.widget.VideoView
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import com.ombhrum.fabushi.telegram.TelegramGlyph
import com.ombhrum.fabushi.telegram.TelegramIcon
import java.io.File

@Composable
internal fun AndroidMediaViewer(
    message: ChatMessage,
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    BackHandler(onBack = onClose)
    val context = LocalContext.current
    var bytes by remember(message.id) { mutableStateOf<ByteArray?>(null) }
    var file by remember(message.id) { mutableStateOf<File?>(null) }
    var error by remember(message.id) { mutableStateOf<String?>(null) }
    var loading by remember(message.id) { mutableStateOf(true) }

    LaunchedEffect(message.id) {
        val blobId = message.mediaBlobId
        if (blobId.isNullOrBlank()) {
            error = "媒体文件不可用"
            loading = false
        } else {
            onLoadBlob(blobId, message.mediaSizeBytes) { result ->
                result.onSuccess { loaded ->
                    bytes = loaded
                    runCatching { writeMediaCache(context, message, loaded) }
                        .onSuccess { file = it }
                        .onFailure { error = it.message }
                    loading = false
                }.onFailure {
                    error = it.message ?: "无法读取媒体"
                    loading = false
                }
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(18.dp),
                color = Color.White.copy(alpha = 0.14f),
                modifier = Modifier
                    .clip(RoundedCornerShape(18.dp))
                    .clickable(onClick = onClose),
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TelegramIcon(TelegramGlyph.CHEVRON_LEFT, tint = Color.White, size = 16.dp)
                    Spacer(Modifier.width(4.dp))
                    Text("返回", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                }
            }
            Text(
                text = message.mediaFileName ?: when (message.contentType) {
                    "photo" -> "图片"
                    "video" -> "视频"
                    else -> "文件"
                },
                color = Color.White,
                fontSize = 15.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (file != null) {
                Surface(
                    shape = RoundedCornerShape(18.dp),
                    color = Color(0xFF0A84FF),
                    modifier = Modifier
                        .clip(RoundedCornerShape(18.dp))
                        .clickable { shareMediaFile(context, file!!, message.mediaMimeType) },
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramIcon(TelegramGlyph.SHARE_IOS, tint = Color.White, size = 15.dp)
                        Spacer(Modifier.width(5.dp))
                        Text("分享", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                    }
                }
            }
        }
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            when {
                loading -> CircularProgressIndicator(color = Color(0xFF0A84FF))
                error != null -> Text(error.orEmpty(), color = Color.White, modifier = Modifier.padding(24.dp))
                message.contentType == "photo" && bytes != null -> {
                    val bitmap = remember(bytes) { BitmapFactory.decodeByteArray(bytes, 0, bytes!!.size) }
                    if (bitmap != null) {
                        Image(
                            bitmap.asImageBitmap(),
                            contentDescription = message.mediaFileName ?: "图片",
                            modifier = Modifier.fillMaxSize().padding(8.dp),
                        )
                    } else {
                        Text("无法解码图片", color = Color.White)
                    }
                }
                message.contentType == "video" && file != null -> {
                    AndroidView(
                        factory = { viewContext ->
                            VideoView(viewContext).apply {
                                val controller = MediaController(viewContext)
                                controller.setAnchorView(this)
                                setMediaController(controller)
                                setVideoPath(file!!.absolutePath)
                                setOnPreparedListener { it.isLooping = false; start() }
                            }
                        },
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                file != null -> {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .verticalScroll(rememberScrollState())
                            .padding(28.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Surface(
                            shape = RoundedCornerShape(20.dp),
                            color = Color.White.copy(alpha = 0.12f),
                        ) {
                            Box(Modifier.size(72.dp), contentAlignment = Alignment.Center) {
                                TelegramIcon(TelegramGlyph.DOCUMENT_FILE, tint = Color.White, size = 34.dp)
                            }
                        }
                        Text(
                            message.mediaFileName ?: "文件",
                            color = Color.White,
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(message.mediaMimeType ?: "application/octet-stream", color = Color(0xFF98989D), fontSize = 13.sp)
                        val displayBytes = if (message.mediaSizeBytes > 0) {
                            message.mediaSizeBytes.toLong()
                        } else {
                            bytes?.size?.toLong() ?: 0L
                        }
                        Text(
                            android.text.format.Formatter.formatFileSize(context, displayBytes),
                            color = Color(0xFF98989D),
                            fontSize = 13.sp,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = Color(0xFF0A84FF),
                                modifier = Modifier
                                    .clip(RoundedCornerShape(12.dp))
                                    .clickable { openMediaFile(context, file!!, message.mediaMimeType) },
                            ) {
                                Text(
                                    "用其他 App 打开",
                                    color = Color.White,
                                    fontSize = 14.sp,
                                    fontWeight = FontWeight.Medium,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                                )
                            }
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = Color.White.copy(alpha = 0.16f),
                                modifier = Modifier
                                    .clip(RoundedCornerShape(12.dp))
                                    .clickable { shareMediaFile(context, file!!, message.mediaMimeType) },
                            ) {
                                Text(
                                    "分享文件",
                                    color = Color.White,
                                    fontSize = 14.sp,
                                    fontWeight = FontWeight.Medium,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

internal fun sanitizeMediaCacheFileName(rawName: String?, fallbackId: String): String {
    val safeFallbackId = fallbackId
        .filterNot { it.code < 0x20 || it.code == 0x7F }
        .replace(Regex("[^A-Za-z0-9._-]"), "_")
        .replace("..", "_")
        .take(64)
        .ifBlank { "item" }
    val fallback = "media-$safeFallbackId"
    val cleaned = rawName
        ?.filterNot { it.code < 0x20 || it.code == 0x7F }
        ?.trim()
        .orEmpty()
    if (cleaned.isEmpty() || cleaned == "." || cleaned == "..") return fallback
    val segments = cleaned.split('/', '\\')
    if (segments.any { it.trim() == ".." }) return fallback
    val flattened = cleaned.replace('/', '-').replace('\\', '-').trim()
    if (flattened.isEmpty() || flattened == "." || flattened == "..") return fallback
    val capped = truncateMediaFileName(flattened)
    if (capped.isEmpty() || capped == "." || capped == "..") return fallback
    return capped
}

private fun truncateMediaFileName(name: String, maxChars: Int = 120, maxUtf8Bytes: Int = 200): String {
    if (name.length <= maxChars && name.toByteArray(Charsets.UTF_8).size <= maxUtf8Bytes) {
        return name
    }
    val dotIdx = name.lastIndexOf('.')
    val ext = if (dotIdx > 0 && dotIdx < name.lastIndex) {
        val candidate = name.substring(dotIdx)
        if (candidate.length in 2..12 && candidate.toByteArray(Charsets.UTF_8).size <= 24) candidate else ""
    } else {
        ""
    }
    val stem = if (ext.isNotEmpty()) name.substring(0, dotIdx) else name
    val extBytes = ext.toByteArray(Charsets.UTF_8).size
    val stemCharBudget = (maxChars - ext.length).coerceAtLeast(1)
    val stemByteBudget = (maxUtf8Bytes - extBytes).coerceAtLeast(1)
    val out = StringBuilder(minOf(stem.length, stemCharBudget) + ext.length)
    var usedBytes = 0
    var idx = 0
    while (idx < stem.length) {
        val cp = Character.codePointAt(stem, idx)
        val charCount = Character.charCount(cp)
        val cpBytes = when {
            cp <= 0x7F -> 1
            cp <= 0x7FF -> 2
            cp <= 0xFFFF -> 3
            else -> 4
        }
        if (out.length + charCount > stemCharBudget || usedBytes + cpBytes > stemByteBudget) break
        out.appendCodePoint(cp)
        usedBytes += cpBytes
        idx += charCount
    }
    out.append(ext)
    return out.toString().trim()
}

private fun writeMediaCache(context: Context, message: ChatMessage, bytes: ByteArray): File {
    val directory = File(context.cacheDir, "fabushi-media").apply { mkdirs() }
    val name = sanitizeMediaCacheFileName(message.mediaFileName, message.id)
    return File(directory, name).apply { writeBytes(bytes) }
}

private fun mediaUri(context: Context, file: File): Uri =
    FileProvider.getUriForFile(context, "${context.packageName}.media", file)

private fun openMediaFile(context: Context, file: File, mimeType: String?) {
    val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(mediaUri(context, file), mimeType ?: "application/octet-stream")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    try {
        context.startActivity(intent)
    } catch (_: ActivityNotFoundException) {
        shareMediaFile(context, file, mimeType)
    }
}

private fun shareMediaFile(context: Context, file: File, mimeType: String?) {
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = mimeType ?: "application/octet-stream"
        putExtra(Intent.EXTRA_STREAM, mediaUri(context, file))
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(intent, "分享文件"))
}
