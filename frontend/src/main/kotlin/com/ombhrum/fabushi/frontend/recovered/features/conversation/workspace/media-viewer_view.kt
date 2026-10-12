package com.ombhrum.fabushi

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.net.Uri
import android.widget.MediaController
import android.widget.VideoView
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
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
import androidx.compose.runtime.rememberCoroutineScope
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

private const val ANDROID_MEDIA_MIN_ZOOM = 1f
private const val ANDROID_MEDIA_MAX_ZOOM = 5f
private const val ANDROID_PDF_MIN_ZOOM = 0.5f
private const val ANDROID_PDF_MAX_ZOOM = 4f

internal fun isAndroidMediaPreviewable(message: ChatMessage): Boolean =
    message.contentType == "photo" || message.contentType == "video"

internal fun isAndroidPdfMessage(message: ChatMessage): Boolean {
    val mime = message.mediaMimeType
        ?.substringBefore(';')
        ?.trim()
        ?.lowercase()
    val name = message.mediaFileName?.trim()?.lowercase()
    return mime == "application/pdf" || name?.endsWith(".pdf") == true
}

internal fun stepAndroidPdfPageIndex(current: Int, delta: Int, count: Int): Int {
    if (count <= 0) return -1
    return (current.coerceIn(0, count - 1) + delta).coerceIn(0, count - 1)
}

internal fun clampAndroidPdfZoom(value: Float): Float =
    value.coerceIn(ANDROID_PDF_MIN_ZOOM, ANDROID_PDF_MAX_ZOOM)

private data class AndroidRenderedPdfPage(
    val bitmap: Bitmap,
    val pageCount: Int,
)

private fun renderAndroidPdfPage(file: File, requestedPage: Int): AndroidRenderedPdfPage =
    ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
        PdfRenderer(descriptor).use { renderer ->
            check(renderer.pageCount > 0) { "PDF has no pages" }
            val pageIndex = requestedPage.coerceIn(0, renderer.pageCount - 1)
            renderer.openPage(pageIndex).use { page ->
                val maxEdge = 2048f
                val sourceWidth = page.width.coerceAtLeast(1)
                val sourceHeight = page.height.coerceAtLeast(1)
                val scale = (maxEdge / maxOf(sourceWidth, sourceHeight)).coerceAtMost(1f)
                val width = (sourceWidth * scale).toInt().coerceAtLeast(1)
                val height = (sourceHeight * scale).toInt().coerceAtLeast(1)
                val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
                page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                AndroidRenderedPdfPage(bitmap = bitmap, pageCount = renderer.pageCount)
            }
        }
    }

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

internal fun androidAttachmentSuggestedSaveName(fileName: String?, messageId: String): String {
    val fallback = "fabushi-" + messageId.take(64).ifBlank { "attachment" }
    val candidate = fileName
        ?.trim()
        ?.split('/', '\\')
        ?.lastOrNull()
        ?.filterNot(Char::isISOControl)
        ?.take(255)
        ?.takeIf(String::isNotBlank)
        ?: fallback
    return candidate.takeIf { it != "." && it != ".." } ?: fallback
}

internal interface AndroidAttachmentSaveTarget {
    fun writeFrom(source: File)
    fun discard()
}

internal fun saveAndroidAttachment(
    source: File,
    target: AndroidAttachmentSaveTarget,
): Result<Unit> = runCatching {
    target.writeFrom(source)
}.onFailure {
    runCatching(target::discard)
}

private class ContentResolverAttachmentSaveTarget(
    private val context: Context,
    private val uri: Uri,
) : AndroidAttachmentSaveTarget {
    override fun writeFrom(source: File) {
        val resolver = context.contentResolver
        val output = resolver.openOutputStream(uri, "wt")
            ?: error("Unable to open the selected destination")
        source.inputStream().use { input ->
            output.use { destination ->
                input.copyTo(destination)
                destination.flush()
            }
        }
    }

    override fun discard() {
        runCatching { context.contentResolver.delete(uri, null, null) }
    }
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
    var pdfPageIndex by remember(message.id) { mutableStateOf(0) }
    var pdfPageCount by remember(message.id) { mutableStateOf(0) }
    var pdfBitmap by remember(message.id) { mutableStateOf<Bitmap?>(null) }
    var pdfLoading by remember(message.id) { mutableStateOf(false) }
    var pdfError by remember(message.id) { mutableStateOf<String?>(null) }
    var pdfScale by remember(message.id) { mutableStateOf(1f) }
    var pdfOffset by remember(message.id) { mutableStateOf(Offset.Zero) }
    var pdfGeneration by remember { mutableStateOf(0L) }
    var spreadsheetSelectedCell by remember(message.id) { mutableStateOf<Pair<String, String>?>(null) }
    var spreadsheetSelectedSheet by remember(message.id) { mutableStateOf(0) }
    var saveNotice by remember(message.id) { mutableStateOf<String?>(null) }
    val saveScope = rememberCoroutineScope()
    val saveDocument = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(message.mediaMimeType ?: "application/octet-stream"),
    ) { destination ->
        val source = file
        if (destination == null || source == null) return@rememberLauncherForActivityResult
        val generation = loadGeneration
        saveNotice = "正在保存…"
        saveScope.launch {
            val result = withContext(Dispatchers.IO) {
                saveAndroidAttachment(
                    source,
                    ContentResolverAttachmentSaveTarget(context, destination),
                )
            }
            if (generation == loadGeneration) {
                saveNotice = if (result.isSuccess) "已保存" else "保存失败"
            }
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            loadGeneration += 1
            pdfGeneration += 1
        }
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
        pdfPageIndex = 0
        pdfPageCount = 0
        pdfBitmap = null
        pdfLoading = false
        pdfError = null
        pdfScale = 1f
        pdfOffset = Offset.Zero
        pdfGeneration += 1
        spreadsheetSelectedCell = null
        spreadsheetSelectedSheet = 0
        saveNotice = null

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
                saveNotice?.let { notice ->
                    Text(
                        notice,
                        color = if (notice == "保存失败") MaterialTheme.colorScheme.error else Color.Gray,
                        fontSize = 11.sp,
                        modifier = Modifier.testTag("android-media-save-status"),
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
                    onClick = {
                        saveNotice = null
                        saveDocument.launch(
                            androidAttachmentSuggestedSaveName(
                                message.mediaFileName,
                                message.id,
                            ),
                        )
                    },
                    modifier = Modifier.testTag("android-media-save"),
                ) { Text("保存") }
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
                isAndroidPdfMessage(message) && file != null -> {
                    LaunchedEffect(message.id, file, pdfPageIndex) {
                        pdfGeneration += 1
                        val generation = pdfGeneration
                        pdfLoading = true
                        pdfError = null
                        pdfBitmap = null
                        val rendered = withContext(Dispatchers.IO) {
                            runCatching { renderAndroidPdfPage(file!!, pdfPageIndex) }
                        }
                        if (generation != pdfGeneration) return@LaunchedEffect
                        rendered.onSuccess { page ->
                            pdfPageCount = page.pageCount
                            pdfPageIndex = pdfPageIndex.coerceIn(0, page.pageCount - 1)
                            pdfBitmap = page.bitmap
                            pdfLoading = false
                        }.onFailure { failure ->
                            pdfError = failure.message ?: "无法读取 PDF"
                            pdfLoading = false
                        }
                    }
                    Column(
                        Modifier.fillMaxSize().padding(8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            TextButton(
                                onClick = {
                                    pdfPageIndex = stepAndroidPdfPageIndex(
                                        pdfPageIndex,
                                        -1,
                                        pdfPageCount,
                                    )
                                    pdfScale = 1f
                                    pdfOffset = Offset.Zero
                                },
                                enabled = pdfPageCount > 0 && pdfPageIndex > 0 && !pdfLoading,
                                modifier = Modifier.testTag("android-pdf-previous"),
                            ) { Text("上一页", color = Color.White) }
                            Text(
                                if (pdfPageCount > 0) {
                                    "${pdfPageIndex + 1} / $pdfPageCount"
                                } else {
                                    "PDF"
                                },
                                color = Color.White,
                                modifier = Modifier.testTag("android-pdf-page-indicator"),
                            )
                            TextButton(
                                onClick = {
                                    pdfPageIndex = stepAndroidPdfPageIndex(
                                        pdfPageIndex,
                                        1,
                                        pdfPageCount,
                                    )
                                    pdfScale = 1f
                                    pdfOffset = Offset.Zero
                                },
                                enabled = pdfPageCount > 0 &&
                                    pdfPageIndex < pdfPageCount - 1 &&
                                    !pdfLoading,
                                modifier = Modifier.testTag("android-pdf-next"),
                            ) { Text("下一页", color = Color.White) }
                            TextButton(
                                onClick = {
                                    pdfScale = clampAndroidPdfZoom(pdfScale - 0.25f)
                                    if (pdfScale <= 1f) pdfOffset = Offset.Zero
                                },
                                enabled = pdfScale > ANDROID_PDF_MIN_ZOOM,
                                modifier = Modifier.testTag("android-pdf-zoom-out"),
                            ) { Text("−", color = Color.White) }
                            Text(
                                "${(pdfScale * 100).toInt()}%",
                                color = Color.White,
                                modifier = Modifier.testTag("android-pdf-zoom-indicator"),
                            )
                            TextButton(
                                onClick = {
                                    pdfScale = clampAndroidPdfZoom(pdfScale + 0.25f)
                                },
                                enabled = pdfScale < ANDROID_PDF_MAX_ZOOM,
                                modifier = Modifier.testTag("android-pdf-zoom-in"),
                            ) { Text("+", color = Color.White) }
                            TextButton(
                                onClick = {
                                    pdfScale = 1f
                                    pdfOffset = Offset.Zero
                                },
                                enabled = pdfScale != 1f || pdfOffset != Offset.Zero,
                                modifier = Modifier.testTag("android-pdf-fit"),
                            ) { Text("适应", color = Color.White) }
                            Button(
                                onClick = {
                                    openMediaFile(context, file!!, message.mediaMimeType)
                                },
                                modifier = Modifier.testTag("android-pdf-open-external"),
                            ) { Text("外部打开") }
                        }
                        Box(
                            Modifier.weight(1f).fillMaxWidth(),
                            contentAlignment = Alignment.Center,
                        ) {
                            when {
                                pdfLoading -> CircularProgressIndicator(
                                    modifier = Modifier.testTag("android-pdf-loading"),
                                )
                                pdfError != null -> Text(
                                    pdfError.orEmpty(),
                                    color = Color.White,
                                    modifier = Modifier
                                        .padding(24.dp)
                                        .testTag("android-pdf-error"),
                                )
                                pdfBitmap != null -> Image(
                                    bitmap = pdfBitmap!!.asImageBitmap(),
                                    contentDescription = message.mediaFileName ?: "PDF",
                                    contentScale = ContentScale.Fit,
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .pointerInput(message.id, pdfPageIndex) {
                                            detectTransformGestures { _, pan, zoom, _ ->
                                                val nextScale = clampAndroidPdfZoom(pdfScale * zoom)
                                                pdfScale = nextScale
                                                pdfOffset = if (nextScale <= 1f) {
                                                    Offset.Zero
                                                } else {
                                                    pdfOffset + pan
                                                }
                                            }
                                        }
                                        .graphicsLayer(
                                            scaleX = pdfScale,
                                            scaleY = pdfScale,
                                            translationX = pdfOffset.x,
                                            translationY = pdfOffset.y,
                                        )
                                        .testTag("android-pdf-page"),
                                )
                            }
                        }
                    }
                }
                isAndroidLegacyBinarySpreadsheet(message.mediaFileName, message.mediaMimeType) && file != null -> {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .verticalScroll(rememberScrollState())
                            .padding(28.dp)
                            .testTag("android-legacy-xls-replacement"),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Text("📊", fontSize = 58.sp)
                        Text(
                            message.mediaFileName ?: "旧版 Excel 工作簿",
                            color = Color.White,
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Text(
                            "旧版 Excel (.xls) 在 Android 上使用已安装的表格应用查看。Fabushi 不在应用内解析 BIFF，也不会执行工作簿宏。",
                            color = Color.LightGray,
                        )
                        Text(
                            "文件只从应用私有缓存通过 FileProvider 授予临时只读访问；若设备没有兼容查看器，将改为系统分享选择器。",
                            color = Color.Gray,
                            fontSize = 12.sp,
                            modifier = Modifier.testTag("android-legacy-xls-security-boundary"),
                        )
                        Button(
                            onClick = {
                                openMediaFile(
                                    context,
                                    file!!,
                                    androidLegacySpreadsheetOpenMimeType(message.mediaMimeType),
                                )
                            },
                            modifier = Modifier.testTag("android-legacy-xls-open-external"),
                        ) { Text("用表格 App 打开") }
                        Button(
                            onClick = {
                                shareMediaFile(
                                    context,
                                    file!!,
                                    androidLegacySpreadsheetOpenMimeType(message.mediaMimeType),
                                )
                            },
                            modifier = Modifier.testTag("android-legacy-xls-share"),
                        ) { Text("分享文件") }
                    }
                }
                isAndroidSpreadsheet(message.mediaFileName, message.mediaMimeType) && bytes != null -> {
                    val parsed = remember(message.id, bytes) {
                        runCatching {
                            parseAndroidSpreadsheet(
                                bytes = bytes!!,
                                fileName = message.mediaFileName,
                                mimeType = message.mediaMimeType,
                            )
                        }
                    }
                    parsed.onSuccess { sheets ->
                        val selectedIndex = spreadsheetSelectedSheet.coerceIn(
                            0,
                            (sheets.size - 1).coerceAtLeast(0),
                        )
                        val table = sheets.getOrNull(selectedIndex)
                        if (table == null) {
                            Text(
                                "File unavailable",
                                color = Color.White,
                                modifier = Modifier.testTag("android-spreadsheet-error"),
                            )
                        } else {
                            val header = table.rows.firstOrNull().orEmpty()
                            val dataRows = table.rows.drop(1).take(ANDROID_SPREADSHEET_RENDER_ROWS)
                            val columnCount = table.rows.maxOfOrNull { it.size }
                                ?.coerceAtMost(ANDROID_SPREADSHEET_RENDER_ROWS) ?: 0
                            Column(
                                Modifier.fillMaxSize().padding(8.dp).testTag("android-spreadsheet-viewer"),
                                verticalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Row(
                                    Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    Text(
                                        ((table.totalRows - 1).coerceAtLeast(0)).toString() + " rows" +
                                            if (table.totalRows > ANDROID_SPREADSHEET_RENDER_ROWS + 1) {
                                                " · showing first " + ANDROID_SPREADSHEET_RENDER_ROWS
                                            } else "",
                                        color = Color.White,
                                        modifier = Modifier
                                            .weight(1f)
                                            .testTag("android-spreadsheet-row-count"),
                                    )
                                    if (sheets.size > 1) {
                                        LazyRow(
                                            modifier = Modifier
                                                .weight(2f)
                                                .testTag("android-spreadsheet-sheets"),
                                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                                        ) {
                                            itemsIndexed(sheets, key = { index, sheet -> sheet.name + ":" + index }) { index, sheet ->
                                                TextButton(
                                                    onClick = {
                                                        spreadsheetSelectedSheet = index
                                                        spreadsheetSelectedCell = null
                                                    },
                                                    modifier = Modifier.testTag("android-spreadsheet-sheet-" + index),
                                                ) {
                                                    Text(
                                                        (if (index == selectedIndex) "● " else "") + sheet.name,
                                                        color = if (index == selectedIndex) Color.White else Color.Gray,
                                                        maxLines = 1,
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                                if (dataRows.isEmpty()) {
                                    Text(
                                        "This sheet is empty",
                                        color = Color.White,
                                        modifier = Modifier.testTag("android-spreadsheet-empty"),
                                    )
                                } else {
                                    Column(
                                        Modifier.weight(1f).fillMaxWidth()
                                            .horizontalScroll(rememberScrollState())
                                            .verticalScroll(rememberScrollState())
                                            .testTag("android-spreadsheet-table"),
                                    ) {
                                        @Composable
                                        fun tableCell(value: String, label: String, isHeader: Boolean) {
                                            TextButton(
                                                onClick = {
                                                    spreadsheetSelectedCell =
                                                        if (value.isBlank()) null else label to value
                                                },
                                                modifier = Modifier.width(160.dp).testTag("android-spreadsheet-cell"),
                                            ) {
                                                Text(
                                                    value,
                                                    color = if (isHeader) Color.White else Color.LightGray,
                                                    maxLines = 2,
                                                )
                                            }
                                        }
                                        if (columnCount > 0) {
                                            Row {
                                                Text(
                                                    "#",
                                                    color = Color.Gray,
                                                    modifier = Modifier.width(48.dp).padding(8.dp),
                                                )
                                                repeat(columnCount) { column ->
                                                    val label = header.getOrNull(column)
                                                        .takeUnless(String?::isNullOrBlank)
                                                        ?: ("Column " + (column + 1))
                                                    tableCell(
                                                        header.getOrNull(column).orEmpty(),
                                                        label + " · header",
                                                        true,
                                                    )
                                                }
                                            }
                                        }
                                        dataRows.forEachIndexed { rowIndex, row ->
                                            Row {
                                                Text(
                                                    (rowIndex + 1).toString(),
                                                    color = Color.Gray,
                                                    modifier = Modifier.width(48.dp).padding(8.dp),
                                                )
                                                repeat(columnCount) { column ->
                                                    val label = (
                                                        header.getOrNull(column)
                                                            .takeUnless(String?::isNullOrBlank)
                                                            ?: ("Column " + (column + 1))
                                                        ) + " · row " + (rowIndex + 1)
                                                    tableCell(row.getOrNull(column).orEmpty(), label, false)
                                                }
                                            }
                                        }
                                    }
                                }
                                spreadsheetSelectedCell?.let { selected ->
                                    Column(
                                        Modifier.fillMaxWidth()
                                            .background(Color.White.copy(alpha = 0.08f))
                                            .padding(10.dp)
                                            .testTag("android-spreadsheet-cell-detail"),
                                    ) {
                                        Row(
                                            Modifier.fillMaxWidth(),
                                            horizontalArrangement = Arrangement.SpaceBetween,
                                        ) {
                                            Text(selected.first, color = Color.Gray)
                                            TextButton(onClick = { spreadsheetSelectedCell = null }) {
                                                Text("×", color = Color.White)
                                            }
                                        }
                                        Text(selected.second, color = Color.White)
                                    }
                                }
                            }
                        }
                    }.onFailure { failure ->
                        Column(
                            Modifier.padding(28.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            Text(
                                failure.message ?: "Couldn't read this spreadsheet",
                                color = Color.White,
                                modifier = Modifier.testTag("android-spreadsheet-error"),
                            )
                            if (file != null) {
                                Button(onClick = { openMediaFile(context, file!!, message.mediaMimeType) }) {
                                    Text("用其他 App 打开")
                                }
                            }
                        }
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
