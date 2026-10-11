package com.ombhrum.fabushi

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.widget.MediaController
import android.widget.VideoView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import java.io.File

@Composable
internal fun AndroidMediaViewer(
    message: ChatMessage,
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    var bytes by remember(message.id) { mutableStateOf<ByteArray?>(null) }
    var file by remember(message.id) { mutableStateOf<File?>(null) }
    var error by remember(message.id) { mutableStateOf<String?>(null) }
    var loading by remember(message.id) { mutableStateOf(true) }

    LaunchedEffect(message.id) {
        val blobId = message.mediaBlobId
        if (blobId == null || message.mediaSizeBytes <= 0) {
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

    Column(Modifier.fillMaxSize().background(Color.Black)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("‹", color = Color.White, fontSize = 34.sp, modifier = Modifier.padding(8.dp).then(Modifier))
            Button(onClick = onClose) { Text("返回") }
            Text(
                message.mediaFileName ?: when (message.contentType) { "photo" -> "图片"; "video" -> "视频"; else -> "文件" },
                color = Color.White,
                modifier = Modifier.padding(start = 12.dp).weight(1f),
                maxLines = 1,
            )
            if (file != null) Button(onClick = { shareMediaFile(context, file!!, message.mediaMimeType) }) { Text("分享") }
        }
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            when {
                loading -> CircularProgressIndicator()
                error != null -> Text(error.orEmpty(), color = Color.White, modifier = Modifier.padding(24.dp))
                message.contentType == "photo" && bytes != null -> {
                    val bitmap = remember(bytes) { BitmapFactory.decodeByteArray(bytes, 0, bytes!!.size) }
                    if (bitmap != null) {
                        Image(bitmap.asImageBitmap(), contentDescription = message.mediaFileName ?: "图片", modifier = Modifier.fillMaxSize().padding(8.dp))
                    } else Text("无法解码图片", color = Color.White)
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
                        Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(28.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Text("📎", fontSize = 58.sp)
                        Text(message.mediaFileName ?: "文件", color = Color.White, style = MaterialTheme.typography.titleMedium)
                        Text(message.mediaMimeType ?: "application/octet-stream", color = Color.Gray)
                        Text(android.text.format.Formatter.formatFileSize(context, message.mediaSizeBytes.toLong()), color = Color.Gray)
                        Button(onClick = { openMediaFile(context, file!!, message.mediaMimeType) }) { Text("用其他 App 打开") }
                        Button(onClick = { shareMediaFile(context, file!!, message.mediaMimeType) }) { Text("分享文件") }
                    }
                }
            }
        }
    }
}

private fun writeMediaCache(context: Context, message: ChatMessage, bytes: ByteArray): File {
    val directory = File(context.cacheDir, "fabushi-media").apply { mkdirs() }
    val name = (message.mediaFileName ?: "media-${message.id}").replace('/', '-').replace('\\', '-')
    return File(directory, name).apply { writeBytes(bytes) }
}

private fun mediaUri(context: Context, file: File): Uri =
    FileProvider.getUriForFile(context, "${context.packageName}.media", file)

private fun openMediaFile(context: Context, file: File, mimeType: String?) {
    val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(mediaUri(context, file), mimeType ?: "application/octet-stream")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    try { context.startActivity(intent) } catch (_: ActivityNotFoundException) { shareMediaFile(context, file, mimeType) }
}

private fun shareMediaFile(context: Context, file: File, mimeType: String?) {
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = mimeType ?: "application/octet-stream"
        putExtra(Intent.EXTRA_STREAM, mediaUri(context, file))
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(intent, "分享文件"))
}
