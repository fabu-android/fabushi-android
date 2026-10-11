package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private const val REPLY_PREVIEW_LIMIT = 96

internal fun replyPreviewLabel(message: ChatMessage): String {
    val normalized = when (message.contentType) {
        "contact" -> message.contactName?.takeIf(String::isNotBlank) ?: "联系人"
        "location" -> listOfNotNull(message.latitude, message.longitude)
            .joinToString(", ")
            .ifBlank { "位置" }
        "poll" -> message.pollQuestion?.takeIf(String::isNotBlank) ?: "投票"
        "voice" -> message.mediaFileName?.takeIf(String::isNotBlank) ?: "语音消息"
        "audio" -> message.mediaFileName?.takeIf(String::isNotBlank) ?: "音频"
        "photo" -> message.mediaFileName?.takeIf(String::isNotBlank) ?: "图片"
        "video" -> message.mediaFileName?.takeIf(String::isNotBlank) ?: "视频"
        "document" -> message.mediaFileName?.takeIf(String::isNotBlank) ?: "文件"
        else -> message.text
    }.replace(Regex("\\s+"), " ").trim()
    if (normalized.length <= REPLY_PREVIEW_LIMIT) return normalized
    return normalized.take(REPLY_PREVIEW_LIMIT - 1)
        .trimEnd(' ', ':', ';', ',', '.', '!', '?', '–', '—', '-')
        .plus("…")
}

internal fun replyComposerPlaceholder(message: ChatMessage?): String = when (message?.contentType) {
    null, "text" -> if (message == null) "消息" else "回复…"
    "photo", "video", "voice", "audio" -> "回复附件…"
    "document" -> "回复文件…"
    else -> "回复…"
}

/**
 * Compact Android reply/edit context bar corresponding to Grok workspace/reply-preview.tsx.
 */
@Composable
internal fun ConversationReplyPreview(
    editingMessage: ChatMessage?,
    replyTarget: ChatMessage?,
    onClear: () -> Unit,
) {
    val target = editingMessage ?: replyTarget ?: return
    val editing = editingMessage != null
    Row(
        Modifier
            .fillMaxWidth()
            .background(homeSurface)
            .padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            if (editing) "✎" else "↩",
            color = homeAccent,
            fontSize = 20.sp,
        )
        Column(
            Modifier
                .weight(1f)
                .padding(start = 10.dp),
        ) {
            Text(
                if (editing) "编辑消息" else "回复",
                color = homeAccent,
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Bold,
            )
            Text(
                replyPreviewLabel(target),
                color = homeSecondaryText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            "×",
            color = homeSecondaryText,
            fontSize = 24.sp,
            modifier = Modifier
                .clickable(onClick = onClear)
                .padding(6.dp),
        )
    }
}
