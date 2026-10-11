package com.ombhrum.fabushi

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Android/Compose transcript boundary corresponding to Grok workspace/transcript.tsx.
 *
 * Message rendering and transcript gestures live here; conversation state/mutations remain in the
 * presentation model and are injected as callbacks. This prevents ConversationDetail from owning
 * renderer implementation details.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ConversationTranscript(
    conversationTitle: String,
    messages: List<ChatMessage>,
    searchQuery: String,
    currentFindMessageId: String? = null,
    playingVoiceMessageId: String?,
    onPlayVoice: (ChatMessage) -> Unit,
    onOpenMedia: (ChatMessage) -> Unit,
    onVotePoll: (String, List<String>) -> Unit,
    onSelectMessage: (ChatMessage) -> Unit,
    onReply: (ChatMessage) -> Unit,
    modifier: Modifier = Modifier,
) {
    val haptics = LocalHapticFeedback.current
    val listState = rememberLazyListState()
    val visibleMessages = messages

    LaunchedEffect(currentFindMessageId, messages) {
        val index = currentFindMessageId?.let { id ->
            messages.indexOfFirst { it.id == id }
        } ?: -1
        if (index >= 0) listState.animateScrollToItem(index)
    }

    LazyColumn(
        state = listState,
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (messages.isEmpty()) {
            item {
                Text(
                    "开始与 $conversationTitle 对话",
                    color = homeSecondaryText,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 72.dp),
                )
            }
        }
        items(visibleMessages, key = { it.id }) { message ->
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = if (message.outgoing) Arrangement.End else Arrangement.Start,
            ) {
                Column(
                    Modifier
                        .fillMaxWidth(0.78f)
                        .background(
                            if (message.outgoing) homeAccent.copy(alpha = 0.18f) else homeSurface,
                            RoundedCornerShape(16.dp),
                        )
                        .then(
                            if (currentFindMessageId == message.id && searchQuery.isNotBlank()) {
                                Modifier.border(1.dp, homeAccent, RoundedCornerShape(16.dp))
                            } else {
                                Modifier
                            },
                        )
                        .pointerInput(message.id) {
                            var horizontalDrag = 0f
                            detectHorizontalDragGestures(
                                onDragStart = { horizontalDrag = 0f },
                                onHorizontalDrag = { change, dragAmount ->
                                    horizontalDrag += dragAmount
                                    change.consume()
                                },
                                onDragEnd = {
                                    if (horizontalDrag > 58.dp.toPx()) {
                                        onReply(message)
                                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                    }
                                    horizontalDrag = 0f
                                },
                                onDragCancel = { horizontalDrag = 0f },
                            )
                        }
                        .combinedClickable(
                            onClick = {},
                            onLongClick = { onSelectMessage(message) },
                        )
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                ) {
                    if (message.forwardOrigin != null) {
                        Text(
                            "↪ 转发自 ${message.forwardOrigin}",
                            color = homeAccent,
                            style = MaterialTheme.typography.bodySmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }

                    val replied = message.replyToMessageId?.let { replyId ->
                        messages.firstOrNull { it.id == replyId }
                    }
                    if (replied != null) {
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .padding(vertical = 4.dp),
                        ) {
                            Text(
                                "回复",
                                color = homeAccent,
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = FontWeight.Bold,
                            )
                            Text(
                                replied.text,
                                color = homeSecondaryText,
                                style = MaterialTheme.typography.bodySmall,
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }

                    when (message.contentType) {
                        "contact" -> Row(
                            Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(
                                Modifier
                                    .size(40.dp)
                                    .background(homeAccent, CircleShape),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    (message.contactName ?: "联").take(1),
                                    color = Color.Black,
                                    fontWeight = FontWeight.Bold,
                                )
                            }
                            Column(Modifier.padding(start = 10.dp)) {
                                Text(
                                    message.contactName ?: "联系人",
                                    color = homePrimaryText,
                                    fontWeight = FontWeight.SemiBold,
                                )
                                Text(
                                    "联系人",
                                    color = homeSecondaryText,
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }

                        "location" -> Column(
                            Modifier.fillMaxWidth(),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Text("📍 位置", color = homePrimaryText, fontWeight = FontWeight.SemiBold)
                            if (message.latitude != null && message.longitude != null) {
                                Text(
                                    "%.6f, %.6f".format(message.latitude, message.longitude),
                                    color = homeSecondaryText,
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }

                        "poll" -> Column(
                            Modifier.fillMaxWidth(),
                            verticalArrangement = Arrangement.spacedBy(5.dp),
                        ) {
                            Text(
                                message.pollQuestion ?: "投票",
                                color = homePrimaryText,
                                fontWeight = FontWeight.SemiBold,
                            )
                            message.pollOptions.forEach { option ->
                                Row(
                                    Modifier
                                        .fillMaxWidth()
                                        .clickable {
                                            val chosenIds = message.pollOptions
                                                .filter { it.chosen }
                                                .map { it.id }
                                                .toMutableSet()
                                            val next = if (message.pollMultipleAnswers) {
                                                if (option.chosen) chosenIds.remove(option.id)
                                                else chosenIds.add(option.id)
                                                chosenIds.toList()
                                            } else if (option.chosen) {
                                                emptyList()
                                            } else {
                                                listOf(option.id)
                                            }
                                            onVotePoll(message.id, next)
                                        }
                                        .padding(vertical = 3.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(if (option.chosen) "●" else "○", color = homeAccent)
                                    Text(
                                        option.text,
                                        color = homePrimaryText,
                                        modifier = Modifier
                                            .weight(1f)
                                            .padding(start = 7.dp),
                                    )
                                    Text(
                                        "${option.voterCount}",
                                        color = homeSecondaryText,
                                        style = MaterialTheme.typography.bodySmall,
                                    )
                                }
                            }
                        }

                        "voice" -> Row(
                            Modifier
                                .fillMaxWidth()
                                .clickable { onPlayVoice(message) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                if (playingVoiceMessageId == message.id) "■" else "▶",
                                color = homeAccent,
                                fontSize = 24.sp,
                            )
                            Column(Modifier.padding(start = 10.dp)) {
                                Text(
                                    "语音消息",
                                    color = homePrimaryText,
                                    fontWeight = FontWeight.Medium,
                                )
                                Text(
                                    if (playingVoiceMessageId == message.id) {
                                        "正在播放"
                                    } else {
                                        message.mediaFileName ?: "录音"
                                    },
                                    color = homeSecondaryText,
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }

                        "audio" -> Row(
                            Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("♫", color = homeAccent, fontSize = 24.sp)
                            Text(
                                message.mediaFileName ?: "音频",
                                color = homePrimaryText,
                                modifier = Modifier.padding(start = 10.dp),
                            )
                        }

                        "photo", "video", "document" -> Row(
                            Modifier
                                .fillMaxWidth()
                                .clickable { onOpenMedia(message) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                when (message.contentType) {
                                    "photo" -> "🖼"
                                    "video" -> "🎬"
                                    else -> "📎"
                                },
                                fontSize = 24.sp,
                            )
                            Column(Modifier.padding(start = 10.dp)) {
                                Text(
                                    message.mediaFileName ?: message.text,
                                    color = homePrimaryText,
                                    fontWeight = FontWeight.Medium,
                                )
                                Text(
                                    when (message.contentType) {
                                        "photo" -> "图片 · 点击查看"
                                        "video" -> "视频 · 点击播放"
                                        else -> "文件 · 点击打开"
                                    },
                                    color = homeSecondaryText,
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }

                        else -> Text(message.text, color = homePrimaryText)
                    }

                    if (message.reactions.isNotEmpty()) {
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(5.dp),
                            modifier = Modifier.padding(top = 5.dp),
                        ) {
                            message.reactions.take(5).forEach { reaction ->
                                Text(
                                    "${reaction.reaction} ${reaction.count}",
                                    color = homePrimaryText,
                                    fontSize = 11.sp,
                                    modifier = Modifier
                                        .background(
                                            if (reaction.chosenByMe) homeAccent.copy(alpha = 0.25f)
                                            else homeBorder,
                                            RoundedCornerShape(10.dp),
                                        )
                                        .padding(horizontal = 7.dp, vertical = 3.dp),
                                )
                            }
                        }
                    }

                    val check = when {
                        message.deliveryState.contains("read", true) -> "✓✓"
                        message.deliveryState.contains("deliver", true) -> "✓✓"
                        else -> "✓"
                    }
                    Text(
                        (if (message.edited) "已编辑  " else "") +
                            message.time +
                            if (message.outgoing) "  $check" else "",
                        color = if (
                            message.outgoing &&
                            message.deliveryState.contains("read", true)
                        ) homeAccent else homeSecondaryText,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.align(Alignment.End),
                    )
                }
            }
        }
    }
}
