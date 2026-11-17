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
import kotlin.math.abs
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
        }
    }
}

private fun DrawScope.drawQrFinder(tint: Color, topLeft: Offset, side: Float, stroke: Stroke) {
    drawRoundRect(tint, topLeft, Size(side, side), CornerRadius(side * 0.18f, side * 0.18f), style = stroke)
}

private val TelegramAvatarGradients: List<Pair<Color, Color>> = listOf(
    Color(0xFF56CCF2) to Color(0xFF2F80ED),
    Color(0xFF6DD5FA) to Color(0xFF2980B9),
    Color(0xFF56AB2F) to Color(0xFFA8E063),
    Color(0xFFF2994A) to Color(0xFFF2C94C),
    Color(0xFFBB6BD9) to Color(0xFF8E44AD),
    Color(0xFFEB5757) to Color(0xFFF2994A),
    Color(0xFF00B4DB) to Color(0xFF0083B0),
)

@Composable
fun TelegramAvatar(
    idSeed: String,
    title: String,
    size: Dp = 52.dp,
    kind: ConversationKind = ConversationKind.DIRECT,
    online: Boolean = false,
    isSecret: Boolean = false,
    isCompanionGhost: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val palette = LocalTelegramPalette.current
    if (isCompanionGhost) {
        ClothGhostAvatarAndroid(
            botId = idSeed,
            size = size,
            active = online,
            badge = if (online) palette.unreadBadgeActive else null,
            modifier = modifier,
        )
        return
    }

    val pair = if (kind == ConversationKind.SAVED_MESSAGES) {
        Color(0xFF4FB6F6) to Color(0xFF2A90DC)
    } else {
        TelegramAvatarGradients[abs(idSeed.hashCode()) % TelegramAvatarGradients.size]
    }

    val initials = title
        .trim()
        .split(Regex("\\s+"))
        .filter { it.isNotBlank() }
        .let { words ->
            when {
                words.size >= 2 -> "${words[0].take(1)}${words[1].take(1)}".uppercase()
                words.size == 1 -> words[0].take(2).uppercase()
                else -> "TG"
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
        if (online) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .size(size * 0.27f)
                    .clip(CircleShape)
                    .background(Color(0xFF4ECC5E))
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
