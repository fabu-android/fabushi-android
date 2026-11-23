package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

enum class TelegramThemePreset(val wire: String, val label: String, val isDark: Boolean) {
    APPLE_DAY("apple_day", "Apple 明亮", false),
    APPLE_NIGHT("apple_night", "Apple 深色", true),
    APPLE_TINTED("apple_tinted", "Apple 靛蓝深色", true),
    DAY_BLUE("day_blue", "经典蓝 (日间)", false),
    NIGHT("night", "深夜模式", true),
    NIGHT_TINTED("night_tinted", "深海蓝调", true),
    CLASSIC_GREEN("classic_green", "经典翠绿", false),
    CUSTOM_ACCENT("custom_accent", "自定义主题色", false),
}

enum class TelegramWallpaperStyle(val wire: String, val label: String) {
    DOODLE_CLASSIC("doodle_classic", "经典几何涂鸦"),
    OCEAN_MIST("ocean_mist", "晴空蔚蓝渐变"),
    AURORA_NIGHT("aurora_night", "极光星云深邃"),
    LOTUS_GOLD("lotus_gold", "琉璃金莲禅韵"),
    FOREST_ZEN("forest_zen", "竹林松风护眼"),
    SOLID_MINIMAL("solid_minimal", "极简纯色"),
}

/** Desktop v1.6.6 canonical 8 accent swatches. */
val DesktopV166AccentSwatches: List<Long> = listOf(
    0xFF007AFFL,
    0xFF40A7E3L,
    0xFF3FC1B0L,
    0xFF34C759L,
    0xFFE5A030L,
    0xFFE86C52L,
    0xFFE16794L,
    0xFF8544D6L,
)

/** Desktop v1.6.6 canonical 7 peer name colors per theme mode. */
val DesktopV166PeerNameColorsDay: List<Color> = listOf(
    Color(0xFFC03D33), Color(0xFF4FAD2D), Color(0xFFD09306), Color(0xFF168ACD),
    Color(0xFF8544D6), Color(0xFFCD4073), Color(0xFF2996AD),
)

val DesktopV166PeerNameColorsNight: List<Color> = listOf(
    Color(0xFFFB6169), Color(0xFF85DE85), Color(0xFFF3BC5C), Color(0xFF65BDF3),
    Color(0xFFB48BF2), Color(0xFFFF5694), Color(0xFF62D4E3),
)

val DesktopV166PeerNameColorsTinted: List<Color> = listOf(
    Color(0xFFEC7577), Color(0xFF86D67F), Color(0xFFE4C054), Color(0xFF68C7F3),
    Color(0xFFB383F3), Color(0xFFE16794), Color(0xFF57C9E0),
)

/** Desktop v1.6.6 canonical 7 peer avatar gradients (indexed by peer_id % 7). */
val DesktopV166PeerAvatarGradients: List<Pair<Color, Color>> = listOf(
    Color(0xFFFF845E) to Color(0xFFD45246),
    Color(0xFF9AD164) to Color(0xFF46BA43),
    Color(0xFFECC65B) to Color(0xFFD59F36),
    Color(0xFF5CAFFA) to Color(0xFF408ACF),
    Color(0xFFB694F9) to Color(0xFF6C61DF),
    Color(0xFFFF8AAC) to Color(0xFFD95574),
    Color(0xFF5BCBE3) to Color(0xFF359AD4),
)

fun resolvePeerNameColor(peerSeed: String, palette: TelegramPalette): Color {
    val idx = (peerSeed.hashCode() and 0x7FFFFFFF) % 7
    return when (palette.settings.preset) {
        TelegramThemePreset.APPLE_TINTED, TelegramThemePreset.NIGHT_TINTED -> DesktopV166PeerNameColorsTinted[idx]
        else -> if (palette.isDark) DesktopV166PeerNameColorsNight[idx] else DesktopV166PeerNameColorsDay[idx]
    }
}

@Immutable
data class TelegramThemeSettings(
    val preset: TelegramThemePreset = TelegramThemePreset.APPLE_DAY,
    val customAccentArgb: Long = 0xFF007AFFL,
    val customDark: Boolean = false,
    val bubbleCornerRadiusDp: Int = 18,
    val messageFontSizeSp: Int = 16,
    val wallpaperStyle: TelegramWallpaperStyle = TelegramWallpaperStyle.SOLID_MINIMAL,
    val wallpaperDimPercent: Int = 0,
    val animationsEnabled: Boolean = true,
    val autoNightMode: Boolean = false,
) {
    val boundedCornerDp: Dp get() = bubbleCornerRadiusDp.coerceIn(4, 18).dp
    val boundedFontSp: TextUnit get() = messageFontSizeSp.coerceIn(12, 22).sp
    val isDarkEffective: Boolean
        get() = when (preset) {
            TelegramThemePreset.APPLE_DAY, TelegramThemePreset.DAY_BLUE, TelegramThemePreset.CLASSIC_GREEN -> false
            TelegramThemePreset.APPLE_NIGHT, TelegramThemePreset.APPLE_TINTED, TelegramThemePreset.NIGHT, TelegramThemePreset.NIGHT_TINTED -> true
            TelegramThemePreset.CUSTOM_ACCENT -> customDark
        }
}

@Immutable
data class TelegramPalette(
    val isDark: Boolean,
    val topBarBackground: Color,
    val topBarContent: Color,
    val topBarSubtitle: Color,
    val drawerHeaderStart: Color,
    val drawerHeaderEnd: Color,
    val appBackground: Color,
    val surfaceColor: Color,
    val elevatedSurface: Color,
    val primaryAccent: Color,
    val outgoingBubble: Color,
    val incomingBubble: Color,
    val outgoingText: Color,
    val incomingText: Color,
    val outgoingMeta: Color,
    val incomingMeta: Color,
    val checkMarkColor: Color,
    val unreadBadgeActive: Color,
    val unreadBadgeMuted: Color,
    val secretGreen: Color,
    val destructiveRed: Color,
    val goldMerit: Color,
    val linkColor: Color,
    val codeBackground: Color,
    val dividerColor: Color,
    val primaryText: Color,
    val secondaryText: Color,
    val hintText: Color,
    val wallpaperStart: Color,
    val wallpaperEnd: Color,
    val doodleTint: Color,
    val settings: TelegramThemeSettings,
    val isAppleStyle: Boolean = false,
    val outgoingBubbleTop: Color = outgoingBubble,
    val outgoingBubbleBottom: Color = outgoingBubble,
    // Desktop v1.6.6 surface & interaction tokens
    val groupedBackground: Color = appBackground,
    val accentHover: Color = primaryAccent,
    val inputBg: Color = elevatedSurface,
    val cardBg: Color = surfaceColor,
    val modalBg: Color = surfaceColor,
    val hoverBg: Color = elevatedSurface,
    val borderColor: Color = dividerColor,
    val servicePillBg: Color = elevatedSurface,
    val servicePillFg: Color = secondaryText,
    val codeForeground: Color = primaryText,
    // 9 mandatory WCAG/iOS contrast tokens (TEST_INFRA.md §3.3 & §5.3) + outgoing code bg
    val outgoingCheckMarkColor: Color = checkMarkColor,
    val outgoingLinkColor: Color = linkColor,
    val outgoingReplyBarColor: Color = primaryAccent,
    val outgoingReactionTextColor: Color = outgoingText,
    val topBarActionColor: Color = if (isAppleStyle) primaryAccent else topBarContent,
    val searchCursorColor: Color = primaryAccent,
    val selectedPillBackground: Color = primaryAccent,
    val selectedPillContent: Color = Color.White,
    val unreadBadgeContent: Color = Color.White,
    val unreadBadgeMutedContent: Color = if (isDark) Color.White else Color(0xFF1D1D1F),
    val outgoingCodeBg: Color = when {
        isAppleStyle -> Color(0x33000000)
        isDark -> codeBackground
        else -> Color.Black.copy(alpha = 0.08f)
    },
    val outgoingSubCardBg: Color = when {
        isAppleStyle -> Color.Black.copy(alpha = 0.20f)
        isDark -> Color.Black.copy(alpha = 0.20f)
        else -> Color.Black.copy(alpha = 0.06f)
    },
    val outgoingSubCardSelectedBg: Color = when {
        isAppleStyle -> Color.Black.copy(alpha = 0.30f)
        isDark -> Color.Black.copy(alpha = 0.30f)
        else -> Color.Black.copy(alpha = 0.12f)
    },
    val approveActionBg: Color = Color(0xFF1A7F37),
    val approveActionContent: Color = Color.White,
    val statusGreenText: Color = if (isDark) Color(0xFF30D158) else Color(0xFF146C2E),
) {
    val outgoingBubbleBrush: Brush
        get() = Brush.verticalGradient(listOf(outgoingBubbleTop, outgoingBubbleBottom))
}

fun resolveTelegramPalette(settings: TelegramThemeSettings): TelegramPalette {
    return when (settings.preset) {
        TelegramThemePreset.APPLE_DAY -> TelegramPalette(
            isDark = false,
            topBarBackground = Color(0xFFFFFFFF),
            topBarContent = Color(0xFF1D1D1F),
            topBarSubtitle = Color(0xFF86868B),
            drawerHeaderStart = Color(0xFF2C95FF),
            drawerHeaderEnd = Color(0xFF007AFF),
            appBackground = Color(0xFFFFFFFF),
            surfaceColor = Color(0xFFFFFFFF),
            elevatedSurface = Color(0xFFF3F3F5),
            primaryAccent = Color(0xFF007AFF),
            outgoingBubble = Color(0xFF007AFF),
            incomingBubble = Color(0xFFE9E9EB),
            outgoingText = Color(0xFFFFFFFF),
            incomingText = Color(0xFF1D1D1F),
            outgoingMeta = Color(0xFFE5F2FF),
            incomingMeta = Color(0xFF8E8E93),
            checkMarkColor = Color(0xFF007AFF),
            unreadBadgeActive = Color(0xFF007AFF),
            unreadBadgeMuted = Color(0xFFC7C7CC),
            secretGreen = Color(0xFF34C759),
            destructiveRed = Color(0xFFFF3B30),
            goldMerit = Color(0xFFD99B26),
            linkColor = Color(0xFF007AFF),
            codeBackground = Color(0xFFE6E8EB),
            dividerColor = Color.Black.copy(alpha = 0.07f),
            primaryText = Color(0xFF1D1D1F),
            secondaryText = Color(0xFF86868B),
            hintText = Color(0xFFAEAEB2),
            wallpaperStart = Color(0xFFFFFFFF),
            wallpaperEnd = Color(0xFFFFFFFF),
            doodleTint = Color(0x12007AFF),
            settings = settings,
            isAppleStyle = true,
            outgoingBubbleTop = Color(0xFF2C95FF),
            outgoingBubbleBottom = Color(0xFF007AFF),
            groupedBackground = Color(0xFFF2F2F7),
            accentHover = Color(0xFF0062CC),
            inputBg = Color(0xFFF3F3F5),
            cardBg = Color(0xFFF7F7F8),
            modalBg = Color(0xFFFFFFFF),
            hoverBg = Color(0xFFE8E8ED),
            borderColor = Color(0xFFE5E5EA),
            servicePillBg = Color(0xFFF2F2F7),
            servicePillFg = Color(0xFF8E8E93),
            codeForeground = Color(0xFF1D1D1F),
            outgoingCheckMarkColor = Color(0xFFFFFFFF),
            outgoingLinkColor = Color(0xFFFFFFFF),
            outgoingReplyBarColor = Color(0xFFFFFFFF),
            outgoingReactionTextColor = Color(0xFFFFFFFF),
            topBarActionColor = Color(0xFF007AFF),
            searchCursorColor = Color(0xFF007AFF),
            selectedPillBackground = Color(0xFF007AFF),
            selectedPillContent = Color(0xFFFFFFFF),
            unreadBadgeContent = Color(0xFFFFFFFF),
            unreadBadgeMutedContent = Color(0xFF1D1D1F),
            outgoingCodeBg = Color(0x33000000),
            outgoingSubCardBg = Color.Black.copy(alpha = 0.20f),
            outgoingSubCardSelectedBg = Color.Black.copy(alpha = 0.30f),
            approveActionBg = Color(0xFF1A7F37),
            approveActionContent = Color.White,
            statusGreenText = Color(0xFF146C2E),
        )

        TelegramThemePreset.APPLE_NIGHT -> TelegramPalette(
            isDark = true,
            topBarBackground = Color(0xFF1E1E1E),
            topBarContent = Color(0xFFF5F5F7),
            topBarSubtitle = Color(0xFF98989D),
            drawerHeaderStart = Color(0xFF1E8FFF),
            drawerHeaderEnd = Color(0xFF0A84FF),
            appBackground = Color(0xFF1E1E1E),
            surfaceColor = Color(0xFF242426),
            elevatedSurface = Color(0xFF2A2A2D),
            primaryAccent = Color(0xFF0A84FF),
            outgoingBubble = Color(0xFF0A84FF),
            incomingBubble = Color(0xFF262629),
            outgoingText = Color(0xFFFFFFFF),
            incomingText = Color(0xFFF5F5F7),
            outgoingMeta = Color(0xFFD6EBFF),
            incomingMeta = Color(0xFF8E8E93),
            checkMarkColor = Color(0xFF0A84FF),
            unreadBadgeActive = Color(0xFF0A84FF),
            unreadBadgeMuted = Color(0xFF48484A),
            secretGreen = Color(0xFF30D158),
            destructiveRed = Color(0xFFFF453A),
            goldMerit = Color(0xFFF5B841),
            linkColor = Color(0xFF409CFF),
            codeBackground = Color(0xFF2C2C2E),
            dividerColor = Color.White.copy(alpha = 0.08f),
            primaryText = Color(0xFFF5F5F7),
            secondaryText = Color(0xFF98989D),
            hintText = Color(0xFF636366),
            wallpaperStart = Color(0xFF1E1E1E),
            wallpaperEnd = Color(0xFF1E1E1E),
            doodleTint = Color(0x160A84FF),
            settings = settings,
            isAppleStyle = true,
            outgoingBubbleTop = Color(0xFF1E8FFF),
            outgoingBubbleBottom = Color(0xFF0A84FF),
            groupedBackground = Color(0xFF000000),
            accentHover = Color(0xFF2994FF),
            inputBg = Color(0xFF2A2A2D),
            cardBg = Color(0xFF262629),
            modalBg = Color(0xFF242426),
            hoverBg = Color(0xFF2C2C2E),
            borderColor = Color(0xFF38383A),
            servicePillBg = Color(0xFF2C2C2E),
            servicePillFg = Color(0xFF8E8E93),
            codeForeground = Color(0xFFE5E5EA),
            outgoingCheckMarkColor = Color(0xFFFFFFFF),
            outgoingLinkColor = Color(0xFFFFFFFF),
            outgoingReplyBarColor = Color(0xFFFFFFFF),
            outgoingReactionTextColor = Color(0xFFFFFFFF),
            topBarActionColor = Color(0xFF0A84FF),
            searchCursorColor = Color(0xFF0A84FF),
            selectedPillBackground = Color(0xFF0A84FF),
            selectedPillContent = Color(0xFFFFFFFF),
            unreadBadgeContent = Color(0xFFFFFFFF),
            unreadBadgeMutedContent = Color(0xFFFFFFFF),
            outgoingCodeBg = Color(0x33000000),
            outgoingSubCardBg = Color.Black.copy(alpha = 0.20f),
            outgoingSubCardSelectedBg = Color.Black.copy(alpha = 0.30f),
            approveActionBg = Color(0xFF1A7F37),
            approveActionContent = Color.White,
            statusGreenText = Color(0xFF30D158),
        )

        TelegramThemePreset.APPLE_TINTED -> TelegramPalette(
            isDark = true,
            topBarBackground = Color(0xFF181C22),
            topBarContent = Color(0xFFF5F7FA),
            topBarSubtitle = Color(0xFF949DA8),
            drawerHeaderStart = Color(0xFF1E8FFF),
            drawerHeaderEnd = Color(0xFF0A84FF),
            appBackground = Color(0xFF181C22),
            surfaceColor = Color(0xFF1E232B),
            elevatedSurface = Color(0xFF242A33),
            primaryAccent = Color(0xFF0A84FF),
            outgoingBubble = Color(0xFF0A84FF),
            incomingBubble = Color(0xFF242A33),
            outgoingText = Color(0xFFFFFFFF),
            incomingText = Color(0xFFF5F7FA),
            outgoingMeta = Color(0xFFD6EBFF),
            incomingMeta = Color(0xFF8E96A0),
            checkMarkColor = Color(0xFF0A84FF),
            unreadBadgeActive = Color(0xFF0A84FF),
            unreadBadgeMuted = Color(0xFF444D5A),
            secretGreen = Color(0xFF30D158),
            destructiveRed = Color(0xFFFF453A),
            goldMerit = Color(0xFFF5B841),
            linkColor = Color(0xFF409CFF),
            codeBackground = Color(0xFF242A33),
            dividerColor = Color.White.copy(alpha = 0.08f),
            primaryText = Color(0xFFF5F7FA),
            secondaryText = Color(0xFF949DA8),
            hintText = Color(0xFF5A6370),
            wallpaperStart = Color(0xFF181C22),
            wallpaperEnd = Color(0xFF181C22),
            doodleTint = Color(0x160A84FF),
            settings = settings,
            isAppleStyle = true,
            outgoingBubbleTop = Color(0xFF1E8FFF),
            outgoingBubbleBottom = Color(0xFF0A84FF),
            groupedBackground = Color(0xFF11151B),
            accentHover = Color(0xFF2994FF),
            inputBg = Color(0xFF242A33),
            cardBg = Color(0xFF222831),
            modalBg = Color(0xFF1E232B),
            hoverBg = Color(0xFF242A33),
            borderColor = Color(0xFF303844),
            servicePillBg = Color(0xFF242A33),
            servicePillFg = Color(0xFF8E96A0),
            codeForeground = Color(0xFFE5EAF0),
            outgoingCheckMarkColor = Color(0xFFFFFFFF),
            outgoingLinkColor = Color(0xFFFFFFFF),
            outgoingReplyBarColor = Color(0xFFFFFFFF),
            outgoingReactionTextColor = Color(0xFFFFFFFF),
            topBarActionColor = Color(0xFF0A84FF),
            searchCursorColor = Color(0xFF0A84FF),
            selectedPillBackground = Color(0xFF0A84FF),
            selectedPillContent = Color(0xFFFFFFFF),
            unreadBadgeContent = Color(0xFFFFFFFF),
            unreadBadgeMutedContent = Color(0xFFFFFFFF),
            outgoingCodeBg = Color(0x33000000),
            outgoingSubCardBg = Color.Black.copy(alpha = 0.20f),
            outgoingSubCardSelectedBg = Color.Black.copy(alpha = 0.30f),
            approveActionBg = Color(0xFF1A7F37),
            approveActionContent = Color.White,
            statusGreenText = Color(0xFF30D158),
        )

        TelegramThemePreset.DAY_BLUE -> TelegramPalette(
            isDark = false,
            topBarBackground = Color(0xFF517DA2),
            topBarContent = Color(0xFFFFFFFF),
            topBarSubtitle = Color(0xFFD2E5F6),
            drawerHeaderStart = Color(0xFF517DA2),
            drawerHeaderEnd = Color(0xFF3E678B),
            appBackground = Color(0xFFF0F2F5),
            surfaceColor = Color(0xFFFFFFFF),
            elevatedSurface = Color(0xFFF7F9FB),
            primaryAccent = Color(0xFF2AABEE),
            outgoingBubble = Color(0xFFEFFDDE),
            incomingBubble = Color(0xFFFFFFFF),
            outgoingText = Color(0xFF1F2328),
            incomingText = Color(0xFF1F2328),
            outgoingMeta = Color(0xFF55A14E),
            incomingMeta = Color(0xFF8E99A4),
            checkMarkColor = Color(0xFF4FAE4E),
            unreadBadgeActive = Color(0xFF4ECC5E),
            unreadBadgeMuted = Color(0xFFC4C9CC),
            secretGreen = Color(0xFF32A852),
            destructiveRed = Color(0xFFE53935),
            goldMerit = Color(0xFFD99B26),
            linkColor = Color(0xFF168ACD),
            codeBackground = Color(0x16295278),
            dividerColor = Color(0xFFE4E8EC),
            primaryText = Color(0xFF222222),
            secondaryText = Color(0xFF707579),
            hintText = Color(0xFF9AA1A8),
            wallpaperStart = Color(0xFFD5E6D0),
            wallpaperEnd = Color(0xFFA9C8B6),
            doodleTint = Color(0x28436A53),
            settings = settings,
        )

        TelegramThemePreset.CLASSIC_GREEN -> TelegramPalette(
            isDark = false,
            topBarBackground = Color(0xFF4A769A),
            topBarContent = Color(0xFFFFFFFF),
            topBarSubtitle = Color(0xFFD6E7F5),
            drawerHeaderStart = Color(0xFF4A769A),
            drawerHeaderEnd = Color(0xFF365E7D),
            appBackground = Color(0xFFEDF1EE),
            surfaceColor = Color(0xFFFFFFFF),
            elevatedSurface = Color(0xFFF6F9F6),
            primaryAccent = Color(0xFF39A5DB),
            outgoingBubble = Color(0xFFE5F6DF),
            incomingBubble = Color(0xFFFFFFFF),
            outgoingText = Color(0xFF1B241B),
            incomingText = Color(0xFF1E2226),
            outgoingMeta = Color(0xFF489844),
            incomingMeta = Color(0xFF8C969E),
            checkMarkColor = Color(0xFF43A047),
            unreadBadgeActive = Color(0xFF47B856),
            unreadBadgeMuted = Color(0xFFC2C8CC),
            secretGreen = Color(0xFF2E9B47),
            destructiveRed = Color(0xFFE53935),
            goldMerit = Color(0xFFD99B26),
            linkColor = Color(0xFF157FB8),
            codeBackground = Color(0x162E5D34),
            dividerColor = Color(0xFFE2E7E3),
            primaryText = Color(0xFF202321),
            secondaryText = Color(0xFF6B736E),
            hintText = Color(0xFF97A09A),
            wallpaperStart = Color(0xFFCBD9C4),
            wallpaperEnd = Color(0xFFA3BF9E),
            doodleTint = Color(0x2A3F623E),
            settings = settings,
        )

        TelegramThemePreset.NIGHT -> TelegramPalette(
            isDark = true,
            topBarBackground = Color(0xFF17212B),
            topBarContent = Color(0xFFF5F5F5),
            topBarSubtitle = Color(0xFF7F91A4),
            drawerHeaderStart = Color(0xFF1F2C3A),
            drawerHeaderEnd = Color(0xFF141D26),
            appBackground = Color(0xFF0E1621),
            surfaceColor = Color(0xFF17212B),
            elevatedSurface = Color(0xFF202B36),
            primaryAccent = Color(0xFF64B5EF),
            outgoingBubble = Color(0xFF2B5278),
            incomingBubble = Color(0xFF182533),
            outgoingText = Color(0xFFF5F5F5),
            incomingText = Color(0xFFF5F5F5),
            outgoingMeta = Color(0xFF7EB8E8),
            incomingMeta = Color(0xFF6D7F8F),
            checkMarkColor = Color(0xFF64B5EF),
            unreadBadgeActive = Color(0xFF3E99DC),
            unreadBadgeMuted = Color(0xFF3B4A59),
            secretGreen = Color(0xFF4CD964),
            destructiveRed = Color(0xFFFF5C5C),
            goldMerit = Color(0xFFF5B841),
            linkColor = Color(0xFF6EC1FF),
            codeBackground = Color(0x330B1119),
            dividerColor = Color(0xFF101921),
            primaryText = Color(0xFFF5F5F5),
            secondaryText = Color(0xFF8A9AA9),
            hintText = Color(0xFF5D6E7E),
            wallpaperStart = Color(0xFF0E1621),
            wallpaperEnd = Color(0xFF132131),
            doodleTint = Color(0x2264B5EF),
            settings = settings,
        )

        TelegramThemePreset.NIGHT_TINTED -> TelegramPalette(
            isDark = true,
            topBarBackground = Color(0xFF18222D),
            topBarContent = Color(0xFFF4F7FA),
            topBarSubtitle = Color(0xFF8498AE),
            drawerHeaderStart = Color(0xFF213245),
            drawerHeaderEnd = Color(0xFF15202B),
            appBackground = Color(0xFF101923),
            surfaceColor = Color(0xFF18222D),
            elevatedSurface = Color(0xFF223040),
            primaryAccent = Color(0xFF3E88F7),
            outgoingBubble = Color(0xFF2A4B7C),
            incomingBubble = Color(0xFF1D2A3A),
            outgoingText = Color(0xFFF6F8FB),
            incomingText = Color(0xFFF4F7FA),
            outgoingMeta = Color(0xFF8DB8FA),
            incomingMeta = Color(0xFF74889D),
            checkMarkColor = Color(0xFF6CA6FF),
            unreadBadgeActive = Color(0xFF3E88F7),
            unreadBadgeMuted = Color(0xFF3A4B5E),
            secretGreen = Color(0xFF48D568),
            destructiveRed = Color(0xFFFF6161),
            goldMerit = Color(0xFFF6C04A),
            linkColor = Color(0xFF75B0FF),
            codeBackground = Color(0x360C131C),
            dividerColor = Color(0xFF121C27),
            primaryText = Color(0xFFF4F7FA),
            secondaryText = Color(0xFF8C9FB3),
            hintText = Color(0xFF607387),
            wallpaperStart = Color(0xFF101923),
            wallpaperEnd = Color(0xFF172738),
            doodleTint = Color(0x243E88F7),
            settings = settings,
        )

        TelegramThemePreset.CUSTOM_ACCENT -> {
            val accent = Color(settings.customAccentArgb.toInt())
            val dark = settings.customDark
            if (dark) {
                TelegramPalette(
                    isDark = true,
                    topBarBackground = Color(0xFF181E27),
                    topBarContent = Color(0xFFF5F7FA),
                    topBarSubtitle = Color(0xFF9AA5B4),
                    drawerHeaderStart = accent.copy(alpha = 0.85f),
                    drawerHeaderEnd = Color(0xFF161C24),
                    appBackground = Color(0xFF0F141B),
                    surfaceColor = Color(0xFF181E27),
                    elevatedSurface = Color(0xFF222A36),
                    primaryAccent = accent,
                    outgoingBubble = Color(
                        red = (accent.red * 0.42f + 0.10f).coerceIn(0f, 1f),
                        green = (accent.green * 0.42f + 0.12f).coerceIn(0f, 1f),
                        blue = (accent.blue * 0.42f + 0.16f).coerceIn(0f, 1f),
                        alpha = 1f,
                    ),
                    incomingBubble = Color(0xFF1D2430),
                    outgoingText = Color(0xFFF8F9FB),
                    incomingText = Color(0xFFF3F5F8),
                    outgoingMeta = Color(0xFFC8D4E3),
                    incomingMeta = Color(0xFF7D8A99),
                    checkMarkColor = accent,
                    unreadBadgeActive = accent,
                    unreadBadgeMuted = Color(0xFF3E4957),
                    secretGreen = Color(0xFF4CD964),
                    destructiveRed = Color(0xFFFF5C5C),
                    goldMerit = Color(0xFFF5B841),
                    linkColor = if (accent.luminance() < 0.25f) Color(0xFF7AB8FF) else accent,
                    codeBackground = Color(0x330A0E14),
                    dividerColor = Color(0xFF141A22),
                    primaryText = Color(0xFFF5F7FA),
                    secondaryText = Color(0xFF94A0AE),
                    hintText = Color(0xFF667280),
                    wallpaperStart = Color(0xFF10151D),
                    wallpaperEnd = Color(0xFF1A2230),
                    doodleTint = accent.copy(alpha = 0.16f),
                    settings = settings,
                )
            } else {
                TelegramPalette(
                    isDark = false,
                    topBarBackground = accent,
                    topBarContent = Color(0xFFFFFFFF),
                    topBarSubtitle = Color(0xFFE6EEF8),
                    drawerHeaderStart = accent,
                    drawerHeaderEnd = Color(
                        red = (accent.red * 0.75f).coerceIn(0f, 1f),
                        green = (accent.green * 0.75f).coerceIn(0f, 1f),
                        blue = (accent.blue * 0.75f).coerceIn(0f, 1f),
                        alpha = 1f,
                    ),
                    appBackground = Color(0xFFF2F4F7),
                    surfaceColor = Color(0xFFFFFFFF),
                    elevatedSurface = Color(0xFFF8F9FC),
                    primaryAccent = accent,
                    outgoingBubble = Color(
                        red = (1f - (1f - accent.red) * 0.14f).coerceIn(0f, 1f),
                        green = (1f - (1f - accent.green) * 0.14f).coerceIn(0f, 1f),
                        blue = (1f - (1f - accent.blue) * 0.14f).coerceIn(0f, 1f),
                        alpha = 1f,
                    ),
                    incomingBubble = Color(0xFFFFFFFF),
                    outgoingText = Color(0xFF1E2228),
                    incomingText = Color(0xFF1E2228),
                    outgoingMeta = accent,
                    incomingMeta = Color(0xFF8C95A0),
                    checkMarkColor = accent,
                    unreadBadgeActive = accent,
                    unreadBadgeMuted = Color(0xFFC4C9D0),
                    secretGreen = Color(0xFF2FA84F),
                    destructiveRed = Color(0xFFE53935),
                    goldMerit = Color(0xFFD99B26),
                    linkColor = accent,
                    codeBackground = accent.copy(alpha = 0.10f),
                    dividerColor = Color(0xFFE5E8EE),
                    primaryText = Color(0xFF202328),
                    secondaryText = Color(0xFF6D747E),
                    hintText = Color(0xFF98A0AB),
                    wallpaperStart = Color(
                        red = (1f - (1f - accent.red) * 0.22f).coerceIn(0f, 1f),
                        green = (1f - (1f - accent.green) * 0.22f).coerceIn(0f, 1f),
                        blue = (1f - (1f - accent.blue) * 0.22f).coerceIn(0f, 1f),
                        alpha = 1f,
                    ),
                    wallpaperEnd = Color(0xFFDDE4EE),
                    doodleTint = accent.copy(alpha = 0.16f),
                    settings = settings,
                )
            }
        }
    }
}

val LocalTelegramPalette = staticCompositionLocalOf { resolveTelegramPalette(TelegramThemeSettings()) }

@Composable
fun ProvideTelegramTheme(
    settings: TelegramThemeSettings,
    content: @Composable () -> Unit,
) {
    val palette = remember(settings) { resolveTelegramPalette(settings) }
    val m3Scheme = remember(palette) {
        if (palette.isDark) {
            darkColorScheme(
                primary = palette.primaryAccent,
                onPrimary = Color.White,
                background = palette.appBackground,
                surface = palette.surfaceColor,
                onSurface = palette.primaryText,
                surfaceVariant = palette.elevatedSurface,
                onSurfaceVariant = palette.secondaryText,
                error = palette.destructiveRed,
            )
        } else {
            lightColorScheme(
                primary = palette.primaryAccent,
                onPrimary = Color.White,
                background = palette.appBackground,
                surface = palette.surfaceColor,
                onSurface = palette.primaryText,
                surfaceVariant = palette.elevatedSurface,
                onSurfaceVariant = palette.secondaryText,
                error = palette.destructiveRed,
            )
        }
    }
    CompositionLocalProvider(LocalTelegramPalette provides palette) {
        MaterialTheme(colorScheme = m3Scheme, content = content)
    }
}

@Composable
fun TelegramHairlineDivider(modifier: Modifier = Modifier) {
    val palette = LocalTelegramPalette.current
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(0.5.dp)
            .background(palette.dividerColor),
    )
}

/**
 * Procedural Telegram Android chat wallpaper renderer with geometric/dharma doodle vector motifs,
 * smooth multi-stop gradients, and configurable dim overlay.
 */
@Composable
fun TelegramChatWallpaperBackground(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val style = palette.settings.wallpaperStyle
    val dimAlpha = (palette.settings.wallpaperDimPercent.coerceIn(0, 80) / 100f)

    val (gradTop, gradBottom, motifTint) = remember(palette, style) {
        when (style) {
            TelegramWallpaperStyle.DOODLE_CLASSIC -> Triple(palette.wallpaperStart, palette.wallpaperEnd, palette.doodleTint)
            TelegramWallpaperStyle.OCEAN_MIST -> if (palette.isDark) {
                Triple(Color(0xFF0E2235), Color(0xFF14354B), Color(0x2664B5EF))
            } else {
                Triple(Color(0xFFCDE5F7), Color(0xFFD9F0E8), Color(0x263478A8))
            }
            TelegramWallpaperStyle.AURORA_NIGHT -> Triple(Color(0xFF12182B), Color(0xFF1E2D3D), Color(0x297EE8FA))
            TelegramWallpaperStyle.LOTUS_GOLD -> if (palette.isDark) {
                Triple(Color(0xFF1F1A14), Color(0xFF141822), Color(0x28F5B841))
            } else {
                Triple(Color(0xFFF7EFE0), Color(0xFFEBDCC3), Color(0x28B68224))
            }
            TelegramWallpaperStyle.FOREST_ZEN -> if (palette.isDark) {
                Triple(Color(0xFF102019), Color(0xFF152922), Color(0x2458C486))
            } else {
                Triple(Color(0xFFD5E8D4), Color(0xFFBCD8C1), Color(0x243A784C))
            }
            TelegramWallpaperStyle.SOLID_MINIMAL -> {
                val bg = if (palette.isAppleStyle) palette.wallpaperStart else palette.appBackground
                Triple(bg, bg, Color.Transparent)
            }
        }
    }

    Box(modifier = modifier.fillMaxSize()) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawRect(brush = Brush.verticalGradient(listOf(gradTop, gradBottom)))
            if (style != TelegramWallpaperStyle.SOLID_MINIMAL) {
                val cell = 96.dp.toPx().coerceAtLeast(64f)
                val cols = (size.width / cell).toInt() + 2
                val rows = (size.height / cell).toInt() + 2
                val stroke = Stroke(width = 1.4.dp.toPx())
                for (r in 0 until rows) {
                    for (c in 0 until cols) {
                        val staggerX = if (r % 2 == 1) cell * 0.48f else 0f
                        val cx = c * cell + staggerX + cell * 0.3f
                        val cy = r * cell + cell * 0.35f
                        when ((r * 7 + c * 3) % 5) {
                            0 -> {
                                // Paper plane glyph
                                val s = cell * 0.13f
                                val p = Path().apply {
                                    moveTo(cx - s, cy + s * 0.2f)
                                    lineTo(cx + s, cy - s * 0.8f)
                                    lineTo(cx + s * 0.25f, cy + s)
                                    lineTo(cx - s * 0.15f, cy + s * 0.35f)
                                    close()
                                }
                                drawPath(p, color = motifTint, style = stroke)
                            }
                            1 -> {
                                // Chat bubble outline
                                val s = cell * 0.14f
                                drawRoundRect(
                                    color = motifTint,
                                    topLeft = Offset(cx - s, cy - s * 0.7f),
                                    size = Size(s * 2f, s * 1.4f),
                                    cornerRadius = CornerRadius(s * 0.5f, s * 0.5f),
                                    style = stroke,
                                )
                            }
                            2 -> {
                                // 8-spoke Dharma / star motif
                                val s = cell * 0.11f
                                drawCircle(color = motifTint, radius = s, center = Offset(cx, cy), style = stroke)
                                for (k in 0 until 4) {
                                    val angle = (k * PI / 4.0)
                                    val dx = (cos(angle) * s * 1.35f).toFloat()
                                    val dy = (sin(angle) * s * 1.35f).toFloat()
                                    drawLine(motifTint, Offset(cx - dx, cy - dy), Offset(cx + dx, cy + dy), strokeWidth = stroke.width)
                                }
                            }
                            3 -> {
                                // Sparkle diamond
                                val s = cell * 0.10f
                                val p = Path().apply {
                                    moveTo(cx, cy - s)
                                    quadraticBezierTo(cx, cy, cx + s, cy)
                                    quadraticBezierTo(cx, cy, cx, cy + s)
                                    quadraticBezierTo(cx, cy, cx - s, cy)
                                    quadraticBezierTo(cx, cy, cx, cy - s)
                                    close()
                                }
                                drawPath(p, color = motifTint, style = stroke)
                            }
                            else -> {
                                // Ringed planet
                                val s = cell * 0.09f
                                drawCircle(color = motifTint, radius = s, center = Offset(cx, cy), style = stroke)
                                drawOval(
                                    color = motifTint,
                                    topLeft = Offset(cx - s * 1.7f, cy - s * 0.45f),
                                    size = Size(s * 3.4f, s * 0.9f),
                                    style = stroke,
                                )
                            }
                        }
                    }
                }
            }
            if (dimAlpha > 0f) {
                drawRect(color = Color.Black.copy(alpha = dimAlpha))
            }
        }
        content()
    }
}

@Composable
fun TelegramBottomSheet(
    visible: Boolean,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    if (!visible) return
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onDismiss)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.46f))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onDismiss,
            ),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = {},
                ),
            shape = RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp),
            color = palette.surfaceColor,
            shadowElevation = 16.dp,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(bottom = 12.dp),
            ) {
                Box(
                    modifier = Modifier
                        .padding(top = 10.dp, bottom = 6.dp)
                        .width(38.dp)
                        .height(4.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(palette.hintText.copy(alpha = 0.45f))
                        .align(Alignment.CenterHorizontally),
                )
                content()
                Spacer(Modifier.height(4.dp))
            }
        }
    }
}
