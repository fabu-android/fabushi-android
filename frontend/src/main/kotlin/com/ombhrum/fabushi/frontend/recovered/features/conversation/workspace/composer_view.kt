package com.ombhrum.fabushi

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Android/Compose composer boundary corresponding to Grok workspace/composer.tsx.
 *
 * Android system pickers/permissions and Coordinator send mutations stay injected; this component
 * owns only composer presentation, reply/edit context, attachment menu and submit affordances.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ConversationComposer(
    draft: String,
    editingMessage: ChatMessage?,
    replyTarget: ChatMessage?,
    isRecordingVoice: Boolean,
    recordingSeconds: Int,
    voiceError: String?,
    onDraftChange: (String) -> Unit,
    onTypingChanged: (Boolean) -> Unit,
    onClearContext: () -> Unit,
    onPickAttachment: (String) -> Unit,
    onRequestLocation: () -> Unit,
    onRequestContact: () -> Unit,
    onRequestPoll: () -> Unit,
    onCancelRecording: () -> Unit,
    onFinishRecording: () -> Unit,
    onStartRecording: () -> Unit,
    onEdit: (String, String) -> Unit,
    onSend: (String, String?) -> Unit,
    onRequestSendModes: () -> Unit,
) {
    var attachmentMenuOpen by remember { mutableStateOf(false) }

    ConversationReplyPreview(
        editingMessage = editingMessage,
        replyTarget = replyTarget,
        onClear = {
            onClearContext()
            if (draft.isNotEmpty()) onDraftChange("")
        },
    )

    if (isRecordingVoice) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(homeSurface)
                .padding(horizontal = 12.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("●", color = Color.Red, fontSize = 14.sp)
            Text(
                "正在录音 ${recordingSeconds / 60}:${"%02d".format(recordingSeconds % 60)}",
                color = homePrimaryText,
                modifier = Modifier
                    .weight(1f)
                    .padding(start = 8.dp),
            )
            Text(
                "取消",
                color = Color(0xFFFF6B6B),
                modifier = Modifier
                    .combinedClickable(onClick = onCancelRecording, onLongClick = {})
                    .padding(6.dp),
            )
        }
    } else if (voiceError != null) {
        Text(
            voiceError,
            color = Color(0xFFFF6B6B),
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 4.dp),
        )
    }

    Row(
        Modifier
            .fillMaxWidth()
            .padding(8.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        Box {
            Text(
                "＋",
                color = homeSecondaryText,
                fontSize = 26.sp,
                modifier = Modifier
                    .combinedClickable(
                        onClick = { attachmentMenuOpen = true },
                        onLongClick = {},
                    )
                    .padding(8.dp)
                    .testTag("conversation-attach"),
            )
            DropdownMenu(
                expanded = attachmentMenuOpen,
                onDismissRequest = { attachmentMenuOpen = false },
                containerColor = homeSurface,
            ) {
                DropdownMenuItem(
                    text = { Text("照片", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onPickAttachment("image/*")
                    },
                )
                DropdownMenuItem(
                    text = { Text("视频", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onPickAttachment("video/*")
                    },
                )
                DropdownMenuItem(
                    text = { Text("文件", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onPickAttachment("*/*")
                    },
                )
                DropdownMenuItem(
                    text = { Text("位置", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onRequestLocation()
                    },
                )
                DropdownMenuItem(
                    text = { Text("联系人", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onRequestContact()
                    },
                )
                DropdownMenuItem(
                    text = { Text("投票", color = homePrimaryText) },
                    onClick = {
                        attachmentMenuOpen = false
                        onRequestPoll()
                    },
                )
            }
        }

        OutlinedTextField(
            value = draft,
            onValueChange = {
                onDraftChange(it)
                onTypingChanged(it.isNotBlank())
            },
            modifier = Modifier
                .weight(1f)
                .testTag("conversation-composer"),
            placeholder = {
                Text(
                    replyComposerPlaceholder(replyTarget),
                    color = homeSecondaryText,
                )
            },
            maxLines = 5,
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = homePrimaryText,
                unfocusedTextColor = homePrimaryText,
                focusedContainerColor = homeSurface,
                unfocusedContainerColor = homeSurface,
            ),
            shape = androidx.compose.foundation.shape.RoundedCornerShape(22.dp),
        )

        val hasText = draft.trim().isNotEmpty()
        Text(
            if (hasText) "➤" else if (isRecordingVoice) "■" else "●",
            color = if (isRecordingVoice) {
                Color.Red
            } else if (!hasText) {
                homeSecondaryText
            } else {
                homeAccent
            },
            fontSize = 22.sp,
            modifier = Modifier
                .combinedClickable(
                    onClick = {
                        val text = draft.trim()
                        if (text.isNotEmpty()) {
                            val edit = editingMessage
                            if (edit != null) {
                                onEdit(edit.id, text)
                            } else {
                                onSend(text, replyTarget?.id)
                            }
                            onDraftChange("")
                            onTypingChanged(false)
                            onClearContext()
                        } else if (isRecordingVoice) {
                            onFinishRecording()
                        } else {
                            onStartRecording()
                        }
                    },
                    onLongClick = {
                        if (draft.isNotBlank()) onRequestSendModes()
                    },
                )
                .padding(10.dp)
                .testTag(
                    when {
                        hasText -> "conversation-send"
                        isRecordingVoice -> "conversation-stop-voice"
                        else -> "conversation-start-voice"
                    },
                ),
        )
    }
}
