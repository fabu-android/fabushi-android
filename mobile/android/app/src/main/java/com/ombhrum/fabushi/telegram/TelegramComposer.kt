package com.ombhrum.fabushi.telegram

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat

/**
 * Floating Apple iOS Messages Capsule Composer (`F14`, Master Spec §8.8):
 * - `min 46dp` height, `23dp` full pill radius
 * - Left `30dp` `+` button toggles HOISTED root-level attachment sheet (ZERO `TelegramBottomSheet` inside composer!)
 * - Embedded `30dp` circular `#007AFF` `↑` send button driven by `canSubmitComposer(text, attachmentCount)`
 * - Hold-to-record `28dp` mic button (`testTag("telegram-record-button")`) strictly gated by `RECORD_AUDIO` permission
 * - Honest GIF empty state, proper `.imePadding()` + `.navigationBarsPadding()`
 */
@Composable
fun TelegramComposer(
    chat: TelegramChatItem,
    draftText: String,
    replyTarget: TelegramMessageItem?,
    editTarget: TelegramMessageItem?,
    recordingState: ActiveRecordingState,
    slashCommands: List<SlashCommandSuggestion>,
    stickerPacks: List<StickerPackModel>,
    gifCatalog: List<GifItemModel>,
    onDraftChange: (String) -> Unit,
    onClearReplyOrEdit: () -> Unit,
    onSendMessage: (text: String, silent: Boolean, scheduledAtMs: Long?, overrideTtl: Int?) -> Unit,
    onSendSticker: (emoji: String, packTitle: String) -> Unit,
    onSendGif: (GifItemModel) -> Unit,
    onOpenAttachmentSheet: () -> Unit,
    onToggleRecordMode: () -> Unit = {},
    onStartRecording: () -> Unit,
    onPermissionDenied: (String) -> Unit,
    onDragRecording: (Float, Float) -> Unit,
    onCancelRecording: () -> Unit,
    onFinishRecording: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current
    val latestRecordingState by rememberUpdatedState(recordingState)

    var tfv by remember(chat.id) { mutableStateOf(TextFieldValue(draftText, TextRange(draftText.length))) }
    LaunchedEffect(draftText) {
        if (draftText != tfv.text) {
            tfv = TextFieldValue(draftText, TextRange(draftText.length))
        }
    }

    var showEmojiPanel by remember { mutableStateOf(false) }
    var emojiPanelTab by remember { mutableIntStateOf(0) } // 0=Emoji, 1=Stickers, 2=GIF

    val audioPermLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            onStartRecording()
        } else {
            onPermissionDenied(
                if (strings.isEnglish) "Microphone permission is required to record voice messages."
                else "未授予麦克风权限，无法录制语音消息",
            )
        }
    }

    fun requestOrStartRecording() {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        if (granted) {
            onStartRecording()
        } else {
            audioPermLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    val matchingCommands = remember(tfv.text, slashCommands) {
        if (tfv.text.startsWith("/") && !tfv.text.contains(" ")) {
            slashCommands.filter { it.command.startsWith(tfv.text, ignoreCase = true) }
        } else {
            emptyList()
        }
    }

    val canSend = canSubmitComposer(tfv.text, attachmentCount = 0)

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.appBackground)
            .imePadding()
            .navigationBarsPadding(),
    ) {
        TelegramHairlineDivider()

        // Slash Command Autocomplete Popup
        if (matchingCommands.isNotEmpty()) {
            Surface(
                color = palette.modalBg,
                shadowElevation = 4.dp,
                modifier = Modifier.fillMaxWidth().heightIn(max = 176.dp),
            ) {
                LazyColumn {
                    items(matchingCommands, key = { it.command }) { suggestion ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable {
                                    val inserted = "${suggestion.command} "
                                    tfv = TextFieldValue(inserted, TextRange(inserted.length))
                                    onDraftChange(inserted)
                                }
                                .padding(horizontal = 16.dp, vertical = 9.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = suggestion.command,
                                color = palette.primaryAccent,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 13.sp,
                            )
                            Spacer(Modifier.width(10.dp))
                            Text(
                                text = suggestion.description,
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        TelegramHairlineDivider(Modifier.padding(start = 16.dp))
                    }
                }
            }
        }

        // Reply / Edit Banner
        if (replyTarget != null || editTarget != null) {
            val isEdit = editTarget != null
            val target = editTarget ?: replyTarget!!
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.cardBg)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .width(3.dp)
                        .height(30.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(palette.primaryAccent),
                )
                Spacer(Modifier.width(10.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = if (isEdit) {
                            if (strings.isEnglish) "Editing Message" else "正在编辑消息"
                        } else {
                            (if (strings.isEnglish) "Reply to " else "回复 ") + target.senderName
                        },
                        color = palette.primaryAccent,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = target.text.ifBlank { target.mediaFileName ?: "[媒体消息]" },
                        color = palette.secondaryText,
                        fontSize = 12.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Box(
                    modifier = Modifier
                        .size(26.dp)
                        .clip(CircleShape)
                        .clickable(onClick = onClearReplyOrEdit),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.CLOSE, tint = palette.secondaryText, size = 14.dp)
                }
            }
        }

        // Hands-Free Locked Voice Recording Strip (finger already lifted)
        if (recordingState.isRecording && recordingState.lockedHandsFree) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.cardBg)
                    .padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(palette.destructiveRed),
                )
                Spacer(Modifier.width(8.dp))
                val elapsedSec = (recordingState.elapsedMs / 1000L).toInt()
                Text(
                    text = "%02d:%02d".format(elapsedSec / 60, elapsedSec % 60),
                    color = palette.primaryText,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                )
                Spacer(Modifier.width(12.dp))
                Text(
                    text = if (strings.isEnglish) "Recording locked" else "已锁定录音",
                    color = palette.secondaryText,
                    fontSize = 12.sp,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = onCancelRecording) {
                    Text(strings.cancel, color = palette.destructiveRed, fontSize = 13.sp)
                }
                Button(
                    onClick = onFinishRecording,
                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                    shape = RoundedCornerShape(16.dp),
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
                ) {
                    Text(if (strings.isEnglish) "Send" else "发送", fontSize = 12.sp)
                }
            }
        } else {
            // Persistent Apple iOS Messages Composer Row:
            // Child 0 (weight 1f) swaps between normal input capsule and live recording status pill;
            // Child 1 (mic gesture host) stays continuously mounted across isRecording false -> true!
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 10.dp, vertical = 7.dp)
                    .testTag("apple-messages-composer-row"),
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    modifier = Modifier.weight(1f),
                    verticalAlignment = Alignment.Bottom,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (recordingState.isRecording) {
                        // Active hold-to-record status pill (mic Box at Child 1 stays mounted!)
                        Row(
                            modifier = Modifier
                                .weight(1f)
                                .heightIn(min = 46.dp)
                                .clip(RoundedCornerShape(23.dp))
                                .background(palette.cardBg)
                                .border(0.5.dp, palette.destructiveRed.copy(alpha = 0.45f), RoundedCornerShape(23.dp))
                                .padding(horizontal = 14.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(10.dp)
                                    .clip(CircleShape)
                                    .background(palette.destructiveRed),
                            )
                            Spacer(Modifier.width(8.dp))
                            val elapsedSec = (recordingState.elapsedMs / 1000L).toInt()
                            Text(
                                text = "%02d:%02d".format(elapsedSec / 60, elapsedSec % 60),
                                color = palette.primaryText,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 14.sp,
                            )
                            Spacer(Modifier.width(10.dp))
                            Text(
                                text = if (strings.isEnglish) "Release to send · Slide left to cancel" else "松开发送 · 左滑取消",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f),
                            )
                            Box(
                                modifier = Modifier
                                    .size(24.dp)
                                    .clip(CircleShape)
                                    .clickable(onClick = onCancelRecording),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(TelegramGlyph.CLOSE, tint = palette.destructiveRed, size = 14.dp)
                            }
                        }
                    } else {
                        // Left 30dp circular '+' button -> triggers HOISTED root attachment sheet
                        Box(
                            modifier = Modifier
                                .padding(bottom = 8.dp)
                                .size(30.dp)
                                .clip(CircleShape)
                                .background(palette.inputBg)
                                .clickable {
                                    showEmojiPanel = false
                                    onOpenAttachmentSheet()
                                }
                                .testTag("telegram-attach-button"),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.PLUS, tint = palette.secondaryText, size = 16.dp)
                        }

                        // Floating 23dp Full Pill Capsule Container
                        Row(
                            modifier = Modifier
                                .weight(1f)
                                .heightIn(min = 46.dp)
                                .clip(RoundedCornerShape(23.dp))
                                .background(palette.inputBg)
                                .border(0.5.dp, palette.borderColor, RoundedCornerShape(23.dp))
                                .padding(start = 14.dp, end = 8.dp, top = 7.dp, bottom = 7.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(
                                modifier = Modifier
                                    .weight(1f)
                                    .padding(vertical = 4.dp),
                                contentAlignment = Alignment.CenterStart,
                            ) {
                                if (tfv.text.isEmpty()) {
                                    Text(
                                        text = strings.composerPlaceholder,
                                        color = palette.hintText,
                                        fontSize = 16.sp,
                                    )
                                }
                                BasicTextField(
                                    value = tfv,
                                    onValueChange = { next ->
                                        tfv = next
                                        onDraftChange(next.text)
                                    },
                                    maxLines = 5,
                                    textStyle = TextStyle(
                                        color = palette.primaryText,
                                        fontSize = 16.sp,
                                        lineHeight = 21.sp,
                                    ),
                                    cursorBrush = SolidColor(palette.searchCursorColor),
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .testTag("telegram-composer-input"),
                                )
                            }

                            Spacer(Modifier.width(4.dp))

                            // Emoji / Sticker picker toggle
                            Box(
                                modifier = Modifier
                                    .size(28.dp)
                                    .clip(CircleShape)
                                    .clickable { showEmojiPanel = !showEmojiPanel },
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(
                                    glyph = TelegramGlyph.EMOJI_SMILE,
                                    tint = if (showEmojiPanel) palette.primaryAccent else palette.secondaryText,
                                    size = 19.dp,
                                )
                            }

                            Spacer(Modifier.width(4.dp))

                            // Embedded 30dp circular `#007AFF` `↑` Send Button (driven by canSubmitComposer)
                            Box(
                                modifier = Modifier
                                    .size(30.dp)
                                    .clip(CircleShape)
                                    .background(
                                        if (canSend) palette.primaryAccent else palette.primaryAccent.copy(alpha = 0.28f),
                                    )
                                    .clickable(enabled = canSend) {
                                        val textToSend = tfv.text.trim()
                                        if (canSubmitComposer(textToSend)) {
                                            onSendMessage(textToSend, false, null, null)
                                            tfv = TextFieldValue("")
                                        }
                                    }
                                    .testTag("telegram-send-button"),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(
                                    glyph = TelegramGlyph.ARROW_UP,
                                    tint = Color.White,
                                    size = 16.dp,
                                )
                            }
                        }
                    }
                }

                // Continuously mounted Hold-to-Record Mic Gesture Host (never disposes on isRecording flip!)
                if (!canSend || recordingState.isRecording) {
                    Box(
                        modifier = Modifier
                            .padding(bottom = 5.dp)
                            .size(36.dp)
                            .clip(CircleShape)
                            .background(if (recordingState.isRecording) palette.destructiveRed else palette.inputBg)
                            .border(
                                width = if (recordingState.isRecording) 0.dp else 0.5.dp,
                                color = if (recordingState.isRecording) Color.Transparent else palette.borderColor,
                                shape = CircleShape,
                            )
                            .pointerInput(chat.id) {
                                var totalDragX = 0f
                                var totalDragY = 0f
                                detectDragGesturesAfterLongPress(
                                    onDragStart = {
                                        totalDragX = 0f
                                        totalDragY = 0f
                                        requestOrStartRecording()
                                    },
                                    onDragEnd = {
                                        if (latestRecordingState.isRecording && !latestRecordingState.lockedHandsFree) {
                                            onFinishRecording()
                                        }
                                    },
                                    onDragCancel = {
                                        if (latestRecordingState.isRecording && !latestRecordingState.lockedHandsFree) {
                                            onCancelRecording()
                                        }
                                    },
                                    onDrag = { _, dragAmount ->
                                        totalDragX += dragAmount.x
                                        totalDragY += dragAmount.y
                                        onDragRecording(totalDragX, totalDragY)
                                    },
                                )
                            }
                            .clickable {
                                if (latestRecordingState.isRecording) {
                                    onFinishRecording()
                                } else {
                                    requestOrStartRecording()
                                }
                            }
                            .testTag("telegram-record-button"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(
                            glyph = if (recordingState.mode == ComposerRecordMode.ROUND_VIDEO_NOTE) TelegramGlyph.VIDEO_ROUND else TelegramGlyph.MIC,
                            tint = if (recordingState.isRecording) Color.White else palette.secondaryText,
                            size = 18.dp,
                        )
                    }
                }
            }
        }

        // Inline Emoji / Sticker / Honest GIF Panel
        if (showEmojiPanel) {
            Surface(
                color = palette.cardBg,
                modifier = Modifier.fillMaxWidth().height(230.dp),
            ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        listOf("表情", "贴纸", "GIF").forEachIndexed { idx, label ->
                            val sel = emojiPanelTab == idx
                            Surface(
                                shape = RoundedCornerShape(8.dp),
                                color = if (sel) palette.selectedPillBackground else palette.inputBg,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(8.dp))
                                    .clickable { emojiPanelTab = idx },
                            ) {
                                Text(
                                    text = label,
                                    color = if (sel) palette.selectedPillContent else palette.primaryText,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Medium,
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 5.dp),
                                )
                            }
                        }
                        Spacer(Modifier.weight(1f))
                        Surface(
                            shape = RoundedCornerShape(8.dp),
                            color = palette.inputBg,
                            modifier = Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .clickable(onClick = onToggleRecordMode),
                        ) {
                            Text(
                                text = if (recordingState.mode == ComposerRecordMode.ROUND_VIDEO_NOTE) {
                                    if (strings.isEnglish) "Video Note" else "视频消息"
                                } else {
                                    if (strings.isEnglish) "Voice Mode" else "语音模式"
                                },
                                color = palette.primaryAccent,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Medium,
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                            )
                        }
                    }
                    when (emojiPanelTab) {
                        0 -> {
                            val emojis = listOf(
                                "😀", "😄", "😊", "🙏", "❤️", "👍", "👏", "🎉",
                                "🔥", "✨", "🤔", "😢", "😂", "🥳", "🚀", "💡",
                                "👌", "🤝", "🌿", "☀️", "🌙", "⭐", "📌", "🔔",
                            )
                            LazyVerticalGrid(
                                columns = GridCells.Fixed(8),
                                contentPadding = PaddingValues(8.dp),
                                verticalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                items(emojis) { emoji ->
                                    Box(
                                        modifier = Modifier
                                            .clip(RoundedCornerShape(8.dp))
                                            .clickable {
                                                val next = tfv.text + emoji
                                                tfv = TextFieldValue(next, TextRange(next.length))
                                                onDraftChange(next)
                                            }
                                            .padding(6.dp),
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        Text(emoji, fontSize = 22.sp)
                                    }
                                }
                            }
                        }

                        1 -> {
                            if (stickerPacks.isEmpty()) {
                                Box(Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                                    Text("暂无已安装的贴纸包", color = palette.secondaryText, fontSize = 13.sp)
                                }
                            } else {
                                val pack = stickerPacks.first()
                                LazyVerticalGrid(
                                    columns = GridCells.Fixed(5),
                                    contentPadding = PaddingValues(10.dp),
                                ) {
                                    items(pack.stickers) { sticker ->
                                        Box(
                                            modifier = Modifier
                                                .clip(RoundedCornerShape(10.dp))
                                                .clickable {
                                                    onSendSticker(sticker, pack.title)
                                                    showEmojiPanel = false
                                                }
                                                .padding(8.dp),
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            Text(sticker, fontSize = 30.sp)
                                        }
                                    }
                                }
                            }
                        }

                        else -> {
                            if (gifCatalog.isEmpty()) {
                                Box(
                                    modifier = Modifier.fillMaxWidth().weight(1f).padding(20.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(
                                        text = if (strings.isEnglish) "No GIF provider configured." else "暂未配置 GIF 动图库",
                                        color = palette.secondaryText,
                                        fontSize = 13.sp,
                                    )
                                }
                            } else {
                                LazyVerticalGrid(
                                    columns = GridCells.Fixed(3),
                                    contentPadding = PaddingValues(10.dp),
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                    verticalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    items(gifCatalog, key = { it.id }) { gif ->
                                        Surface(
                                            shape = RoundedCornerShape(10.dp),
                                            color = palette.modalBg,
                                            modifier = Modifier.clickable {
                                                onSendGif(gif)
                                                showEmojiPanel = false
                                            },
                                        ) {
                                            Text(
                                                text = gif.title,
                                                color = palette.primaryText,
                                                fontSize = 12.sp,
                                                textAlign = TextAlign.Center,
                                                modifier = Modifier.padding(12.dp),
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Hoisted Root-Level Attachment Sheet & Dialogs (Rendered inside TelegramChatScreen Root Box!)
// ============================================================================

@Composable
internal fun TelegramAttachmentRootOverlay(
    visible: Boolean,
    onDismiss: () -> Unit,
    onPickPhotoOrFile: () -> Unit,
    onOpenPollCreator: () -> Unit,
    onOpenContactDialog: () -> Unit,
    onRequestLocation: () -> Unit,
    onRequestQuickVoice: () -> Unit,
) {
    if (!visible) return
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current

    TelegramBottomSheet(visible = visible, onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .testTag("chat-root-attachment-sheet"),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = if (strings.isEnglish) "Share & Attach" else "发送附件与互动",
                color = palette.primaryText,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                AttachmentTile(
                    glyph = TelegramGlyph.MEDIA_PHOTO,
                    color = Color(0xFF007AFF),
                    label = if (strings.isEnglish) "Photo / File" else "照片/文件",
                    onClick = {
                        onDismiss()
                        onPickPhotoOrFile()
                    },
                )
                AttachmentTile(
                    glyph = TelegramGlyph.POLL_BARS,
                    color = Color(0xFFFF9500),
                    label = if (strings.isEnglish) "Poll" else "投票",
                    onClick = {
                        onDismiss()
                        onOpenPollCreator()
                    },
                )
                AttachmentTile(
                    glyph = TelegramGlyph.LOCATION_PIN,
                    color = Color(0xFF34C759),
                    label = if (strings.isEnglish) "Location" else "位置",
                    onClick = {
                        onDismiss()
                        onRequestLocation()
                    },
                )
                AttachmentTile(
                    glyph = TelegramGlyph.CONTACTS,
                    color = Color(0xFF5856D6),
                    label = if (strings.isEnglish) "Contact" else "联系人",
                    onClick = {
                        onDismiss()
                        onOpenContactDialog()
                    },
                )
                AttachmentTile(
                    glyph = TelegramGlyph.MIC,
                    color = Color(0xFFFF3B30),
                    label = if (strings.isEnglish) "Voice" else "语音",
                    onClick = {
                        onDismiss()
                        onRequestQuickVoice()
                    },
                )
            }
            Spacer(Modifier.height(4.dp))
        }
    }
}

@Composable
private fun AttachmentTile(
    glyph: TelegramGlyph,
    color: Color,
    label: String,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .clip(RoundedCornerShape(12.dp))
            .clickable(onClick = onClick)
            .padding(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(48.dp)
                .clip(CircleShape)
                .background(color),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph = glyph, tint = Color.White, size = 22.dp)
        }
        Spacer(Modifier.height(5.dp))
        Text(label, color = palette.primaryText, fontSize = 11.sp, maxLines = 1)
    }
}

@Composable
internal fun TelegramContactShareDialog(
    onDismiss: () -> Unit,
    onSendContact: (name: String, phone: String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var cName by remember { mutableStateOf("") }
    var cPhone by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = palette.modalBg,
        titleContentColor = palette.primaryText,
        textContentColor = palette.primaryText,
        title = { Text(if (strings.isEnglish) "Share Contact" else "发送联系人名片") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = cName,
                    onValueChange = { cName = it },
                    label = { Text(if (strings.isEnglish) "Name" else "姓名") },
                    singleLine = true,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedTextColor = palette.primaryText,
                        unfocusedTextColor = palette.primaryText,
                        cursorColor = palette.searchCursorColor,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = cPhone,
                    onValueChange = { cPhone = it },
                    label = { Text(if (strings.isEnglish) "Phone Number" else "电话号码") },
                    singleLine = true,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedTextColor = palette.primaryText,
                        unfocusedTextColor = palette.primaryText,
                        cursorColor = palette.searchCursorColor,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            Button(
                enabled = cName.isNotBlank() && cPhone.isNotBlank(),
                onClick = {
                    onSendContact(cName.trim(), cPhone.trim())
                    onDismiss()
                },
                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
            ) {
                Text(if (strings.isEnglish) "Send" else "发送")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(strings.cancel, color = palette.topBarActionColor)
            }
        },
    )
}

@Composable
internal fun TelegramPollCreatorDialog(
    onDismiss: () -> Unit,
    onCreatePoll: (
        question: String,
        options: List<String>,
        isAnonymous: Boolean,
        multipleAnswers: Boolean,
        isQuiz: Boolean,
        correctIndex: Int,
        explanation: String?,
    ) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var question by remember { mutableStateOf("") }
    var option1 by remember { mutableStateOf("") }
    var option2 by remember { mutableStateOf("") }
    var option3 by remember { mutableStateOf("") }
    var isAnonymous by remember { mutableStateOf(true) }
    var multipleAnswers by remember { mutableStateOf(false) }

    val options = listOf(option1.trim(), option2.trim(), option3.trim()).filter { it.isNotEmpty() }
    val canCreate = question.isNotBlank() && options.size >= 2

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = palette.modalBg,
        titleContentColor = palette.primaryText,
        textContentColor = palette.primaryText,
        title = { Text(if (strings.isEnglish) "Create Poll" else "发起投票") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val colors = OutlinedTextFieldDefaults.colors(
                    focusedTextColor = palette.primaryText,
                    unfocusedTextColor = palette.primaryText,
                    cursorColor = palette.searchCursorColor,
                )
                OutlinedTextField(value = question, onValueChange = { question = it }, label = { Text("问题") }, singleLine = true, colors = colors, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = option1, onValueChange = { option1 = it }, label = { Text("选项 1") }, singleLine = true, colors = colors, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = option2, onValueChange = { option2 = it }, label = { Text("选项 2") }, singleLine = true, colors = colors, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = option3, onValueChange = { option3 = it }, label = { Text("选项 3（可选）") }, singleLine = true, colors = colors, modifier = Modifier.fillMaxWidth())
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("匿名投票", color = palette.primaryText, fontSize = 13.sp, modifier = Modifier.weight(1f))
                    Switch(checked = isAnonymous, onCheckedChange = { isAnonymous = it })
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("允许多选", color = palette.primaryText, fontSize = 13.sp, modifier = Modifier.weight(1f))
                    Switch(checked = multipleAnswers, onCheckedChange = { multipleAnswers = it })
                }
            }
        },
        confirmButton = {
            Button(
                enabled = canCreate,
                onClick = {
                    onCreatePoll(question.trim(), options, isAnonymous, multipleAnswers, false, 0, null)
                    onDismiss()
                },
                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
            ) {
                Text(if (strings.isEnglish) "Create" else "发起")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(strings.cancel, color = palette.topBarActionColor)
            }
        },
    )
}

/**
 * Fetches real device coordinates ONLY when fine or coarse location permission is granted and a real
 * last-known fix exists. NEVER fabricates Hangzhou/Shanghai fallback coordinates!
 */
@SuppressLint("MissingPermission")
internal fun fetchRealDeviceLocationOrNull(context: Context): Pair<Double, Double>? {
    val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
    val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
    if (!fine && !coarse) return null
    val lm = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager ?: return null
    val providers = listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER, LocationManager.PASSIVE_PROVIDER)
    for (p in providers) {
        val loc = runCatching { lm.getLastKnownLocation(p) }.getOrNull()
        if (loc != null && (loc.latitude != 0.0 || loc.longitude != 0.0)) {
            return loc.latitude to loc.longitude
        }
    }
    return null
}
