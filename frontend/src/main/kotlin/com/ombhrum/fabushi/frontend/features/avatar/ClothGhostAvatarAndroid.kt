package com.ombhrum.fabushi

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.absoluteValue
import kotlin.math.sin


internal fun ghostColor(identity: String): Color {
    val palette = listOf(
        Color(0xFF00C978), Color(0xFF1685F7), Color(0xFF8A4CFF),
        Color(0xFFFF681D), Color(0xFFEE2546), Color(0xFFF9A516),
    )
    return palette[identity.hashCode().absoluteValue % palette.size]
}

internal fun avatarAnimationDurationMillis(active: Boolean): Int? =
    if (active) 1200 else null

internal enum class AgentAvatarVisualState {
    IDLE,
    THINKING,
    SEARCHING,
    WORKING,
    LOADING,
    ORBIT,
    SENDING,
}

internal enum class AgentAvatarPresentationKind { PHOTO, SHARED_ROOM, GROUP, PERSONA }

/**
 * Mirrors Desktop workspace/agent-avatar.tsx precedence without taking ownership of runtime state.
 * All inputs are immutable projection fields supplied by the canonical Agent owner.
 */
internal fun agentAvatarVisualState(
    awaitingUserResponse: Boolean?,
    currentActivityKind: String? = null,
    currentActivityVerb: String? = null,
    currentActivityTool: String? = null,
    isComposingMessage: Boolean = false,
    isRunning: Boolean,
): AgentAvatarVisualState {
    if (awaitingUserResponse != null) return AgentAvatarVisualState.IDLE
    if (currentActivityKind == "thinking") return AgentAvatarVisualState.THINKING
    if (currentActivityKind == "tool" && currentActivityTool == "SendToAgent") {
        return AgentAvatarVisualState.SENDING
    }
    when (currentActivityVerb) {
        "thinking" -> return AgentAvatarVisualState.THINKING
        "searching", "browsing", "reading", "connecting" -> return AgentAvatarVisualState.SEARCHING
        "writing", "coding", "running-commands", "on-its-computer", "on-your-computer", "working" ->
            return AgentAvatarVisualState.WORKING
        "generating" -> return AgentAvatarVisualState.LOADING
        "messaging", "waiting" -> return AgentAvatarVisualState.ORBIT
        "sending" -> return AgentAvatarVisualState.SENDING
    }
    when (currentActivityTool) {
        "WebSearch", "WebFetch" -> return AgentAvatarVisualState.SEARCHING
        "GenerateImage" -> return AgentAvatarVisualState.LOADING
        "SendToAgent", "UpdateAgent" -> return AgentAvatarVisualState.SENDING
        "Task", "Await", "CheckSubagent" -> return AgentAvatarVisualState.ORBIT
        null, "" -> Unit
        else -> if (currentActivityTool.startsWith("browser_")) {
            return AgentAvatarVisualState.SEARCHING
        } else {
            return AgentAvatarVisualState.WORKING
        }
    }
    if (isComposingMessage) return AgentAvatarVisualState.THINKING
    return if (isRunning) AgentAvatarVisualState.WORKING else AgentAvatarVisualState.IDLE
}

internal fun agentAvatarPresentationKind(bot: MobileBotSummaryAndroid): AgentAvatarPresentationKind =
    when {
        !bot.avatarDataUrl.isNullOrBlank() -> AgentAvatarPresentationKind.PHOTO
        bot.isSharedRoom -> AgentAvatarPresentationKind.SHARED_ROOM
        bot.isGroup -> AgentAvatarPresentationKind.GROUP
        else -> AgentAvatarPresentationKind.PERSONA
    }

internal fun avatarAnimationDurationMillis(state: AgentAvatarVisualState): Int? =
    if (state == AgentAvatarVisualState.IDLE) null else 1200

internal fun avatarPersonaColor(value: String?): Color? {
    val normalized = normalizeAgentAvatarColor(value) ?: return null
    val encoded = DESKTOP_AGENT_AVATAR_COLORS[normalized] ?: normalized
    return runCatching {
        val rgb = encoded.removePrefix("#").toLong(16)
        Color(0xFF000000L or rgb)
    }.getOrNull()
}

internal fun avatarPersonaShape(value: String?): androidx.compose.ui.graphics.Shape =
    when (normalizeAgentAvatarShape(value)) {
        "circle" -> CircleShape
        "square" -> RoundedCornerShape(0.dp)
        else -> RoundedCornerShape(28.dp)
    }

@Composable
fun ClothGhostAvatarAndroid(
    botId: String,
    size: Dp = 46.dp,
    active: Boolean = false,
    badge: Color? = null,
    avatarShape: String? = null,
    avatarColor: String? = null,
    modifier: Modifier = Modifier,
) {
    val animationDurationMillis = avatarAnimationDurationMillis(active)
    val phase = if (animationDurationMillis != null) {
        val transition = rememberInfiniteTransition(label = "cloth-ghost-active")
        val animatedPhase by transition.animateFloat(
            initialValue = 0f,
            targetValue = (Math.PI * 2).toFloat(),
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = animationDurationMillis),
                repeatMode = RepeatMode.Restart,
            ),
            label = "cloth-phase",
        )
        animatedPhase
    } else {
        0f
    }
    val base = avatarPersonaColor(avatarColor) ?: ghostColor(botId)
    val containerShape = avatarPersonaShape(avatarShape)
    Box(
        modifier.size(size).background(base.copy(alpha = 0.08f), containerShape),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.fillMaxSize().testTag("cloth-ghost-avatar")) {
            val w = this.size.width
            val h = this.size.height
            val lift = sin(phase) * h * 0.018f
            val drift = sin(phase * 0.7f) * w * 0.018f
            val top = h * 0.08f + lift
            val shoulder = h * 0.27f + lift
            val hem = h * 0.79f + lift
            val wave = h * 0.055f
            val left = w * 0.16f + drift
            val right = w * 0.84f + drift

            val path = Path().apply {
                moveTo(left, hem)
                lineTo(w * 0.16f, shoulder)
                cubicTo(w * 0.17f, h * 0.13f + lift, w * 0.34f, top, w * 0.50f, top)
                cubicTo(w * 0.66f, top, w * 0.83f, h * 0.13f + lift, w * 0.84f, shoulder)
                lineTo(right, hem)
                val segment = (right - left) / 3f
                for (index in 3 downTo 1) {
                    val xRight = left + segment * index
                    val xLeft = xRight - segment
                    val local = phase + index * 0.9f
                    quadraticBezierTo(
                        (xLeft + xRight) / 2f,
                        hem + h * 0.12f + sin(local) * wave,
                        xLeft,
                        hem + sin(local + 0.7f) * wave,
                    )
                }
                close()
            }
            drawPath(
                path = path,
                brush = Brush.linearGradient(
                    colors = listOf(base.copy(alpha = 0.98f), base.copy(alpha = 0.84f), base),
                    start = Offset(0f, 0f),
                    end = Offset(w, h),
                ),
            )
            drawPath(path, Color.White.copy(alpha = 0.22f), style = Stroke(width = (w * 0.012f).coerceAtLeast(0.7f)))

            val eyeY = h * 0.38f + lift + sin(phase * 0.39f) * h * 0.012f
            val gaze = sin(phase * 0.48f) * w * 0.025f
            val eyeWidth = w * 0.10f
            val eyeHeight = h * 0.23f
            drawRoundRect(
                Color.White,
                topLeft = Offset(w * 0.37f + gaze, eyeY - eyeHeight / 2f),
                size = androidx.compose.ui.geometry.Size(eyeWidth, eyeHeight),
                cornerRadius = androidx.compose.ui.geometry.CornerRadius(eyeWidth / 2f),
            )
            drawRoundRect(
                Color.White,
                topLeft = Offset(w * 0.54f + gaze, eyeY - eyeHeight / 2f),
                size = androidx.compose.ui.geometry.Size(eyeWidth, eyeHeight),
                cornerRadius = androidx.compose.ui.geometry.CornerRadius(eyeWidth / 2f),
            )
        }
        if (badge != null) {
            Box(
                Modifier.align(Alignment.TopEnd).size(size * 0.23f)
                    .background(Color.White, CircleShape).padding(2.dp).background(badge, CircleShape),
            )
        }
    }
}

private const val MAX_AVATAR_DATA_URL_CHARS = 5_600_000
private const val MAX_AVATAR_IMAGE_BYTES = 4_000_000

private fun decodeAvatarDataUrl(value: String): Bitmap? {
    if (value.length > MAX_AVATAR_DATA_URL_CHARS) return null
    val separator = value.indexOf(',')
    if (separator <= 0 || separator == value.lastIndex) return null
    val header = value.substring(0, separator).lowercase()
    if (!header.startsWith("data:image/") || !header.endsWith(";base64")) return null
    val bytes = runCatching { Base64.decode(value.substring(separator + 1), Base64.DEFAULT) }.getOrNull()
        ?: return null
    if (bytes.isEmpty() || bytes.size > MAX_AVATAR_IMAGE_BYTES) return null
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
}

@Composable
private fun AgentPhotoAvatarAndroid(
    bot: MobileBotSummaryAndroid,
    dataUrl: String,
    size: Dp,
    badge: Color?,
    active: Boolean,
    modifier: Modifier,
) {
    val bitmap = remember(dataUrl) { decodeAvatarDataUrl(dataUrl)?.asImageBitmap() }
    if (bitmap == null) {
        ClothGhostAvatarAndroid(
            botId = bot.id,
            size = size,
            active = active,
            badge = badge,
            avatarShape = bot.avatarShape,
            avatarColor = bot.avatarColor,
            modifier = modifier.testTag("agent-photo-avatar-invalid"),
        )
        return
    }
    Box(
        modifier = modifier.size(size).testTag("agent-photo-avatar"),
        contentAlignment = Alignment.Center,
    ) {
        Image(
            bitmap = bitmap,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxSize().clip(avatarPersonaShape(bot.avatarShape)),
        )
        if (badge != null) {
            Box(
                Modifier.align(Alignment.TopEnd).size(size * 0.23f)
                    .background(Color.White, CircleShape).padding(2.dp).background(badge, CircleShape),
            )
        }
    }
}

@Composable
private fun SharedRoomAvatarAndroid(
    bot: MobileBotSummaryAndroid,
    size: Dp,
    badge: Color?,
    modifier: Modifier,
) {
    val base = avatarPersonaColor(bot.avatarColor) ?: ghostColor(bot.id)
    Box(
        modifier = modifier
            .size(size)
            .background(base.copy(alpha = 0.08f), CircleShape)
            .testTag("agent-shared-room-avatar"),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val stroke = (this.size.minDimension * 0.055f).coerceAtLeast(1f)
            val radius = this.size.minDimension * 0.31f
            val center = Offset(this.size.width / 2f, this.size.height / 2f)
            drawCircle(color = base, radius = radius, center = center, style = Stroke(width = stroke))
            drawLine(
                color = base,
                start = Offset(center.x - radius, center.y),
                end = Offset(center.x + radius, center.y),
                strokeWidth = stroke,
            )
            drawLine(
                color = base,
                start = Offset(center.x, center.y - radius),
                end = Offset(center.x, center.y + radius),
                strokeWidth = stroke,
            )
        }
        if (badge != null) {
            Box(
                Modifier.align(Alignment.TopEnd).size(size * 0.23f)
                    .background(Color.White, CircleShape).padding(2.dp).background(badge, CircleShape),
            )
        }
    }
}

@Composable
internal fun AgentAvatarAndroid(
    bot: MobileBotSummaryAndroid,
    size: Dp = 46.dp,
    badge: Color? = null,
    activeOverride: Boolean? = null,
    modifier: Modifier = Modifier,
) {
    val visualState = activeOverride?.let {
        if (it) AgentAvatarVisualState.WORKING else AgentAvatarVisualState.IDLE
    } ?: agentAvatarVisualState(
        awaitingUserResponse = bot.awaitingUserResponse,
        currentActivityKind = bot.currentActivityKind,
        currentActivityVerb = bot.currentActivityVerb,
        currentActivityTool = bot.currentActivityTool,
        isComposingMessage = bot.isComposingMessage,
        isRunning = bot.isRunning,
    )
    val active = avatarAnimationDurationMillis(visualState) != null
    when (agentAvatarPresentationKind(bot)) {
        AgentAvatarPresentationKind.PHOTO -> {
            AgentPhotoAvatarAndroid(
                bot = bot,
                dataUrl = requireNotNull(bot.avatarDataUrl),
                size = size,
                badge = badge,
                active = active,
                modifier = modifier,
            )
            return
        }
        AgentAvatarPresentationKind.SHARED_ROOM -> {
            SharedRoomAvatarAndroid(bot = bot, size = size, badge = badge, modifier = modifier)
            return
        }
        AgentAvatarPresentationKind.PERSONA -> {
            ClothGhostAvatarAndroid(
                botId = bot.id,
                size = size,
                active = active,
                badge = badge,
                avatarShape = bot.avatarShape,
                avatarColor = bot.avatarColor,
                modifier = modifier,
            )
            return
        }
        AgentAvatarPresentationKind.GROUP -> Unit
    }

    val participants = bot.memberIds.ifEmpty { listOf(bot.id) }
    val visible = participants.take(if (participants.size > 4) 3 else 4)
    Box(
        modifier = modifier
            .size(size)
            .testTag("agent-group-avatar"),
    ) {
        val child = size * if (visible.size <= 1) 1f else 0.62f
        val alignments = listOf(
            Alignment.TopStart,
            Alignment.TopEnd,
            Alignment.BottomStart,
            Alignment.BottomEnd,
        )
        visible.forEachIndexed { index, memberId ->
            ClothGhostAvatarAndroid(
                botId = memberId,
                size = child,
                active = false,
                modifier = Modifier.align(alignments[index.coerceAtMost(alignments.lastIndex)]),
            )
        }
        if (participants.size > 4) {
            Text(
                "+" + (participants.size - 3),
                color = Color.White,
                fontSize = 8.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .background(Color.Black.copy(alpha = 0.72f), CircleShape)
                    .padding(horizontal = 3.dp, vertical = 2.dp)
                    .testTag("agent-group-avatar-overflow"),
            )
        }
        if (badge != null) {
            Box(
                Modifier.align(Alignment.TopEnd).size(size * 0.23f)
                    .background(Color.White, CircleShape).padding(2.dp).background(badge, CircleShape),
            )
        }
    }
}
