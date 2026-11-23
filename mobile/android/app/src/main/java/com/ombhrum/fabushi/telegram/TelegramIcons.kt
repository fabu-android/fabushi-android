package com.ombhrum.fabushi.telegram

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ClothGhostAvatarAndroid
import com.ombhrum.fabushi.ConversationKind
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

enum class TelegramGlyph {
    HAMBURGER,
    BACK,
    SEARCH,
    LOCK_CLOSED,
    LOCK_OPEN,
    MOON,
    SUN,
    VERIFIED,
    SECRET_SHIELD,
    MUTE,
    PIN,
    CHECK_SINGLE,
    CHECK_DOUBLE,
    CLOCK_SENDING,
    MIC,
    VIDEO_ROUND,
    PAPERCLIP,
    EMOJI_SMILE,
    STICKER,
    SEND_PLANE,
    PHONE_CALL,
    VIDEO_CALL,
    SCREEN_SHARE,
    TALK_WAVE,
    BOT_ROBOT,
    MINI_APP_GRID,
    TON_DIAMOND,
    DHARMA_WHEEL,
    HERMES_SPARK,
    MAGPIE_GATEWAY,
    CLOUD_COMPUTER,
    MIKA_PHONE,
    REPLY_ARROW,
    FORWARD_ARROW,
    FOLDER,
    ARCHIVE,
    MORE_VERT,
    CLOSE,
    PLUS,
    PENCIL,
    CAMERA,
    PLAY,
    PAUSE,
    BOOKMARK_SAVED,
    CONTACTS,
    SETTINGS_GEAR,
    QR_CODE,
    TRANSLATE,
    COPY,
    TRASH,
    CHAT_BUBBLE,
    COMPOSE_SQUARE_PEN,
    ARROW_UP,
    CHEVRON_LEFT,
    CHEVRON_RIGHT,
    CHEVRON_DOWN,
    MORE_HORIZ,
    INFO_CIRCLE,
    MEDIA_PHOTO,
    DOCUMENT_FILE,
    LINK_CHAIN,
    MUSIC_NOTE,
    BELL,
    BELL_SLASH,
    SHARE_IOS,
    SIGN_OUT,
    CHECK_CIRCLE,
    CIRCLE_OUTLINE,
    POLL_BARS,
    LOCATION_PIN,
    RETRY_REFRESH,
    PLUG_MCP,
}

@Composable
fun TelegramIcon(
    glyph: TelegramGlyph,
    tint: Color,
    size: Dp = 22.dp,
    modifier: Modifier = Modifier,
) {
    Canvas(modifier = modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val sw = (w * 0.088f).coerceAtLeast(1.8f)
        val stroke = Stroke(width = sw, cap = StrokeCap.Round, join = StrokeJoin.Round)

        when (glyph) {
            TelegramGlyph.HAMBURGER -> {
                drawLine(tint, Offset(w * 0.16f, h * 0.27f), Offset(w * 0.84f, h * 0.27f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.16f, h * 0.50f), Offset(w * 0.84f, h * 0.50f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.16f, h * 0.73f), Offset(w * 0.84f, h * 0.73f), sw, StrokeCap.Round)
            }

            TelegramGlyph.BACK -> {
                val p = Path().apply {
                    moveTo(w * 0.54f, h * 0.22f)
                    lineTo(w * 0.22f, h * 0.50f)
                    lineTo(w * 0.54f, h * 0.78f)
                }
                drawPath(p, tint, style = stroke)
                drawLine(tint, Offset(w * 0.24f, h * 0.50f), Offset(w * 0.82f, h * 0.50f), sw, StrokeCap.Round)
            }

            TelegramGlyph.SEARCH -> {
                drawCircle(tint, radius = w * 0.27f, center = Offset(w * 0.44f, h * 0.44f), style = stroke)
                drawLine(tint, Offset(w * 0.64f, h * 0.64f), Offset(w * 0.84f, h * 0.84f), sw, StrokeCap.Round)
            }

            TelegramGlyph.LOCK_CLOSED, TelegramGlyph.LOCK_OPEN -> {
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.22f, h * 0.46f),
                    size = Size(w * 0.56f, h * 0.40f),
                    cornerRadius = CornerRadius(w * 0.10f, w * 0.10f),
                    style = stroke,
                )
                val shackle = Path().apply {
                    moveTo(w * 0.33f, h * 0.46f)
                    lineTo(w * 0.33f, h * 0.31f)
                    cubicTo(w * 0.33f, h * 0.15f, w * 0.67f, h * 0.15f, w * 0.67f, h * 0.31f)
                    if (glyph == TelegramGlyph.LOCK_CLOSED) {
                        lineTo(w * 0.67f, h * 0.46f)
                    }
                }
                drawPath(shackle, tint, style = stroke)
                drawCircle(tint, radius = w * 0.055f, center = Offset(w * 0.50f, h * 0.66f))
            }

            TelegramGlyph.MOON -> {
                val p = Path().apply {
                    arcTo(Rect(w * 0.18f, h * 0.18f, w * 0.82f, h * 0.82f), 35f, 260f, false)
                    quadraticBezierTo(w * 0.42f, h * 0.52f, w * 0.76f, h * 0.68f)
                    close()
                }
                drawPath(p, tint)
            }

            TelegramGlyph.SUN -> {
                drawCircle(tint, radius = w * 0.21f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                for (i in 0 until 8) {
                    val a = i * (PI / 4.0)
                    val r1 = w * 0.31f
                    val r2 = w * 0.43f
                    drawLine(
                        tint,
                        Offset(w * 0.50f + (cos(a) * r1).toFloat(), h * 0.50f + (sin(a) * r1).toFloat()),
                        Offset(w * 0.50f + (cos(a) * r2).toFloat(), h * 0.50f + (sin(a) * r2).toFloat()),
                        sw * 0.85f,
                        StrokeCap.Round,
                    )
                }
            }

            TelegramGlyph.VERIFIED -> {
                drawCircle(tint, radius = w * 0.42f, center = Offset(w * 0.50f, h * 0.50f))
                val check = Path().apply {
                    moveTo(w * 0.31f, h * 0.52f)
                    lineTo(w * 0.45f, h * 0.65f)
                    lineTo(w * 0.70f, h * 0.37f)
                }
                drawPath(check, Color.White, style = Stroke(width = sw * 1.05f, cap = StrokeCap.Round, join = StrokeJoin.Round))
            }

            TelegramGlyph.SECRET_SHIELD -> {
                val shield = Path().apply {
                    moveTo(w * 0.50f, h * 0.14f)
                    lineTo(w * 0.80f, h * 0.27f)
                    lineTo(w * 0.80f, h * 0.52f)
                    quadraticBezierTo(w * 0.80f, h * 0.76f, w * 0.50f, h * 0.88f)
                    quadraticBezierTo(w * 0.20f, h * 0.76f, w * 0.20f, h * 0.52f)
                    lineTo(w * 0.20f, h * 0.27f)
                    close()
                }
                drawPath(shield, tint, style = stroke)
                val check = Path().apply {
                    moveTo(w * 0.36f, h * 0.52f)
                    lineTo(w * 0.46f, h * 0.62f)
                    lineTo(w * 0.65f, h * 0.41f)
                }
                drawPath(check, tint, style = stroke)
            }

            TelegramGlyph.MUTE -> {
                val speaker = Path().apply {
                    moveTo(w * 0.18f, h * 0.40f)
                    lineTo(w * 0.34f, h * 0.40f)
                    lineTo(w * 0.54f, h * 0.22f)
                    lineTo(w * 0.54f, h * 0.78f)
                    lineTo(w * 0.34f, h * 0.60f)
                    lineTo(w * 0.18f, h * 0.60f)
                    close()
                }
                drawPath(speaker, tint, style = stroke)
                drawLine(tint, Offset(w * 0.66f, h * 0.38f), Offset(w * 0.84f, h * 0.62f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.84f, h * 0.38f), Offset(w * 0.66f, h * 0.62f), sw, StrokeCap.Round)
            }

            TelegramGlyph.PIN -> {
                drawLine(tint, Offset(w * 0.32f, h * 0.22f), Offset(w * 0.68f, h * 0.22f), sw, StrokeCap.Round)
                val body = Path().apply {
                    moveTo(w * 0.37f, h * 0.22f)
                    lineTo(w * 0.34f, h * 0.54f)
                    lineTo(w * 0.66f, h * 0.54f)
                    lineTo(w * 0.63f, h * 0.22f)
                }
                drawPath(body, tint, style = stroke)
                drawLine(tint, Offset(w * 0.50f, h * 0.54f), Offset(w * 0.50f, h * 0.84f), sw, StrokeCap.Round)
            }

            TelegramGlyph.CHECK_SINGLE -> {
                val p = Path().apply {
                    moveTo(w * 0.18f, h * 0.54f)
                    lineTo(w * 0.40f, h * 0.74f)
                    lineTo(w * 0.82f, h * 0.28f)
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.CHECK_DOUBLE -> {
                val first = Path().apply {
                    moveTo(w * 0.08f, h * 0.55f)
                    lineTo(w * 0.28f, h * 0.74f)
                    lineTo(w * 0.66f, h * 0.28f)
                }
                val second = Path().apply {
                    moveTo(w * 0.34f, h * 0.55f)
                    lineTo(w * 0.52f, h * 0.74f)
                    lineTo(w * 0.92f, h * 0.28f)
                }
                drawPath(first, tint, style = stroke)
                drawPath(second, tint, style = stroke)
            }

            TelegramGlyph.CLOCK_SENDING -> {
                drawCircle(tint, radius = w * 0.36f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                drawLine(tint, Offset(w * 0.50f, h * 0.50f), Offset(w * 0.50f, h * 0.30f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.50f, h * 0.50f), Offset(w * 0.66f, h * 0.58f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MIC -> {
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.35f, h * 0.14f),
                    size = Size(w * 0.30f, h * 0.46f),
                    cornerRadius = CornerRadius(w * 0.15f, w * 0.15f),
                    style = stroke,
                )
                drawArc(
                    color = tint,
                    startAngle = 0f,
                    sweepAngle = 180f,
                    useCenter = false,
                    topLeft = Offset(w * 0.22f, h * 0.30f),
                    size = Size(w * 0.56f, h * 0.42f),
                    style = stroke,
                )
                drawLine(tint, Offset(w * 0.50f, h * 0.72f), Offset(w * 0.50f, h * 0.86f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.36f, h * 0.86f), Offset(w * 0.64f, h * 0.86f), sw, StrokeCap.Round)
            }

            TelegramGlyph.VIDEO_ROUND, TelegramGlyph.VIDEO_CALL -> {
                if (glyph == TelegramGlyph.VIDEO_ROUND) {
                    drawCircle(tint, radius = w * 0.42f, center = Offset(w * 0.50f, h * 0.50f), style = Stroke(width = sw * 0.75f))
                }
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.18f, h * 0.32f),
                    size = Size(w * 0.44f, h * 0.36f),
                    cornerRadius = CornerRadius(w * 0.08f, w * 0.08f),
                    style = stroke,
                )
                val lens = Path().apply {
                    moveTo(w * 0.62f, h * 0.43f)
                    lineTo(w * 0.82f, h * 0.32f)
                    lineTo(w * 0.82f, h * 0.68f)
                    lineTo(w * 0.62f, h * 0.57f)
                    close()
                }
                drawPath(lens, tint, style = stroke)
            }

            TelegramGlyph.PAPERCLIP -> {
                val p = Path().apply {
                    moveTo(w * 0.68f, h * 0.34f)
                    lineTo(w * 0.40f, h * 0.62f)
                    cubicTo(w * 0.30f, h * 0.72f, w * 0.44f, h * 0.86f, w * 0.54f, h * 0.76f)
                    lineTo(w * 0.80f, h * 0.50f)
                    cubicTo(w * 0.92f, h * 0.36f, w * 0.70f, h * 0.14f, w * 0.56f, h * 0.28f)
                    lineTo(w * 0.24f, h * 0.60f)
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.EMOJI_SMILE -> {
                drawCircle(tint, radius = w * 0.38f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                drawCircle(tint, radius = w * 0.045f, center = Offset(w * 0.37f, h * 0.41f))
                drawCircle(tint, radius = w * 0.045f, center = Offset(w * 0.63f, h * 0.41f))
                drawArc(
                    color = tint,
                    startAngle = 25f,
                    sweepAngle = 130f,
                    useCenter = false,
                    topLeft = Offset(w * 0.33f, h * 0.38f),
                    size = Size(w * 0.34f, h * 0.30f),
                    style = stroke,
                )
            }

            TelegramGlyph.STICKER -> {
                val p = Path().apply {
                    moveTo(w * 0.20f, h * 0.20f)
                    lineTo(w * 0.80f, h * 0.20f)
                    lineTo(w * 0.80f, h * 0.58f)
                    lineTo(w * 0.58f, h * 0.80f)
                    lineTo(w * 0.20f, h * 0.80f)
                    close()
                }
                drawPath(p, tint, style = stroke)
                drawLine(tint, Offset(w * 0.58f, h * 0.80f), Offset(w * 0.58f, h * 0.58f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.58f, h * 0.58f), Offset(w * 0.80f, h * 0.58f), sw, StrokeCap.Round)
            }

            TelegramGlyph.SEND_PLANE -> {
                val plane = Path().apply {
                    moveTo(w * 0.14f, h * 0.48f)
                    lineTo(w * 0.86f, h * 0.18f)
                    lineTo(w * 0.62f, h * 0.84f)
                    lineTo(w * 0.45f, h * 0.58f)
                    close()
                }
                drawPath(plane, tint)
            }

            TelegramGlyph.PHONE_CALL -> {
                val p = Path().apply {
                    moveTo(w * 0.24f, h * 0.20f)
                    lineTo(w * 0.38f, h * 0.20f)
                    lineTo(w * 0.44f, h * 0.38f)
                    lineTo(w * 0.35f, h * 0.47f)
                    quadraticBezierTo(w * 0.44f, h * 0.60f, w * 0.56f, h * 0.67f)
                    lineTo(w * 0.65f, h * 0.58f)
                    lineTo(w * 0.82f, h * 0.64f)
                    lineTo(w * 0.82f, h * 0.78f)
                    quadraticBezierTo(w * 0.42f, h * 0.84f, w * 0.20f, h * 0.26f)
                    close()
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.SCREEN_SHARE -> {
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.14f, h * 0.20f),
                    size = Size(w * 0.72f, h * 0.48f),
                    cornerRadius = CornerRadius(w * 0.08f, w * 0.08f),
                    style = stroke,
                )
                drawLine(tint, Offset(w * 0.50f, h * 0.56f), Offset(w * 0.50f, h * 0.32f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.39f, h * 0.42f), Offset(w * 0.50f, h * 0.31f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.61f, h * 0.42f), Offset(w * 0.50f, h * 0.31f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.32f, h * 0.80f), Offset(w * 0.68f, h * 0.80f), sw, StrokeCap.Round)
            }

            TelegramGlyph.TALK_WAVE -> {
                val heights = listOf(0.24f, 0.52f, 0.78f, 0.46f, 0.30f)
                heights.forEachIndexed { idx, ratio ->
                    val x = w * (0.20f + idx * 0.15f)
                    val half = h * ratio * 0.5f
                    drawLine(tint, Offset(x, h * 0.50f - half), Offset(x, h * 0.50f + half), sw * 1.05f, StrokeCap.Round)
                }
            }

            TelegramGlyph.BOT_ROBOT -> {
                drawLine(tint, Offset(w * 0.50f, h * 0.14f), Offset(w * 0.50f, h * 0.28f), sw, StrokeCap.Round)
                drawCircle(tint, radius = w * 0.05f, center = Offset(w * 0.50f, h * 0.13f))
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.20f, h * 0.28f),
                    size = Size(w * 0.60f, h * 0.50f),
                    cornerRadius = CornerRadius(w * 0.14f, w * 0.14f),
                    style = stroke,
                )
                drawCircle(tint, radius = w * 0.055f, center = Offset(w * 0.38f, h * 0.49f))
                drawCircle(tint, radius = w * 0.055f, center = Offset(w * 0.62f, h * 0.49f))
                drawLine(tint, Offset(w * 0.38f, h * 0.64f), Offset(w * 0.62f, h * 0.64f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MINI_APP_GRID -> {
                val r = CornerRadius(w * 0.06f, w * 0.06f)
                val box = Size(w * 0.27f, h * 0.27f)
                drawRoundRect(tint, Offset(w * 0.18f, h * 0.18f), box, r, style = stroke)
                drawRoundRect(tint, Offset(w * 0.55f, h * 0.18f), box, r, style = stroke)
                drawRoundRect(tint, Offset(w * 0.18f, h * 0.55f), box, r, style = stroke)
                drawRoundRect(tint, Offset(w * 0.55f, h * 0.55f), box, r, style = stroke)
            }

            TelegramGlyph.TON_DIAMOND -> {
                val p = Path().apply {
                    moveTo(w * 0.20f, h * 0.24f)
                    lineTo(w * 0.80f, h * 0.24f)
                    lineTo(w * 0.50f, h * 0.84f)
                    close()
                }
                drawPath(p, tint, style = stroke)
                drawLine(tint, Offset(w * 0.50f, h * 0.24f), Offset(w * 0.50f, h * 0.82f), sw * 0.9f, StrokeCap.Round)
            }

            TelegramGlyph.DHARMA_WHEEL -> {
                drawCircle(tint, radius = w * 0.36f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                drawCircle(tint, radius = w * 0.11f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                for (k in 0 until 8) {
                    val a = k * (PI / 4.0)
                    drawLine(
                        tint,
                        Offset(w * 0.50f + (cos(a) * w * 0.11f).toFloat(), h * 0.50f + (sin(a) * h * 0.11f).toFloat()),
                        Offset(w * 0.50f + (cos(a) * w * 0.42f).toFloat(), h * 0.50f + (sin(a) * h * 0.42f).toFloat()),
                        sw * 0.8f,
                        StrokeCap.Round,
                    )
                }
            }

            TelegramGlyph.HERMES_SPARK -> {
                val star = Path().apply {
                    moveTo(w * 0.50f, h * 0.12f)
                    quadraticBezierTo(w * 0.52f, h * 0.48f, w * 0.88f, h * 0.50f)
                    quadraticBezierTo(w * 0.52f, h * 0.52f, w * 0.50f, h * 0.88f)
                    quadraticBezierTo(w * 0.48f, h * 0.52f, w * 0.12f, h * 0.50f)
                    quadraticBezierTo(w * 0.48f, h * 0.48f, w * 0.50f, h * 0.12f)
                    close()
                }
                drawPath(star, tint, style = stroke)
            }

            TelegramGlyph.MAGPIE_GATEWAY -> {
                drawCircle(tint, radius = w * 0.12f, center = Offset(w * 0.22f, h * 0.50f), style = stroke)
                drawCircle(tint, radius = w * 0.10f, center = Offset(w * 0.78f, h * 0.24f), style = stroke)
                drawCircle(tint, radius = w * 0.10f, center = Offset(w * 0.78f, h * 0.76f), style = stroke)
                drawLine(tint, Offset(w * 0.34f, h * 0.45f), Offset(w * 0.68f, h * 0.28f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.34f, h * 0.55f), Offset(w * 0.68f, h * 0.72f), sw, StrokeCap.Round)
            }

            TelegramGlyph.CLOUD_COMPUTER -> {
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.16f, h * 0.20f),
                    size = Size(w * 0.68f, h * 0.44f),
                    cornerRadius = CornerRadius(w * 0.08f, w * 0.08f),
                    style = stroke,
                )
                drawLine(tint, Offset(w * 0.50f, h * 0.64f), Offset(w * 0.50f, h * 0.80f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.30f, h * 0.80f), Offset(w * 0.70f, h * 0.80f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MIKA_PHONE -> {
                drawRoundRect(
                    color = tint,
                    topLeft = Offset(w * 0.28f, h * 0.12f),
                    size = Size(w * 0.44f, h * 0.76f),
                    cornerRadius = CornerRadius(w * 0.10f, w * 0.10f),
                    style = stroke,
                )
                drawCircle(tint, radius = w * 0.06f, center = Offset(w * 0.50f, h * 0.46f))
                drawLine(tint, Offset(w * 0.42f, h * 0.76f), Offset(w * 0.58f, h * 0.76f), sw * 0.85f, StrokeCap.Round)
            }

            TelegramGlyph.REPLY_ARROW -> {
                val p = Path().apply {
                    moveTo(w * 0.44f, h * 0.24f)
                    lineTo(w * 0.18f, h * 0.46f)
                    lineTo(w * 0.44f, h * 0.68f)
                }
                drawPath(p, tint, style = stroke)
                val curve = Path().apply {
                    moveTo(w * 0.20f, h * 0.46f)
                    quadraticBezierTo(w * 0.78f, h * 0.46f, w * 0.82f, h * 0.78f)
                }
                drawPath(curve, tint, style = stroke)
            }

            TelegramGlyph.FORWARD_ARROW -> {
                val p = Path().apply {
                    moveTo(w * 0.56f, h * 0.24f)
                    lineTo(w * 0.82f, h * 0.46f)
                    lineTo(w * 0.56f, h * 0.68f)
                }
                drawPath(p, tint, style = stroke)
                val curve = Path().apply {
                    moveTo(w * 0.80f, h * 0.46f)
                    quadraticBezierTo(w * 0.22f, h * 0.46f, w * 0.18f, h * 0.78f)
                }
                drawPath(curve, tint, style = stroke)
            }

            TelegramGlyph.FOLDER -> {
                val p = Path().apply {
                    moveTo(w * 0.14f, h * 0.26f)
                    lineTo(w * 0.40f, h * 0.26f)
                    lineTo(w * 0.48f, h * 0.36f)
                    lineTo(w * 0.86f, h * 0.36f)
                    lineTo(w * 0.86f, h * 0.78f)
                    lineTo(w * 0.14f, h * 0.78f)
                    close()
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.ARCHIVE -> {
                drawRoundRect(tint, Offset(w * 0.14f, h * 0.20f), Size(w * 0.72f, h * 0.16f), CornerRadius(w * 0.05f, w * 0.05f), style = stroke)
                drawRoundRect(tint, Offset(w * 0.20f, h * 0.36f), Size(w * 0.60f, h * 0.44f), CornerRadius(w * 0.06f, w * 0.06f), style = stroke)
                drawLine(tint, Offset(w * 0.40f, h * 0.54f), Offset(w * 0.60f, h * 0.54f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MORE_VERT -> {
                drawCircle(tint, radius = w * 0.07f, center = Offset(w * 0.50f, h * 0.24f))
                drawCircle(tint, radius = w * 0.07f, center = Offset(w * 0.50f, h * 0.50f))
                drawCircle(tint, radius = w * 0.07f, center = Offset(w * 0.50f, h * 0.76f))
            }

            TelegramGlyph.CLOSE -> {
                drawLine(tint, Offset(w * 0.24f, h * 0.24f), Offset(w * 0.76f, h * 0.76f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.76f, h * 0.24f), Offset(w * 0.24f, h * 0.76f), sw, StrokeCap.Round)
            }

            TelegramGlyph.PLUS -> {
                drawLine(tint, Offset(w * 0.50f, h * 0.20f), Offset(w * 0.50f, h * 0.80f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.20f, h * 0.50f), Offset(w * 0.80f, h * 0.50f), sw, StrokeCap.Round)
            }

            TelegramGlyph.PENCIL -> {
                val p = Path().apply {
                    moveTo(w * 0.20f, h * 0.80f)
                    lineTo(w * 0.24f, h * 0.64f)
                    lineTo(w * 0.68f, h * 0.20f)
                    lineTo(w * 0.80f, h * 0.32f)
                    lineTo(w * 0.36f, h * 0.76f)
                    close()
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.CAMERA -> {
                drawRoundRect(tint, Offset(w * 0.14f, h * 0.30f), Size(w * 0.72f, h * 0.48f), CornerRadius(w * 0.10f, w * 0.10f), style = stroke)
                drawCircle(tint, radius = w * 0.14f, center = Offset(w * 0.50f, h * 0.54f), style = stroke)
                drawLine(tint, Offset(w * 0.36f, h * 0.22f), Offset(w * 0.64f, h * 0.22f), sw, StrokeCap.Round)
            }

            TelegramGlyph.PLAY -> {
                val p = Path().apply {
                    moveTo(w * 0.32f, h * 0.22f)
                    lineTo(w * 0.76f, h * 0.50f)
                    lineTo(w * 0.32f, h * 0.78f)
                    close()
                }
                drawPath(p, tint)
            }

            TelegramGlyph.PAUSE -> {
                drawLine(tint, Offset(w * 0.36f, h * 0.24f), Offset(w * 0.36f, h * 0.76f), sw * 1.35f, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.64f, h * 0.24f), Offset(w * 0.64f, h * 0.76f), sw * 1.35f, StrokeCap.Round)
            }

            TelegramGlyph.BOOKMARK_SAVED -> {
                val p = Path().apply {
                    moveTo(w * 0.26f, h * 0.18f)
                    lineTo(w * 0.74f, h * 0.18f)
                    lineTo(w * 0.74f, h * 0.84f)
                    lineTo(w * 0.50f, h * 0.66f)
                    lineTo(w * 0.26f, h * 0.84f)
                    close()
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.CONTACTS -> {
                drawCircle(tint, radius = w * 0.16f, center = Offset(w * 0.50f, h * 0.34f), style = stroke)
                drawArc(
                    tint,
                    startAngle = 180f,
                    sweepAngle = 180f,
                    useCenter = false,
                    topLeft = Offset(w * 0.20f, h * 0.56f),
                    size = Size(w * 0.60f, h * 0.36f),
                    style = stroke,
                )
            }

            TelegramGlyph.SETTINGS_GEAR -> {
                drawCircle(tint, radius = w * 0.18f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                for (k in 0 until 8) {
                    val a = k * (PI / 4.0)
                    drawLine(
                        tint,
                        Offset(w * 0.50f + (cos(a) * w * 0.24f).toFloat(), h * 0.50f + (sin(a) * h * 0.24f).toFloat()),
                        Offset(w * 0.50f + (cos(a) * w * 0.38f).toFloat(), h * 0.50f + (sin(a) * h * 0.38f).toFloat()),
                        sw * 1.15f,
                        StrokeCap.Round,
                    )
                }
            }

            TelegramGlyph.QR_CODE -> {
                drawQrFinder(tint, Offset(w * 0.16f, h * 0.16f), w * 0.26f, stroke)
                drawQrFinder(tint, Offset(w * 0.58f, h * 0.16f), w * 0.26f, stroke)
                drawQrFinder(tint, Offset(w * 0.16f, h * 0.58f), w * 0.26f, stroke)
                drawCircle(tint, radius = w * 0.06f, center = Offset(w * 0.64f, h * 0.64f))
                drawCircle(tint, radius = w * 0.06f, center = Offset(w * 0.78f, h * 0.78f))
            }

            TelegramGlyph.TRANSLATE -> {
                drawCircle(tint, radius = w * 0.36f, center = Offset(w * 0.50f, h * 0.50f), style = stroke)
                drawLine(tint, Offset(w * 0.15f, h * 0.50f), Offset(w * 0.85f, h * 0.50f), sw * 0.85f)
                drawOval(tint, Offset(w * 0.33f, h * 0.14f), Size(w * 0.34f, h * 0.72f), style = stroke)
            }

            TelegramGlyph.COPY -> {
                drawRoundRect(tint, Offset(w * 0.32f, h * 0.28f), Size(w * 0.48f, h * 0.54f), CornerRadius(w * 0.07f, w * 0.07f), style = stroke)
                val back = Path().apply {
                    moveTo(w * 0.22f, h * 0.66f)
                    lineTo(w * 0.22f, h * 0.18f)
                    lineTo(w * 0.64f, h * 0.18f)
                }
                drawPath(back, tint, style = stroke)
            }

            TelegramGlyph.TRASH -> {
                drawLine(tint, Offset(w * 0.20f, h * 0.26f), Offset(w * 0.80f, h * 0.26f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.40f, h * 0.18f), Offset(w * 0.60f, h * 0.18f), sw, StrokeCap.Round)
                drawRoundRect(tint, Offset(w * 0.26f, h * 0.26f), Size(w * 0.48f, h * 0.56f), CornerRadius(w * 0.06f, w * 0.06f), style = stroke)
            }

            TelegramGlyph.CHAT_BUBBLE -> {
                val p = Path().apply {
                    moveTo(w * 0.50f, h * 0.16f)
                    cubicTo(w * 0.76f, h * 0.16f, w * 0.88f, h * 0.30f, w * 0.88f, h * 0.48f)
                    cubicTo(w * 0.88f, h * 0.66f, w * 0.76f, h * 0.80f, w * 0.50f, h * 0.80f)
                    lineTo(w * 0.32f, h * 0.80f)
                    lineTo(w * 0.16f, h * 0.88f)
                    lineTo(w * 0.22f, h * 0.72f)
                    cubicTo(w * 0.14f, h * 0.65f, w * 0.12f, h * 0.57f, w * 0.12f, h * 0.48f)
                    cubicTo(w * 0.12f, h * 0.30f, w * 0.24f, h * 0.16f, w * 0.50f, h * 0.16f)
                    close()
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.COMPOSE_SQUARE_PEN -> {
                val box = Path().apply {
                    moveTo(w * 0.52f, h * 0.20f)
                    lineTo(w * 0.24f, h * 0.20f)
                    cubicTo(w * 0.18f, h * 0.20f, w * 0.16f, h * 0.24f, w * 0.16f, h * 0.30f)
                    lineTo(w * 0.16f, h * 0.76f)
                    cubicTo(w * 0.16f, h * 0.82f, w * 0.20f, h * 0.84f, w * 0.26f, h * 0.84f)
                    lineTo(w * 0.72f, h * 0.84f)
                    cubicTo(w * 0.78f, h * 0.84f, w * 0.80f, h * 0.80f, w * 0.80f, h * 0.74f)
                    lineTo(w * 0.80f, h * 0.48f)
                }
                drawPath(box, tint, style = stroke)
                drawLine(tint, Offset(w * 0.46f, h * 0.54f), Offset(w * 0.82f, h * 0.18f), sw, StrokeCap.Round)
                drawCircle(tint, radius = sw * 0.65f, center = Offset(w * 0.44f, h * 0.56f))
            }

            TelegramGlyph.ARROW_UP -> {
                drawLine(tint, Offset(w * 0.50f, h * 0.78f), Offset(w * 0.50f, h * 0.22f), sw * 1.15f, StrokeCap.Round)
                val head = Path().apply {
                    moveTo(w * 0.26f, h * 0.44f)
                    lineTo(w * 0.50f, h * 0.20f)
                    lineTo(w * 0.74f, h * 0.44f)
                }
                drawPath(head, tint, style = Stroke(width = sw * 1.15f, cap = StrokeCap.Round, join = StrokeJoin.Round))
            }

            TelegramGlyph.CHEVRON_LEFT -> {
                val p = Path().apply {
                    moveTo(w * 0.64f, h * 0.20f)
                    lineTo(w * 0.34f, h * 0.50f)
                    lineTo(w * 0.64f, h * 0.80f)
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.CHEVRON_RIGHT -> {
                val p = Path().apply {
                    moveTo(w * 0.36f, h * 0.20f)
                    lineTo(w * 0.66f, h * 0.50f)
                    lineTo(w * 0.36f, h * 0.80f)
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.CHEVRON_DOWN -> {
                val p = Path().apply {
                    moveTo(w * 0.22f, h * 0.36f)
                    lineTo(w * 0.50f, h * 0.66f)
                    lineTo(w * 0.78f, h * 0.36f)
                }
                drawPath(p, tint, style = stroke)
            }

            TelegramGlyph.MORE_HORIZ -> {
                val r = (w * 0.075f).coerceAtLeast(1.8f)
                drawCircle(tint, r, Offset(w * 0.24f, h * 0.50f))
                drawCircle(tint, r, Offset(w * 0.50f, h * 0.50f))
                drawCircle(tint, r, Offset(w * 0.76f, h * 0.50f))
            }

            TelegramGlyph.INFO_CIRCLE -> {
                drawCircle(tint, w * 0.36f, Offset(w * 0.50f, h * 0.50f), style = stroke)
                drawCircle(tint, sw * 0.65f, Offset(w * 0.50f, h * 0.34f))
                drawLine(tint, Offset(w * 0.50f, h * 0.46f), Offset(w * 0.50f, h * 0.68f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MEDIA_PHOTO -> {
                drawRoundRect(tint, Offset(w * 0.14f, h * 0.20f), Size(w * 0.72f, h * 0.60f), CornerRadius(w * 0.10f, w * 0.10f), style = stroke)
                drawCircle(tint, w * 0.06f, Offset(w * 0.32f, h * 0.36f))
                val hills = Path().apply {
                    moveTo(w * 0.18f, h * 0.74f)
                    lineTo(w * 0.42f, h * 0.50f)
                    lineTo(w * 0.58f, h * 0.64f)
                    lineTo(w * 0.70f, h * 0.52f)
                    lineTo(w * 0.82f, h * 0.74f)
                }
                drawPath(hills, tint, style = stroke)
            }

            TelegramGlyph.DOCUMENT_FILE -> {
                val p = Path().apply {
                    moveTo(w * 0.24f, h * 0.16f)
                    lineTo(w * 0.58f, h * 0.16f)
                    lineTo(w * 0.76f, h * 0.34f)
                    lineTo(w * 0.76f, h * 0.84f)
                    lineTo(w * 0.24f, h * 0.84f)
                    close()
                }
                drawPath(p, tint, style = stroke)
                drawLine(tint, Offset(w * 0.34f, h * 0.52f), Offset(w * 0.66f, h * 0.52f), sw * 0.9f, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.34f, h * 0.66f), Offset(w * 0.58f, h * 0.66f), sw * 0.9f, StrokeCap.Round)
            }

            TelegramGlyph.LINK_CHAIN -> {
                drawRoundRect(tint, Offset(w * 0.14f, h * 0.38f), Size(w * 0.36f, h * 0.24f), CornerRadius(w * 0.12f, w * 0.12f), style = stroke)
                drawRoundRect(tint, Offset(w * 0.50f, h * 0.38f), Size(w * 0.36f, h * 0.24f), CornerRadius(w * 0.12f, w * 0.12f), style = stroke)
                drawLine(tint, Offset(w * 0.38f, h * 0.50f), Offset(w * 0.62f, h * 0.50f), sw, StrokeCap.Round)
            }

            TelegramGlyph.MUSIC_NOTE -> {
                drawCircle(tint, w * 0.10f, Offset(w * 0.32f, h * 0.74f))
                drawCircle(tint, w * 0.10f, Offset(w * 0.68f, h * 0.66f))
                drawLine(tint, Offset(w * 0.40f, h * 0.72f), Offset(w * 0.40f, h * 0.26f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.76f, h * 0.64f), Offset(w * 0.76f, h * 0.20f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.40f, h * 0.26f), Offset(w * 0.76f, h * 0.20f), sw * 1.2f, StrokeCap.Round)
            }

            TelegramGlyph.BELL -> {
                val bell = Path().apply {
                    moveTo(w * 0.22f, h * 0.68f)
                    lineTo(w * 0.78f, h * 0.68f)
                    lineTo(w * 0.70f, h * 0.56f)
                    lineTo(w * 0.70f, h * 0.42f)
                    cubicTo(w * 0.70f, h * 0.26f, w * 0.60f, h * 0.18f, w * 0.50f, h * 0.18f)
                    cubicTo(w * 0.40f, h * 0.18f, w * 0.30f, h * 0.26f, w * 0.30f, h * 0.42f)
                    lineTo(w * 0.30f, h * 0.56f)
                    close()
                }
                drawPath(bell, tint, style = stroke)
                drawArc(tint, 0f, 180f, false, Offset(w * 0.41f, h * 0.68f), Size(w * 0.18f, h * 0.14f), style = stroke)
            }

            TelegramGlyph.BELL_SLASH -> {
                val bell = Path().apply {
                    moveTo(w * 0.24f, h * 0.68f)
                    lineTo(w * 0.76f, h * 0.68f)
                    lineTo(w * 0.70f, h * 0.56f)
                    lineTo(w * 0.70f, h * 0.42f)
                    cubicTo(w * 0.70f, h * 0.26f, w * 0.60f, h * 0.18f, w * 0.50f, h * 0.18f)
                    cubicTo(w * 0.40f, h * 0.18f, w * 0.30f, h * 0.26f, w * 0.30f, h * 0.42f)
                    lineTo(w * 0.30f, h * 0.56f)
                    close()
                }
                drawPath(bell, tint, style = stroke)
                drawLine(tint, Offset(w * 0.18f, h * 0.18f), Offset(w * 0.82f, h * 0.82f), sw, StrokeCap.Round)
            }

            TelegramGlyph.SHARE_IOS -> {
                drawLine(tint, Offset(w * 0.50f, h * 0.58f), Offset(w * 0.50f, h * 0.16f), sw, StrokeCap.Round)
                val head = Path().apply {
                    moveTo(w * 0.34f, h * 0.30f)
                    lineTo(w * 0.50f, h * 0.16f)
                    lineTo(w * 0.66f, h * 0.30f)
                }
                drawPath(head, tint, style = stroke)
                val tray = Path().apply {
                    moveTo(w * 0.32f, h * 0.42f)
                    lineTo(w * 0.20f, h * 0.42f)
                    lineTo(w * 0.20f, h * 0.82f)
                    lineTo(w * 0.80f, h * 0.82f)
                    lineTo(w * 0.80f, h * 0.42f)
                    lineTo(w * 0.68f, h * 0.42f)
                }
                drawPath(tray, tint, style = stroke)
            }

            TelegramGlyph.SIGN_OUT -> {
                val door = Path().apply {
                    moveTo(w * 0.52f, h * 0.20f)
                    lineTo(w * 0.22f, h * 0.20f)
                    lineTo(w * 0.22f, h * 0.80f)
                    lineTo(w * 0.52f, h * 0.80f)
                }
                drawPath(door, tint, style = stroke)
                drawLine(tint, Offset(w * 0.44f, h * 0.50f), Offset(w * 0.82f, h * 0.50f), sw, StrokeCap.Round)
                val arrow = Path().apply {
                    moveTo(w * 0.66f, h * 0.34f)
                    lineTo(w * 0.82f, h * 0.50f)
                    lineTo(w * 0.66f, h * 0.66f)
                }
                drawPath(arrow, tint, style = stroke)
            }

            TelegramGlyph.CHECK_CIRCLE -> {
                drawCircle(tint, w * 0.38f, Offset(w * 0.50f, h * 0.50f))
                val check = Path().apply {
                    moveTo(w * 0.32f, h * 0.50f)
                    lineTo(w * 0.45f, h * 0.63f)
                    lineTo(w * 0.70f, h * 0.37f)
                }
                drawPath(check, Color.White, style = stroke)
            }

            TelegramGlyph.CIRCLE_OUTLINE -> {
                drawCircle(tint, w * 0.36f, Offset(w * 0.50f, h * 0.50f), style = stroke)
            }

            TelegramGlyph.POLL_BARS -> {
                drawLine(tint, Offset(w * 0.24f, h * 0.78f), Offset(w * 0.24f, h * 0.46f), sw * 1.4f, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.50f, h * 0.78f), Offset(w * 0.50f, h * 0.24f), sw * 1.4f, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.76f, h * 0.78f), Offset(w * 0.76f, h * 0.38f), sw * 1.4f, StrokeCap.Round)
            }

            TelegramGlyph.LOCATION_PIN -> {
                val pin = Path().apply {
                    moveTo(w * 0.50f, h * 0.86f)
                    cubicTo(w * 0.24f, h * 0.60f, w * 0.22f, h * 0.46f, w * 0.22f, h * 0.38f)
                    cubicTo(w * 0.22f, h * 0.22f, w * 0.34f, h * 0.14f, w * 0.50f, h * 0.14f)
                    cubicTo(w * 0.66f, h * 0.14f, w * 0.78f, h * 0.22f, w * 0.78f, h * 0.38f)
                    cubicTo(w * 0.78f, h * 0.46f, w * 0.76f, h * 0.60f, w * 0.50f, h * 0.86f)
                    close()
                }
                drawPath(pin, tint, style = stroke)
                drawCircle(tint, w * 0.09f, Offset(w * 0.50f, h * 0.38f), style = stroke)
            }

            TelegramGlyph.RETRY_REFRESH -> {
                drawArc(tint, -45f, 275f, false, Offset(w * 0.18f, h * 0.18f), Size(w * 0.64f, h * 0.64f), style = stroke)
                val head = Path().apply {
                    moveTo(w * 0.70f, h * 0.14f)
                    lineTo(w * 0.78f, h * 0.30f)
                    lineTo(w * 0.60f, h * 0.30f)
                }
                drawPath(head, tint, style = stroke)
            }

            TelegramGlyph.PLUG_MCP -> {
                drawLine(tint, Offset(w * 0.36f, h * 0.16f), Offset(w * 0.36f, h * 0.38f), sw, StrokeCap.Round)
                drawLine(tint, Offset(w * 0.64f, h * 0.16f), Offset(w * 0.64f, h * 0.38f), sw, StrokeCap.Round)
                drawRoundRect(tint, Offset(w * 0.24f, h * 0.38f), Size(w * 0.52f, h * 0.28f), CornerRadius(w * 0.12f, w * 0.12f), style = stroke)
                drawLine(tint, Offset(w * 0.50f, h * 0.66f), Offset(w * 0.50f, h * 0.86f), sw, StrokeCap.Round)
            }
        }
    }
}

private fun DrawScope.drawQrFinder(tint: Color, topLeft: Offset, side: Float, stroke: Stroke) {
    drawRoundRect(tint, topLeft, Size(side, side), CornerRadius(side * 0.18f, side * 0.18f), style = stroke)
}

/** Desktop v1.6.6 canonical 9 Bot Avatar Shapes. */
val DesktopV166BotAvatarShapes: List<Pair<String, String>> = listOf(
    "blob" to "水滴云",
    "pebble" to "鹅卵石",
    "squircle" to "圆方",
    "tablet" to "胶囊",
    "wedge" to "菱盾",
    "hex" to "六边形",
    "cloud" to "云朵",
    "teardrop" to "雨滴",
    "classic" to "经典圆",
)

/** Desktop v1.6.6 canonical 11 Bot Avatar Color Palettes. */
val DesktopV166BotAvatarPalettes: List<Pair<String, Color>> = listOf(
    "black" to Color(0xFF111114),
    "brown" to Color(0xFF936439),
    "red" to Color(0xFFFF263C),
    "orange" to Color(0xFFFF6700),
    "yellow" to Color(0xFFFF9800),
    "green" to Color(0xFF00C972),
    "cyan" to Color(0xFF00BCA6),
    "blue" to Color(0xFF1084FE),
    "violet" to Color(0xFF9159FE),
    "magenta" to Color(0xFFFF309B),
    "gray" to Color(0xFF777777),
)

fun resolveBotAvatarColor(raw: String?, idSeed: String): Color {
    val key = raw?.trim()?.lowercase().orEmpty()
    if (key.isNotEmpty()) {
        DesktopV166BotAvatarPalettes.firstOrNull { it.first == key }?.let { return it.second }
        if (key.startsWith("#")) {
            val hex = key.removePrefix("#")
            val parsed = when (hex.length) {
                6 -> hex.toLongOrNull(16)?.let { Color((0xFF000000L or it).toInt()) }
                8 -> hex.toLongOrNull(16)?.let { Color(it.toInt()) }
                else -> null
            }
            if (parsed != null) return parsed
        }
    }
    return DesktopV166BotAvatarPalettes[(idSeed.hashCode() and 0x7FFFFFFF) % DesktopV166BotAvatarPalettes.size].second
}

@Composable
fun BotCustomAvatar(
    idSeed: String,
    shape: String?,
    colorName: String?,
    size: Dp = 52.dp,
    active: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val palette = LocalTelegramPalette.current
    val baseColor = resolveBotAvatarColor(colorName, idSeed)
    val lighter = Color(
        red = (baseColor.red + (1f - baseColor.red) * 0.24f).coerceIn(0f, 1f),
        green = (baseColor.green + (1f - baseColor.green) * 0.24f).coerceIn(0f, 1f),
        blue = (baseColor.blue + (1f - baseColor.blue) * 0.24f).coerceIn(0f, 1f),
        alpha = 1f,
    )
    val normalizedShape = shape?.trim()?.lowercase().orEmpty().ifBlank { "classic" }

    Box(modifier = modifier.size(size), contentAlignment = Alignment.Center) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val w = this.size.width
            val h = this.size.height
            val brush = Brush.linearGradient(listOf(lighter, baseColor), Offset(0f, 0f), Offset(w, h))
            val path = Path()
            when (normalizedShape) {
                "blob" -> {
                    path.moveTo(w * 0.50f, h * 0.08f)
                    path.cubicTo(w * 0.84f, h * 0.06f, w * 0.96f, h * 0.34f, w * 0.88f, h * 0.62f)
                    path.cubicTo(w * 0.80f, h * 0.90f, w * 0.44f, h * 0.96f, w * 0.22f, h * 0.84f)
                    path.cubicTo(w * 0.04f, h * 0.72f, w * 0.06f, h * 0.36f, w * 0.24f, h * 0.18f)
                    path.cubicTo(w * 0.32f, h * 0.10f, w * 0.40f, h * 0.08f, w * 0.50f, h * 0.08f)
                    path.close()
                    drawPath(path, brush)
                }
                "pebble" -> {
                    drawOval(brush = brush, topLeft = Offset(w * 0.06f, h * 0.14f), size = Size(w * 0.88f, h * 0.72f))
                }
                "squircle" -> {
                    drawRoundRect(
                        brush = brush,
                        topLeft = Offset(w * 0.08f, h * 0.08f),
                        size = Size(w * 0.84f, h * 0.84f),
                        cornerRadius = CornerRadius(w * 0.28f, h * 0.28f),
                    )
                }
                "tablet" -> {
                    drawRoundRect(
                        brush = brush,
                        topLeft = Offset(w * 0.16f, h * 0.06f),
                        size = Size(w * 0.68f, h * 0.88f),
                        cornerRadius = CornerRadius(w * 0.34f, w * 0.34f),
                    )
                }
                "wedge" -> {
                    path.moveTo(w * 0.50f, h * 0.08f)
                    path.quadraticBezierTo(w * 0.92f, h * 0.18f, w * 0.88f, h * 0.56f)
                    path.quadraticBezierTo(w * 0.82f, h * 0.90f, w * 0.50f, h * 0.92f)
                    path.quadraticBezierTo(w * 0.18f, h * 0.90f, w * 0.12f, h * 0.56f)
                    path.quadraticBezierTo(w * 0.08f, h * 0.18f, w * 0.50f, h * 0.08f)
                    path.close()
                    drawPath(path, brush)
                }
                "hex" -> {
                    val cx = w * 0.50f
                    val cy = h * 0.50f
                    val r = w * 0.44f
                    for (i in 0 until 6) {
                        val a = (i * 60.0 - 90.0) * PI / 180.0
                        val px = cx + (cos(a) * r).toFloat()
                        val py = cy + (sin(a) * r).toFloat()
                        if (i == 0) path.moveTo(px, py) else path.lineTo(px, py)
                    }
                    path.close()
                    drawPath(path, brush)
                }
                "cloud" -> {
                    drawCircle(brush, w * 0.26f, Offset(w * 0.34f, h * 0.54f))
                    drawCircle(brush, w * 0.28f, Offset(w * 0.50f, h * 0.40f))
                    drawCircle(brush, w * 0.25f, Offset(w * 0.68f, h * 0.54f))
                    drawRoundRect(brush, Offset(w * 0.20f, h * 0.46f), Size(w * 0.60f, h * 0.34f), CornerRadius(w * 0.16f, w * 0.16f))
                }
                "teardrop" -> {
                    path.moveTo(w * 0.50f, h * 0.08f)
                    path.cubicTo(w * 0.78f, h * 0.34f, w * 0.90f, h * 0.56f, w * 0.84f, h * 0.74f)
                    path.cubicTo(w * 0.78f, h * 0.92f, w * 0.22f, h * 0.92f, w * 0.16f, h * 0.74f)
                    path.cubicTo(w * 0.10f, h * 0.56f, w * 0.22f, h * 0.34f, w * 0.50f, h * 0.08f)
                    path.close()
                    drawPath(path, brush)
                }
                else -> {
                    // classic / circle
                    drawCircle(brush, w * 0.46f, Offset(w * 0.50f, h * 0.50f))
                }
            }

            // Expressive Muse bot eyes
            val eyeY = h * 0.48f
            val eyeR = (w * 0.07f).coerceAtLeast(2.2f)
            drawCircle(Color.White, eyeR, Offset(w * 0.38f, eyeY))
            drawCircle(Color.White, eyeR, Offset(w * 0.62f, eyeY))
            drawCircle(Color(0xFF1D1D1F), eyeR * 0.45f, Offset(w * 0.39f, eyeY))
            drawCircle(Color(0xFF1D1D1F), eyeR * 0.45f, Offset(w * 0.63f, eyeY))
        }
        if (active) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .size(size * 0.27f)
                    .clip(CircleShape)
                    .background(palette.secretGreen)
                    .border(2.dp, palette.surfaceColor, CircleShape),
            )
        }
    }
}

@Composable
fun TelegramAvatar(
    idSeed: String,
    title: String,
    size: Dp = 52.dp,
    kind: ConversationKind = ConversationKind.DIRECT,
    online: Boolean = false,
    isSecret: Boolean = false,
    isCompanionGhost: Boolean = false,
    avatarShape: String? = null,
    avatarColor: String? = null,
    isBot: Boolean = false,
    botActivityActive: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val palette = LocalTelegramPalette.current
    val hasCustomBotStyle = !avatarShape.isNullOrBlank() || !avatarColor.isNullOrBlank()
    if (hasCustomBotStyle || (isBot && !isCompanionGhost)) {
        BotCustomAvatar(
            idSeed = idSeed,
            shape = avatarShape,
            colorName = avatarColor,
            size = size,
            active = botActivityActive || online,
            modifier = modifier,
        )
        return
    }
    if (isCompanionGhost) {
        ClothGhostAvatarAndroid(
            botId = idSeed,
            size = size,
            active = botActivityActive || online,
            badge = if (botActivityActive || online) palette.secretGreen else null,
            modifier = modifier,
        )
        return
    }
    if (kind == ConversationKind.GROUP) {
        CompositeGroupAvatar(
            idSeed = idSeed,
            title = title,
            size = size,
            surfaceCutout = palette.appBackground,
            modifier = modifier,
        )
        return
    }

    val pair = if (kind == ConversationKind.SAVED_MESSAGES) {
        Color(0xFF1E86FC) to Color(0xFF126FD4)
    } else {
        DesktopV166PeerAvatarGradients[(idSeed.hashCode() and 0x7FFFFFFF) % DesktopV166PeerAvatarGradients.size]
    }

    val initials = title
        .trim()
        .split(Regex("\\s+"))
        .filter { it.isNotBlank() }
        .let { words ->
            when {
                words.size >= 2 -> "${words[0].take(1)}${words[1].take(1)}".uppercase()
                words.size == 1 -> words[0].take(2).uppercase()
                else -> "FB"
            }
        }

    Box(modifier = modifier.size(size), contentAlignment = Alignment.Center) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .clip(CircleShape)
                .background(Brush.linearGradient(listOf(pair.first, pair.second))),
            contentAlignment = Alignment.Center,
        ) {
            if (kind == ConversationKind.SAVED_MESSAGES) {
                TelegramIcon(TelegramGlyph.BOOKMARK_SAVED, tint = Color.White, size = size * 0.48f)
            } else {
                Text(
                    text = initials,
                    color = Color.White,
                    fontWeight = FontWeight.Bold,
                    fontSize = (size.value * 0.34f).sp,
                    maxLines = 1,
                )
            }
        }
        if (online || botActivityActive) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .size(size * 0.27f)
                    .clip(CircleShape)
                    .background(palette.secretGreen)
                    .border(2.dp, palette.surfaceColor, CircleShape),
            )
        } else if (isSecret) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .size(size * 0.32f)
                    .clip(CircleShape)
                    .background(palette.secretGreen)
                    .border(1.5.dp, palette.surfaceColor, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = Color.White, size = size * 0.19f)
            }
        }
    }
}

/**
 * Desktop v1.6.9 composite overlapping mini-circle avatar for group chats (`grok_bot_interaction_spec.md`).
 * Renders 3 staggered circular tiles with surface-matched cutout rings.
 */
@Composable
fun CompositeGroupAvatar(
    idSeed: String,
    title: String,
    size: Dp = 52.dp,
    surfaceCutout: Color = LocalTelegramPalette.current.appBackground,
    modifier: Modifier = Modifier,
) {
    val seedHash = idSeed.hashCode() and 0x7FFFFFFF
    val gradients = DesktopV166PeerAvatarGradients
    val p0 = gradients[seedHash % gradients.size]
    val p1 = gradients[(seedHash + 2) % gradients.size]
    val p2 = gradients[(seedHash + 4) % gradients.size]

    val words = title.trim().split(Regex("\\s+")).filter { it.isNotBlank() }
    val c0 = words.getOrNull(0)?.take(1)?.uppercase() ?: "G"
    val c1 = words.getOrNull(1)?.take(1)?.uppercase()
        ?: words.getOrNull(0)?.drop(1)?.take(1)?.uppercase()?.ifBlank { "P" }
        ?: "P"
    val c2 = words.getOrNull(2)?.take(1)?.uppercase() ?: "#"

    val tileSize = size * 0.56f
    val ringWidth = (size.value * 0.045f).coerceIn(1.2f, 2.2f).dp

    Box(modifier = modifier.size(size)) {
        // Top-left back tile
        Box(
            modifier = Modifier
                .align(Alignment.TopStart)
                .size(tileSize)
                .clip(CircleShape)
                .background(Brush.linearGradient(listOf(p1.first, p1.second)))
                .border(ringWidth, surfaceCutout, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = c1,
                color = Color.White,
                fontWeight = FontWeight.Bold,
                fontSize = (tileSize.value * 0.42f).sp,
                maxLines = 1,
            )
        }
        // Top-right middle tile
        Box(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .size(tileSize * 0.90f)
                .clip(CircleShape)
                .background(Brush.linearGradient(listOf(p2.first, p2.second)))
                .border(ringWidth, surfaceCutout, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = c2,
                color = Color.White,
                fontWeight = FontWeight.Bold,
                fontSize = (tileSize.value * 0.36f).sp,
                maxLines = 1,
            )
        }
        // Bottom-center front hero tile
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .size(tileSize * 1.04f)
                .clip(CircleShape)
                .background(Brush.linearGradient(listOf(p0.first, p0.second)))
                .border(ringWidth, surfaceCutout, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = c0,
                color = Color.White,
                fontWeight = FontWeight.Bold,
                fontSize = (tileSize.value * 0.44f).sp,
                maxLines = 1,
            )
        }
    }
}

