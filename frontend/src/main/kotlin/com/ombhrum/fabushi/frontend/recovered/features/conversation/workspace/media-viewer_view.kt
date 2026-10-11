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
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import java.io.File

private const val ANDROID_MEDIA_MIN_ZOOM = 1f
private const val ANDROID_MEDIA_MAX_ZOOM = 5f

internal fun isAndroidMediaPreviewable(message: ChatMessage): Boolean =
    message.contentType == "photo" || message.contentType == "video"

internal fun projectAndroidMediaGallery(
    messages: List<ChatMessage>,
    startMessageId: String,
): List<ChatMessage> {
    val start = messages.firstOrNull { it.id == startMessageId } ?: return emptyList()
    if (!isAndroidMediaPreviewable(start)) return listOf(start)
    return messages.filter(::isAndroidMediaPreviewable)
}

internal fun initialAndroidMediaGalleryIndex(
    gallery: List<ChatMessage>,
    startMessageId: String,
): Int = gallery.indexOfFirst { it.id == startMessageId }.takeIf { it >= 0 } ?: 0

internal fun stepAndroidMediaGalleryIndex(current: Int, delta: Int, count: Int): Int {
    if (count <= 0) return -1
    val base = current.coerceIn(0, count - 1)
    return ((base + delta) % count + count) % count
}

@Composable
internal fun AndroidMediaViewer(
    messages: List<ChatMessage>,
    startMessageId: String,
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    val gallery = remember(messages, startMessageId) {
        projectAndroidMediaGallery(messages, startMessageId)
    }
    if (gallery.isEmpty()) {
        LaunchedEffect(startMessageId) { onClose() }
        return
    }

    var currentIndex by remember(startMessageId) {
        mutableStateOf(initialAndroidMediaGalleryIndex(gallery, startMessageId))
    }
    LaunchedEffect(gallery.size) {
        if (currentIndex !in gallery.indices) currentIndex = 0
    }
    val message = gallery[currentIndex.coerceIn(0, gallery.lastIndex)]

    var bytes by remember(message.id) { mutableStateOf<ByteArray?>(null) }
    var file by remember(message.id) { mutableStateOf<File?>(null) }
    var error by remember(message.id) { mutableStateOf<String?>(null) }
    var loading by remember(message.id) { mutableStateOf(true) }
    var scale by remember(message.id) { mutableStateOf(ANDROID_MEDIA_MIN_ZOOM) }
    var offset by remember(message.id) { mutableStateOf(Offset.Zero) }
    var loadGeneration by remember { mutableStateOf(0L) }

    DisposableEffect(Unit) {
        onDispose { loadGeneration += 1 }
    }

    LaunchedEffect(message.id) {
        loadGeneration += 1
        val generation = loadGeneration
        bytes = null
        file = null
        error = null
        loading = true
        scale = ANDROID_MEDIA_MIN_ZOOM
        offset = Offset.Zero

        val blobId = message.mediaBlobId
        if (blobId == null || message.mediaSizeBytes <= 0) {
            error = "媒体文件不可用"
            loading = false
        } else {
            onLoadBlob(blobId, message.mediaSizeBytes) { result ->
                if (generation == loadGeneration) {
                    result.onSuccess { loaded ->
                        bytes = loaded
                        runCatching { writeMediaCache(context, message, loaded) }
                            .onSuccess { cached -> file = cached }
                            .onFailure { failure -> error = failure.message }
                        loading = false
                    }.onFailure { failure ->
                        error = failure.message ?: "无法读取媒体"
                        loading = false
                    }
                }
            }
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            .testTag("android-media-viewer"),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Button(onClick = onClose, modifier = Modifier.testTag("android-media-close")) {
                Text("返回")
            }
            if (gallery.size > 1) {
                TextButton(
                    onClick = {
                        currentIndex = stepAndroidMediaGalleryIndex(currentIndex, -1, gallery.size)
                    },
                    modifier = Modifier.testTag("android-media-previous"),
                ) { Text("‹", fontSize = 28.sp) }
            }
            Column(Modifier.weight(1f).padding(horizontal = 10.dp)) {
                Text(
                    message.mediaFileName ?: when (message.contentType) {
                        "photo" -> "图片"
                        "video" -> "视频"
                        else -> "文件"
                    },
                    color = Color.White,
                    maxLines = 1,
                )
                if (gallery.size > 1) {
                    Text(
                        (currentIndex + 1).toString() + " / " + gallery.size,
                        color = Color.Gray,
                        fontSize = 11.sp,
                    )
                }
            }
            if (gallery.size > 1) {
                TextButton(
                    onClick = {
                        currentIndex = stepAndroidMediaGalleryIndex(currentIndex, 1, gallery.size)
                    },
                    modifier = Modifier.testTag("android-media-next"),
                ) { Text("›", fontSize = 28.sp) }
            }
            if (message.contentType == "photo" && scale > ANDROID_MEDIA_MIN_ZOOM) {
                TextButton(
                    onClick = {
                        scale = ANDROID_MEDIA_MIN_ZOOM
                        offset = Offset.Zero
                    },
                    modifier = Modifier.testTag("android-media-fit"),
                ) { Text("适应") }
            }
            if (file != null) {
                Button(
                    onClick = { shareMediaFile(context, file!!, message.mediaMimeType) },
                    modifier = Modifier.testTag("android-media-share"),
                ) { Text("分享") }
            }
        }

        Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
            when {
                loading -> CircularProgressIndicator(modifier = Modifier.testTag("android-media-loading"))
                error != null -> Text(
                    error.orEmpty(),
                    color = Color.White,
                    modifier = Modifier.padding(24.dp).testTag("android-media-error"),
                )
                message.contentType == "photo" && bytes != null -> {
                    val bitmap = remember(bytes) {
                        BitmapFactory.decodeByteArray(bytes, 0, bytes!!.size)
                    }
                    if (bitmap != null) {
                        Image(
                            bitmap.asImageBitmap(),
                            contentDescription = message.mediaFileName ?: "图片",
                            contentScale = ContentScale.Fit,
                            modifier = Modifier
                                .fillMaxSize()
                                .padding(8.dp)
                                .pointerInput(message.id) {
                                    detectTransformGestures { _, pan, zoom, _ ->
                                        val nextScale = (scale * zoom)
                                            .coerceIn(ANDROID_MEDIA_MIN_ZOOM, ANDROID_MEDIA_MAX_ZOOM)
                                        scale = nextScale
                                        offset = if (nextScale <= ANDROID_MEDIA_MIN_ZOOM) {
                                            Offset.Zero
                                        } else {
                                            offset + pan
                                        }
                                    }
                                }
                                .graphicsLayer(
                                    scaleX = scale,
                                    scaleY = scale,
                                    translationX = offset.x,
                                    translationY = offset.y,
                                )
                                .testTag("android-media-image"),
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
                                tag = file!!.absolutePath
                                setVideoPath(file!!.absolutePath)
                                setOnPreparedListener { player ->
                                    player.isLooping = false
                                    start()
                                }
                            }
                        },
                        update = { view ->
                            val expectedPath = file!!.absolutePath
                            if (view.tag != expectedPath) {
                                view.tag = expectedPath
                                view.setVideoPath(expectedPath)
                                view.start()
                            }
                        },
                        modifier = Modifier.fillMaxSize().testTag("android-media-video"),
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
                        Text("📎", fontSize = 58.sp)
                        Text(
                            message.mediaFileName ?: "文件",
                            color = Color.White,
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Text(
                            message.mediaMimeType ?: "application/octet-stream",
                            color = Color.Gray,
                        )
                        Text(
                            android.text.format.Formatter.formatFileSize(
                                context,
                                message.mediaSizeBytes.toLong(),
                            ),
                            color = Color.Gray,
                        )
                        Button(
                            onClick = {
                                openMediaFile(context, file!!, message.mediaMimeType)
                            },
                        ) { Text("用其他 App 打开") }
                        Button(
                            onClick = {
                                shareMediaFile(context, file!!, message.mediaMimeType)
                            },
                        ) { Text("分享文件") }
                    }
                }
            }
        }

        if (gallery.size > 1) {
            LazyRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(58.dp)
                    .padding(horizontal = 8.dp, vertical = 4.dp)
                    .testTag("android-media-filmstrip"),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                itemsIndexed(gallery, key = { _, item -> item.id }) { index, item ->
                    TextButton(
                        onClick = { currentIndex = index },
                        modifier = Modifier.testTag("android-media-thumb-" + item.id),
                    ) {
                        Text(
                            if (index == currentIndex) {
                                "● " + (item.mediaFileName ?: (index + 1).toString())
                            } else {
                                item.mediaFileName ?: (index + 1).toString()
                            },
                            color = if (index == currentIndex) Color.White else Color.Gray,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}

private fun writeMediaCache(context: Context, message: ChatMessage, bytes: ByteArray): File {
    val directory = File(context.cacheDir, "fabushi-media").apply { mkdirs() }
    val name = (message.mediaFileName ?: ("media-" + message.id))
        .replace('/', '-')
        .replace('\\', '-')
    return File(directory, name).apply { writeBytes(bytes) }
}

private fun mediaUri(context: Context, file: File): Uri =
    FileProvider.getUriForFile(context, context.packageName + ".media", file)

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
