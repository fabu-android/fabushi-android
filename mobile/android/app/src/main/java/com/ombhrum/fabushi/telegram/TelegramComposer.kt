package com.ombhrum.fabushi.telegram

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat

private val QuickEmojiPalette = listOf(
    "😀", "😂", "🥹", "😍", "🤩", "😎", "🥳", "🤔", "🙏", "🪷", "☸️", "🪔",
    "👍", "❤️", "🔥", "🎉", "⚡", "✨", "💎", "🤖", "🦀", "🚀", "🌍", "🕊️",
    "🙌", "👏", "💯", "✅", "📌", "🔒", "📱", "🧠", "🎯", "🏆", "🌈", "☀️",
)

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TelegramComposer(
    conversationId: String,
    draftText: String,
    replyTarget: TelegramMessageItem?,
    editTarget: TelegramMessageItem?,
    recordingState: ActiveRecordingState,
    slashCommands: List<SlashCommandSuggestion>,
    stickerPacks: List<StickerPackModel>,
    gifCatalog: List<GifItemModel>,
    onDraftChange: (String) -> Unit,
    onClearReplyOrEdit: () -> Unit,
    onSendMessage: (text: String, silent: Boolean, scheduledAtMs: Long?, ttlSeconds: Int?) -> Unit,
    onToggleRecordMode: () -> Unit,
    onStartRecording: () -> Unit,
    onDragRecording: (Float, Float) -> Unit,
    onCancelRecording: () -> Unit,
    onFinishRecording: () -> Unit,
    onSendSticker: (emoji: String, packTitle: String) -> Unit,
    onSendGif: (GifItemModel) -> Unit,
    onSendPollOrQuiz: (question: String, options: List<String>, anonymous: Boolean, multi: Boolean, quiz: Boolean, correctIdx: Int, explanation: String?) -> Unit,
    onSendLocation: (Double, Double, String) -> Unit,
    onSendContact: (String, String) -> Unit,
    onSendMediaAttachment: (fileName: String, mimeType: String, bytes: ByteArray) -> Unit,
    onOpenGlobalDharmaMiniApp: () -> Unit,
    onPromptMikaTask: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val context = LocalContext.current

    var tfv by remember(conversationId) {
        mutableStateOf(TextFieldValue(text = draftText, selection = TextRange(draftText.length)))
    }
    LaunchedEffect(draftText) {
        if (draftText != tfv.text) {
            tfv = TextFieldValue(text = draftText, selection = TextRange(draftText.length))
        }
    }

    var showFormatBar by remember { mutableStateOf(false) }
    var showLinkDialog by remember { mutableStateOf(false) }
    var showEmojiStickerPanel by remember { mutableStateOf(false) }
    var emojiPanelTab by remember { mutableIntStateOf(0) } // 0=Emoji, 1=Stickers, 2=GIFs
    var showAttachmentSheet by remember { mutableStateOf(false) }
    var showPollCreatorDialog by remember { mutableStateOf(false) }
    var showContactDialog by remember { mutableStateOf(false) }
    var showSendOptionsMenu by remember { mutableStateOf(false) }

    val mediaPicker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) {
            runCatching {
                val mime = context.contentResolver.getType(uri) ?: "application/octet-stream"
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: ByteArray(0)
                val name = uri.lastPathSegment?.substringAfterLast('/') ?: "attachment"
                if (bytes.isNotEmpty()) onSendMediaAttachment(name, mime, bytes)
            }
        }
    }

    val audioPermLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { _ ->
        onStartRecording()
    }

    val locPermLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { _ ->
        fetchCurrentOrDefaultLocation(context) { lat, lon, label -> onSendLocation(lat, lon, label) }
    }

    val hasSelection = tfv.selection.max > tfv.selection.min
    val activeSlashSuggestions = remember(tfv.text, slashCommands) {
        val trimmed = tfv.text.trim()
        if (trimmed.startsWith("/") && !trimmed.contains(' ')) {
            slashCommands.filter { it.command.startsWith(trimmed, ignoreCase = true) }
        } else emptyList()
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.surfaceColor)
            .navigationBarsPadding(),
    ) {
        TelegramHairlineDivider()

        // 1. Slash Command Autocomplete Popup
        AnimatedVisibility(visible = activeSlashSuggestions.isNotEmpty()) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .height((activeSlashSuggestions.size.coerceAtMost(4) * 46).dp)
                    .background(palette.elevatedSurface),
            ) {
                items(activeSlashSuggestions, key = { it.command }) { cmd ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                val next = "${cmd.command} "
                                tfv = TextFieldValue(next, TextRange(next.length))
                                onDraftChange(next)
                            }
                            .padding(horizontal = 16.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(cmd.command, color = palette.primaryAccent, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                        Spacer(Modifier.width(12.dp))
                        Text(cmd.description, color = palette.secondaryText, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    }
                    TelegramHairlineDivider()
                }
            }
        }

        // 2. Reply / Edit Preview Header
        AnimatedVisibility(visible = replyTarget != null || editTarget != null) {
            val target = editTarget ?: replyTarget
            if (target != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.elevatedSurface)
                        .padding(horizontal = 12.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TelegramIcon(
                        glyph = if (editTarget != null) TelegramGlyph.PENCIL else TelegramGlyph.REPLY_ARROW,
                        tint = palette.primaryAccent,
                        size = 19.dp,
                    )
                    Spacer(Modifier.width(10.dp))
                    Box(
                        modifier = Modifier
                            .width(2.5.dp)
                            .height(30.dp)
                            .background(palette.primaryAccent),
                    )
                    Spacer(Modifier.width(8.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = if (editTarget != null) "编辑消息" else "回复 ${target.senderName}",
                            color = palette.primaryAccent,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                        )
                        Text(
                            text = target.text,
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Box(
                        modifier = Modifier.size(32.dp).clip(CircleShape).clickable(onClick = onClearReplyOrEdit),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.CLOSE, tint = palette.hintText, size = 18.dp)
                    }
                }
            }
        }

        // 3. UTF-16 Selection Formatting Bar (B / I / U / S / Spoiler / Code / Pre / Link)
        AnimatedVisibility(visible = hasSelection || showFormatBar) {
            LazyRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.elevatedSurface)
                    .padding(vertical = 5.dp),
                contentPadding = PaddingValues(horizontal = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                val formats = listOf(
                    "B 粗体" to MessageEntityType.BOLD,
                    "I 斜体" to MessageEntityType.ITALIC,
                    "U 下划线" to MessageEntityType.UNDERLINE,
                    "S 删除线" to MessageEntityType.STRIKETHROUGH,
                    "||剧透||" to MessageEntityType.SPOILER,
                    "`代码`" to MessageEntityType.CODE,
                    "```块```" to MessageEntityType.PRE,
                )
                items(formats, key = { it.first }) { (label, entityType) ->
                    FormatChip(label) {
                        val next = TelegramEntitiesEngine.wrapDraftSelectionWithMarkdown(
                            draft = tfv.text,
                            selectionStart = tfv.selection.min,
                            selectionEnd = tfv.selection.max,
                            type = entityType,
                        )
                        tfv = TextFieldValue(next, TextRange(next.length))
                        onDraftChange(next)
                    }
                }
                item(key = "fmt-link") {
                    FormatChip("🔗 链接") { showLinkDialog = true }
                }
            }
        }

        // 4. Active Voice / Round Video Recording Bar OR Standard Composer Row
        if (recordingState.isRecording) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(palette.destructiveRed),
                )
                Spacer(Modifier.width(8.dp))
                val sec = (recordingState.elapsedMs / 1000L).toInt()
                Text(
                    text = "%02d:%02d".format(sec / 60, sec % 60),
                    color = palette.primaryText,
                    fontWeight = FontWeight.Bold,
                    fontSize = 15.sp,
                )
                Spacer(Modifier.width(10.dp))
                // Mini live 5-bit waveform preview
                Row(
                    modifier = Modifier.weight(1f).height(24.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    recordingState.liveBars.takeLast(28).forEach { bar ->
                        val hFrac = (bar.coerceIn(2, 31) / 31f).coerceIn(0.12f, 1f)
                        Box(
                            modifier = Modifier
                                .width(3.dp)
                                .height((22f * hFrac).dp)
                                .clip(RoundedCornerShape(1.5.dp))
                                .background(palette.primaryAccent),
                        )
                    }
                }
                TextButton(onClick = onCancelRecording) {
                    Text("取消", color = palette.destructiveRed, fontWeight = FontWeight.SemiBold)
                }
                Button(onClick = onFinishRecording) {
                    Text(if (recordingState.mode == ComposerRecordMode.ROUND_VIDEO_NOTE) "发送圆视频" else "发送语音")
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.Bottom,
            ) {
                // Emoji / Sticker / GIF Toggle
                Box(
                    modifier = Modifier
                        .size(42.dp)
                        .clip(CircleShape)
                        .clickable { showEmojiStickerPanel = !showEmojiStickerPanel },
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.EMOJI_SMILE, tint = if (showEmojiStickerPanel) palette.primaryAccent else palette.hintText, size = 23.dp)
                }

                // Rich Input Field
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .padding(vertical = 8.dp, horizontal = 4.dp),
                ) {
                    BasicTextField(
                        value = tfv,
                        onValueChange = { next ->
                            tfv = next
                            onDraftChange(next.text)
                        },
                        maxLines = 6,
                        textStyle = TextStyle(color = palette.primaryText, fontSize = palette.settings.boundedFontSp),
                        cursorBrush = SolidColor(palette.primaryAccent),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("telegram-composer-input"),
                        decorationBox = { inner ->
                            if (tfv.text.isEmpty()) {
                                Text("消息（支持 **粗体** ||剧透|| /mika）", color = palette.hintText, fontSize = 15.sp)
                            }
                            inner()
                        },
                    )
                }

                // Format toggle button 'Aa'
                Box(
                    modifier = Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .clickable { showFormatBar = !showFormatBar },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        "Aa",
                        color = if (showFormatBar) palette.primaryAccent else palette.hintText,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }

                // Attachment Paperclip
                Box(
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .clickable { showAttachmentSheet = true }
                        .testTag("telegram-attach-button"),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.PAPERCLIP, tint = palette.hintText, size = 22.dp)
                }

                // Right-side Send Plane OR Voice Mic / Round Video Camera
                if (tfv.text.isNotBlank()) {
                    Box {
                        Box(
                            modifier = Modifier
                                .size(44.dp)
                                .clip(CircleShape)
                                .background(palette.primaryAccent)
                                .combinedClickable(
                                    onClick = {
                                        val sending = tfv.text
                                        tfv = TextFieldValue("")
                                        onSendMessage(sending, false, null, null)
                                    },
                                    onLongClick = { showSendOptionsMenu = true },
                                )
                                .testTag("telegram-send-button"),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.SEND_PLANE, tint = Color.White, size = 21.dp)
                        }
                        DropdownMenu(
                            expanded = showSendOptionsMenu,
                            onDismissRequest = { showSendOptionsMenu = false },
                        ) {
                            DropdownMenuItem(
                                text = { Text("🔕 静音无声发送 (Send Without Sound)") },
                                onClick = {
                                    showSendOptionsMenu = false
                                    val sending = tfv.text
                                    tfv = TextFieldValue("")
                                    onSendMessage(sending, true, null, null)
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("🕒 10 分钟后定时发送") },
                                onClick = {
                                    showSendOptionsMenu = false
                                    val sending = tfv.text
                                    tfv = TextFieldValue("")
                                    onSendMessage(sending, false, System.currentTimeMillis() + 600_000L, null)
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("🕒 明早 09:00 定时发送") },
                                onClick = {
                                    showSendOptionsMenu = false
                                    val sending = tfv.text
                                    tfv = TextFieldValue("")
                                    onSendMessage(sending, false, System.currentTimeMillis() + 43_200_000L, null)
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("🔥 阅后即焚 (60秒自毁)") },
                                onClick = {
                                    showSendOptionsMenu = false
                                    val sending = tfv.text
                                    tfv = TextFieldValue("")
                                    onSendMessage(sending, false, null, 60)
                                },
                            )
                        }
                    }
                } else {
                    Box(
                        modifier = Modifier
                            .size(44.dp)
                            .clip(CircleShape)
                            .background(palette.primaryAccent.copy(alpha = 0.14f))
                            .pointerInput(recordingState.mode) {
                                detectDragGesturesAfterLongPress(
                                    onDragStart = {
                                        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                                            onStartRecording()
                                        } else {
                                            audioPermLauncher.launch(Manifest.permission.RECORD_AUDIO)
                                        }
                                    },
                                    onDrag = { change, dragAmount ->
                                        change.consume()
                                        onDragRecording(dragAmount.x, dragAmount.y)
                                    },
                                    onDragEnd = {
                                        if (!recordingState.lockedHandsFree) onFinishRecording()
                                    },
                                )
                            }
                            .combinedClickable(
                                onClick = onToggleRecordMode,
                                onLongClick = {
                                    if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                                        onStartRecording()
                                    } else {
                                        audioPermLauncher.launch(Manifest.permission.RECORD_AUDIO)
                                    }
                                },
                            )
                            .testTag("telegram-record-button"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(
                            glyph = if (recordingState.mode == ComposerRecordMode.VOICE_MIC) TelegramGlyph.MIC else TelegramGlyph.VIDEO_ROUND,
                            tint = palette.primaryAccent,
                            size = 22.dp,
                        )
                    }
                }
            }
        }

        // 5. Emoji / Sticker Packs / GIF Picker Panel
        AnimatedVisibility(visible = showEmojiStickerPanel) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(250.dp)
                    .background(palette.elevatedSurface),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    listOf("😀 Emoji", "🎨 贴纸包", "🎬 GIF 动图").forEachIndexed { idx, tabLabel ->
                        Box(
                            modifier = Modifier
                                .clip(RoundedCornerShape(12.dp))
                                .background(if (emojiPanelTab == idx) palette.primaryAccent.copy(alpha = 0.18f) else Color.Transparent)
                                .clickable { emojiPanelTab = idx }
                                .padding(horizontal = 12.dp, vertical = 5.dp),
                        ) {
                            Text(
                                tabLabel,
                                color = if (emojiPanelTab == idx) palette.primaryAccent else palette.secondaryText,
                                fontSize = 13.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
                TelegramHairlineDivider()
                when (emojiPanelTab) {
                    0 -> {
                        LazyVerticalGrid(
                            columns = GridCells.Fixed(8),
                            contentPadding = PaddingValues(8.dp),
                        ) {
                            items(QuickEmojiPalette) { emoji ->
                                Box(
                                    modifier = Modifier
                                        .size(40.dp)
                                        .clip(CircleShape)
                                        .clickable {
                                            val next = tfv.text + emoji
                                            tfv = TextFieldValue(next, TextRange(next.length))
                                            onDraftChange(next)
                                        },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(emoji, fontSize = 22.sp)
                                }
                            }
                        }
                    }
                    1 -> {
                        LazyColumn(contentPadding = PaddingValues(10.dp)) {
                            items(stickerPacks, key = { it.id }) { pack ->
                                Text(pack.title, color = palette.secondaryText, fontSize = 12.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(vertical = 4.dp))
                                LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                    items(pack.stickers) { stk ->
                                        Box(
                                            modifier = Modifier
                                                .size(56.dp)
                                                .clip(RoundedCornerShape(12.dp))
                                                .background(palette.surfaceColor)
                                                .clickable {
                                                    onSendSticker(stk, pack.title)
                                                    showEmojiStickerPanel = false
                                                },
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            Text(stk, fontSize = 30.sp)
                                        }
                                    }
                                }
                                Spacer(Modifier.height(8.dp))
                            }
                        }
                    }
                    else -> {
                        LazyVerticalGrid(
                            columns = GridCells.Fixed(2),
                            contentPadding = PaddingValues(10.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            items(gifCatalog, key = { it.id }) { gif ->
                                Box(
                                    modifier = Modifier
                                        .height(78.dp)
                                        .clip(RoundedCornerShape(14.dp))
                                        .background(Color(gif.accentHex.toInt()).copy(alpha = 0.20f))
                                        .border(1.dp, Color(gif.accentHex.toInt()).copy(alpha = 0.45f), RoundedCornerShape(14.dp))
                                        .clickable {
                                            onSendGif(gif)
                                            showEmojiStickerPanel = false
                                        }
                                        .padding(10.dp),
                                ) {
                                    Column {
                                        Text(gif.previewGlyph, fontSize = 24.sp)
                                        Spacer(Modifier.height(4.dp))
                                        Text(gif.title, color = palette.primaryText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Attachment Bottom Sheet
    TelegramBottomSheet(visible = showAttachmentSheet, onDismiss = { showAttachmentSheet = false }) {
        Text("发送附件与富媒体", color = palette.primaryText, fontSize = 17.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
        LazyVerticalGrid(
            columns = GridCells.Fixed(4),
            modifier = Modifier.fillMaxWidth().height(196.dp),
            contentPadding = PaddingValues(12.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            val itemsList = listOf(
                Triple("🖼", "相册图片") { showAttachmentSheet = false; mediaPicker.launch("image/*") },
                Triple("📄", "文件文档") { showAttachmentSheet = false; mediaPicker.launch("*/*") },
                Triple("📊", "投票/测验") { showAttachmentSheet = false; showPollCreatorDialog = true },
                Triple("📍", "实时位置") {
                    showAttachmentSheet = false
                    locPermLauncher.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
                },
                Triple("👤", "联系人") { showAttachmentSheet = false; showContactDialog = true },
                Triple("🎙", "快捷语音") {
                    showAttachmentSheet = false
                    onStartRecording()
                },
                Triple("🪷", "全球法布施") { showAttachmentSheet = false; onOpenGlobalDharmaMiniApp() },
                Triple("🤖", "Mika 操控") { showAttachmentSheet = false; onPromptMikaTask() },
            )
            items(itemsList) { (emoji, title, action) ->
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    modifier = Modifier.clickable(onClick = action),
                ) {
                    Box(
                        modifier = Modifier
                            .size(50.dp)
                            .clip(CircleShape)
                            .background(palette.primaryAccent.copy(alpha = 0.14f)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(emoji, fontSize = 22.sp)
                    }
                    Spacer(Modifier.height(6.dp))
                    Text(title, color = palette.primaryText, fontSize = 12.sp, maxLines = 1)
                }
            }
        }
    }

    if (showLinkDialog) {
        var urlInput by remember { mutableStateOf("https://") }
        AlertDialog(
            onDismissRequest = { showLinkDialog = false },
            title = { Text("插入超链接 (UTF-16 text_url)") },
            text = {
                OutlinedTextField(value = urlInput, onValueChange = { urlInput = it }, label = { Text("目标 URL") }, singleLine = true)
            },
            confirmButton = {
                Button(onClick = {
                    val next = TelegramEntitiesEngine.wrapDraftSelectionWithMarkdown(
                        draft = tfv.text,
                        selectionStart = tfv.selection.min,
                        selectionEnd = tfv.selection.max,
                        type = MessageEntityType.TEXT_URL,
                        url = urlInput.trim(),
                    )
                    tfv = TextFieldValue(next, TextRange(next.length))
                    onDraftChange(next)
                    showLinkDialog = false
                }) { Text("确定") }
            },
            dismissButton = { TextButton(onClick = { showLinkDialog = false }) { Text("取消") } },
        )
    }

    if (showPollCreatorDialog) {
        PollAndQuizCreatorDialog(
            onDismiss = { showPollCreatorDialog = false },
            onCreate = { q, opts, anon, multi, quiz, correctIdx, exp ->
                onSendPollOrQuiz(q, opts, anon, multi, quiz, correctIdx, exp)
                showPollCreatorDialog = false
            },
        )
    }

    if (showContactDialog) {
        var cName by remember { mutableStateOf("觉行居士") }
        var cPhone by remember { mutableStateOf("+86 139 1080 2026") }
        AlertDialog(
            onDismissRequest = { showContactDialog = false },
            title = { Text("发送联系人名片") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = cName, onValueChange = { cName = it }, label = { Text("姓名") }, singleLine = true)
                    OutlinedTextField(value = cPhone, onValueChange = { cPhone = it }, label = { Text("电话号码") }, singleLine = true)
                }
            },
            confirmButton = {
                Button(onClick = {
                    onSendContact(cName, cPhone)
                    showContactDialog = false
                }) { Text("发送") }
            },
            dismissButton = { TextButton(onClick = { showContactDialog = false }) { Text("取消") } },
        )
    }
}

@Composable
private fun FormatChip(label: String, onClick: () -> Unit) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(8.dp),
        color = palette.surfaceColor,
        modifier = Modifier.clickable(onClick = onClick),
        shadowElevation = 1.dp,
    ) {
        Text(
            text = label,
            color = palette.primaryText,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
        )
    }
}

@Composable
private fun PollAndQuizCreatorDialog(
    onDismiss: () -> Unit,
    onCreate: (String, List<String>, Boolean, Boolean, Boolean, Int, String?) -> Unit,
) {
    var question by remember { mutableStateOf("") }
    var opt1 by remember { mutableStateOf("") }
    var opt2 by remember { mutableStateOf("") }
    var opt3 by remember { mutableStateOf("") }
    var anonymous by remember { mutableStateOf(true) }
    var multiple by remember { mutableStateOf(false) }
    var quizMode by remember { mutableStateOf(false) }
    var correctIdx by remember { mutableIntStateOf(0) }
    var explanation by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("创建投票或测验问答 (Poll / Quiz)") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = question, onValueChange = { question = it }, label = { Text("投票问题") }, singleLine = true)
                OutlinedTextField(value = opt1, onValueChange = { opt1 = it }, label = { Text("选项 1") }, singleLine = true)
                OutlinedTextField(value = opt2, onValueChange = { opt2 = it }, label = { Text("选项 2") }, singleLine = true)
                OutlinedTextField(value = opt3, onValueChange = { opt3 = it }, label = { Text("选项 3 (可选)") }, singleLine = true)
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    TextButton(onClick = { anonymous = !anonymous }) { Text(if (anonymous) "☑️ 匿名" else "⬜ 匿名") }
                    TextButton(onClick = { multiple = !multiple; if (multiple) quizMode = false }) { Text(if (multiple) "☑️ 多选" else "⬜ 多选") }
                    TextButton(onClick = { quizMode = !quizMode; if (quizMode) multiple = false }) { Text(if (quizMode) "☑️ 测验" else "⬜ 测验") }
                }
                if (quizMode) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("正确答案:", fontSize = 12.sp)
                        listOf(0, 1, 2).forEach { idx ->
                            TextButton(onClick = { correctIdx = idx }) {
                                Text(if (correctIdx == idx) "● #${idx + 1}" else "#${idx + 1}")
                            }
                        }
                    }
                    OutlinedTextField(value = explanation, onValueChange = { explanation = it }, label = { Text("测验答案解析说明") }, singleLine = true)
                }
            }
        },
        confirmButton = {
            Button(onClick = {
                onCreate(question, listOf(opt1, opt2, opt3), anonymous, multiple, quizMode, correctIdx, explanation)
            }) { Text("发送投票") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@SuppressLint("MissingPermission")
private fun fetchCurrentOrDefaultLocation(
    context: Context,
    onResult: (Double, Double, String) -> Unit,
) {
    val fineGranted = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
    val coarseGranted = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
    if (!fineGranted && !coarseGranted) {
        onResult(30.2741, 120.1551, "共享位置 · 灵隐禅境坐标")
        return
    }
    val lm = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
    val provider = when {
        lm?.isProviderEnabled(LocationManager.GPS_PROVIDER) == true -> LocationManager.GPS_PROVIDER
        lm?.isProviderEnabled(LocationManager.NETWORK_PROVIDER) == true -> LocationManager.NETWORK_PROVIDER
        else -> null
    }
    if (lm == null || provider == null) {
        onResult(30.2741, 120.1551, "共享位置 · 当前设备坐标")
        return
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        lm.getCurrentLocation(provider, CancellationSignal(), context.mainExecutor) { loc ->
            if (loc != null) onResult(loc.latitude, loc.longitude, "GPS 实时位置")
            else onResult(30.2741, 120.1551, "共享位置")
        }
    } else {
        @Suppress("DEPRECATION")
        val loc = lm.getLastKnownLocation(provider)
        if (loc != null) onResult(loc.latitude, loc.longitude, "实时位置")
        else onResult(30.2741, 120.1551, "共享位置")
    }
}
