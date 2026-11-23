package com.ombhrum.fabushi.telegram

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.luminance
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.isAllowedRemoteComputerUrl
import com.ombhrum.fabushi.sanitizeMediaCacheFileName
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.time.ZoneOffset
import java.time.ZonedDateTime
import kotlin.math.max
import kotlin.math.min

class AppleUiAndContrastTest {

    private fun contrastRatio(a: Color, b: Color): Double {
        val la = a.luminance().toDouble()
        val lb = b.luminance().toDouble()
        val lighter = max(la, lb)
        val darker = min(la, lb)
        return (lighter + 0.05) / (darker + 0.05)
    }

    @Test
    fun desktopV166AppleTokensAndZeroDimDefault() {
        val defaults = TelegramThemeSettings()
        assertEquals(TelegramThemePreset.APPLE_DAY, defaults.preset)
        assertEquals(TelegramWallpaperStyle.SOLID_MINIMAL, defaults.wallpaperStyle)
        assertEquals(0, defaults.wallpaperDimPercent)
        assertEquals(18, defaults.bubbleCornerRadiusDp)

        val day = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_DAY))
        assertFalse(day.isDark)
        assertTrue(day.isAppleStyle)
        assertEquals(Color(0xFFFFFFFF), day.appBackground)
        assertEquals(Color(0xFFF2F2F7), day.groupedBackground)
        assertEquals(Color(0xFFFFFFFF), day.topBarBackground)
        assertEquals(Color(0xFF1D1D1F), day.topBarContent)
        assertEquals(Color(0xFF007AFF), day.primaryAccent)
        assertEquals(Color(0xFF2C95FF), day.outgoingBubbleTop)
        assertEquals(Color(0xFF007AFF), day.outgoingBubbleBottom)
        assertEquals(Color(0xFFFFFFFF), day.outgoingText)
        assertEquals(Color(0xFFE5F2FF), day.outgoingMeta)
        assertEquals(Color(0xFFE9E9EB), day.incomingBubble)
        assertEquals(Color(0xFF1D1D1F), day.incomingText)
        assertEquals(Color(0xFF86868B), day.secondaryText)
        assertEquals(Color(0xFFFF3B30), day.destructiveRed)
        assertEquals(Color(0xFF34C759), day.secretGreen)

        val night = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_NIGHT))
        assertTrue(night.isDark)
        assertTrue(night.isAppleStyle)
        assertEquals(Color(0xFF1E1E1E), night.appBackground)
        assertEquals(Color(0xFF000000), night.groupedBackground)
        assertEquals(Color(0xFF0A84FF), night.primaryAccent)
        assertEquals(Color(0xFF1E8FFF), night.outgoingBubbleTop)
        assertEquals(Color(0xFF0A84FF), night.outgoingBubbleBottom)
        assertEquals(Color(0xFF262629), night.incomingBubble)
        assertEquals(Color(0xFFF5F5F7), night.incomingText)
        assertEquals(Color(0xFF98989D), night.secondaryText)

        val tinted = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.APPLE_TINTED))
        assertTrue(tinted.isDark)
        assertTrue(tinted.isAppleStyle)
        assertEquals(Color(0xFF181C22), tinted.appBackground)
        assertEquals(Color(0xFF0A84FF), tinted.primaryAccent)
        assertEquals(Color(0xFF242A33), tinted.incomingBubble)
        assertEquals(Color(0xFFF5F7FA), tinted.incomingText)
        assertEquals(Color(0xFF949DA8), tinted.secondaryText)

        val classicGreen = resolveTelegramPalette(TelegramThemeSettings(preset = TelegramThemePreset.CLASSIC_GREEN))
        assertFalse(classicGreen.isAppleStyle)
    }

    @Test
    fun wcag21ContrastOraclesPassAcrossAllThreeAppleThemes() {
        val presets = listOf(
            TelegramThemePreset.APPLE_DAY,
            TelegramThemePreset.APPLE_NIGHT,
            TelegramThemePreset.APPLE_TINTED,
        )
        for (preset in presets) {
            val p = resolveTelegramPalette(TelegramThemeSettings(preset = preset))

            assertTrue("$preset primaryText on appBackground", contrastRatio(p.primaryText, p.appBackground) >= 4.5)
            assertTrue("$preset topBarContent on topBarBackground", contrastRatio(p.topBarContent, p.topBarBackground) >= 4.5)
            assertTrue("$preset incomingText on incomingBubble", contrastRatio(p.incomingText, p.incomingBubble) >= 4.5)

            // Outgoing foregrounds must pass >= 3.0 against BOTH bottom and top gradient stops
            val outgoingFgs = listOf(
                "outgoingText" to p.outgoingText,
                "outgoingLinkColor" to p.outgoingLinkColor,
                "outgoingCheckMarkColor" to p.outgoingCheckMarkColor,
                "outgoingReplyBarColor" to p.outgoingReplyBarColor,
                "outgoingReactionTextColor" to p.outgoingReactionTextColor,
            )
            for ((name, fg) in outgoingFgs) {
                assertTrue("$preset $name on outgoingBubbleBottom", contrastRatio(fg, p.outgoingBubbleBottom) >= 3.0)
                assertTrue("$preset $name on outgoingBubbleTop", contrastRatio(fg, p.outgoingBubbleTop) >= 3.0)
            }

            // outgoingMeta renders at bottom of outgoing bubble (>= 2.95:1)
            assertTrue("$preset outgoingMeta on outgoingBubbleBottom", contrastRatio(p.outgoingMeta, p.outgoingBubbleBottom) >= 2.95)
            assertTrue("$preset secondaryText on appBackground", contrastRatio(p.secondaryText, p.appBackground) >= 3.0)
            assertTrue("$preset selectedPillContent on selectedPillBackground", contrastRatio(p.selectedPillContent, p.selectedPillBackground) >= 3.0)
            assertTrue("$preset unreadBadgeContent on primaryAccent", contrastRatio(p.unreadBadgeContent, p.primaryAccent) >= 3.0)
            assertTrue("$preset topBarActionColor on topBarBackground", contrastRatio(p.topBarActionColor, p.topBarBackground) >= 3.0)
        }
    }

    @Test
    fun appleShellTabAndPinnedChatsGridPartitioning() {
        assertEquals(
            listOf(
                AppleShellTab.MESSAGES,
                AppleShellTab.CONTACTS,
                AppleShellTab.AGENTS,
                AppleShellTab.REMOTE,
                AppleShellTab.SETTINGS,
            ),
            AppleShellTab.entries,
        )

        fun chat(id: String, pinned: Boolean, archived: Boolean = false) = TelegramChatItem(
            id = id,
            title = "Chat $id",
            subtitle = "",
            preview = "",
            time = "10:00",
            kind = ConversationKind.DIRECT,
            isPinned = pinned,
            isArchived = archived,
        )

        // 0 pinned (archived pinned excluded)
        val zero = partitionPinnedChatsGrid(listOf(chat("1", false), chat("2", true, archived = true)))
        assertFalse(zero.centeredSingleRow)
        assertTrue(zero.rows.isEmpty())

        // 1 pinned -> centered single row
        val one = partitionPinnedChatsGrid(listOf(chat("1", true), chat("2", false)))
        assertTrue(one.centeredSingleRow)
        assertEquals(1, one.rows.size)
        assertEquals(listOf("1"), one.rows[0].map { it.id })

        // 2 pinned -> centered single row preserving order
        val two = partitionPinnedChatsGrid(listOf(chat("b", true), chat("a", true)))
        assertTrue(two.centeredSingleRow)
        assertEquals(listOf("b", "a"), two.rows[0].map { it.id })

        // 3 pinned -> 3-column row
        val three = partitionPinnedChatsGrid((1..3).map { chat("$it", true) })
        assertFalse(three.centeredSingleRow)
        assertEquals(1, three.rows.size)
        assertEquals(3, three.rows[0].size)

        // 4 pinned -> 3 + 1
        val four = partitionPinnedChatsGrid((1..4).map { chat("$it", true) })
        assertFalse(four.centeredSingleRow)
        assertEquals(listOf(3, 1), four.rows.map { it.size })

        // 7 pinned -> 3 + 3 + 1
        val seven = partitionPinnedChatsGrid((1..7).map { chat("$it", true) })
        assertFalse(seven.centeredSingleRow)
        assertEquals(listOf(3, 3, 1), seven.rows.map { it.size })
    }

    @Test
    fun buildTimelineItemsEmitsDateDividersPreservesMediaOnlyAndGroupsBubbles() {
        val zone = ZoneOffset.UTC
        val day1a = ZonedDateTime.of(2026, 11, 20, 9, 0, 0, 0, zone).toInstant().toEpochMilli()
        val day1b = ZonedDateTime.of(2026, 11, 20, 9, 2, 0, 0, zone).toInstant().toEpochMilli()
        val day2a = ZonedDateTime.of(2026, 11, 21, 10, 0, 0, 0, zone).toInstant().toEpochMilli()

        val msgs = listOf(
            TelegramMessageItem("m1", "c1", "u1", "Alice", "Hello", outgoing = false, time = "09:00", timestampMs = day1a),
            TelegramMessageItem("m2", "c1", "u1", "Alice", "", contentType = "photo", mediaFileName = "pic.jpg", outgoing = false, time = "09:02", timestampMs = day1b),
            TelegramMessageItem("m3", "c1", "me", "Me", "Next day reply", outgoing = true, time = "10:00", timestampMs = day2a),
        )

        val timeline = buildTimelineItems(msgs, uiLanguage = "en-US", zoneId = zone)
        assertEquals(5, timeline.size)
        assertTrue(timeline[0] is ChatTimelineItem.DateDivider)
        assertEquals("2026-11-20", (timeline[0] as ChatTimelineItem.DateDivider).dayKey)

        val e1 = timeline[1] as ChatTimelineItem.MessageEntry
        assertFalse(e1.isGroupedWithPrev)
        assertTrue(e1.isGroupedWithNext)

        // Media-only blank-text message is preserved and grouped with previous same-sender message
        val e2 = timeline[2] as ChatTimelineItem.MessageEntry
        assertEquals("m2", e2.message.id)
        assertTrue(e2.isGroupedWithPrev)
        assertFalse(e2.isGroupedWithNext)

        assertTrue(timeline[3] is ChatTimelineItem.DateDivider)
        assertEquals("2026-11-21", (timeline[3] as ChatTimelineItem.DateDivider).dayKey)

        val e3 = timeline[4] as ChatTimelineItem.MessageEntry
        assertEquals("m3", e3.message.id)
        assertFalse(e3.isGroupedWithPrev)
        assertFalse(e3.isGroupedWithNext)
    }

    @Test
    fun parseMessageCodeSegmentsDeduplicatesFencedCodeBlocks() {
        val raw = "Intro text\n```kotlin\nval x = 42\nprintln(x)\n```\nMiddle text\n```bash\necho hi\n```\nOutro"
        val segments = parseMessageCodeSegments(raw)
        assertEquals(5, segments.size)
        assertEquals(MessageContentSegment.TextSegment("Intro text"), segments[0])
        assertEquals(MessageContentSegment.CodeBlockSegment("kotlin", "val x = 42\nprintln(x)"), segments[1])
        assertEquals(MessageContentSegment.TextSegment("Middle text"), segments[2])
        assertEquals(MessageContentSegment.CodeBlockSegment("bash", "echo hi"), segments[3])
        assertEquals(MessageContentSegment.TextSegment("Outro"), segments[4])

        // Ensure code is never duplicated inside any TextSegment
        val allText = segments.filterIsInstance<MessageContentSegment.TextSegment>().joinToString("\n") { it.text }
        assertFalse(allText.contains("val x = 42"))
        assertFalse(allText.contains("echo hi"))

        // Unclosed fence degrades cleanly to text without throwing
        val unclosed = parseMessageCodeSegments("```rust\nfn main() {}")
        assertEquals(1, unclosed.size)
        assertTrue(unclosed[0] is MessageContentSegment.TextSegment)
    }

    @Test
    fun canSubmitComposerAndLiveLanguageStringsAndBotStatus() {
        assertFalse(canSubmitComposer(""))
        assertFalse(canSubmitComposer("   \n\t "))
        assertTrue(canSubmitComposer("hello"))
        assertTrue(canSubmitComposer("", attachmentCount = 1))

        val zh = resolveUiStrings("zh-CN")
        assertFalse(zh.isEnglish)
        assertEquals("信息", zh.tabMessages)
        assertEquals("联系人", zh.tabContacts)
        assertEquals("智能体", zh.tabAgents)
        assertEquals("远程", zh.tabRemote)
        assertEquals("设置", zh.tabSettings)
        assertEquals("退出登录", zh.signOut)

        val en = resolveUiStrings("en-US")
        assertTrue(en.isEnglish)
        assertEquals("Messages", en.tabMessages)
        assertEquals("Contacts", en.tabContacts)
        assertEquals("Agents", en.tabAgents)
        assertEquals("Remote", en.tabRemote)
        assertEquals("Settings", en.tabSettings)
        assertEquals("Message", en.composerPlaceholder)
        assertEquals("Sign Out", en.signOut)

        assertNull(formatBotActivitySubtitle("idle", null, "zh-CN"))
        assertEquals("正在调用 shell_exec...", formatBotActivitySubtitle("running_tool", "shell_exec", "zh-CN"))
        assertEquals("Thinking...", formatBotActivitySubtitle("thinking", null, "en-US"))
    }

    @Test
    fun remoteComputerUrlAllowlistRejectsUntrustedAndMalformedUrls() {
        assertTrue(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com/remote-computer"))
        assertTrue(isAllowedRemoteComputerUrl("https://FABUSHI.OMBHRUM.COM:443/remote-computer?session=abc"))
        assertTrue(isAllowedRemoteComputerUrl("https://remote.fabushi.ombhrum.com/session/1"))

        assertFalse(isAllowedRemoteComputerUrl(""))
        assertFalse(isAllowedRemoteComputerUrl("http://fabushi.ombhrum.com/remote-computer"))
        assertFalse(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com:8443/remote-computer"))
        assertFalse(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com@evil.com/remote-computer"))
        assertFalse(isAllowedRemoteComputerUrl("https://evil.fabushi.ombhrum.com/remote-computer"))
        assertFalse(isAllowedRemoteComputerUrl("https://fabushi.ombhrum.com.evil.com/"))
        assertFalse(isAllowedRemoteComputerUrl("javascript:alert(1)"))
        assertFalse(isAllowedRemoteComputerUrl("file:///etc/passwd"))
    }

    @Test
    fun qrCodeEncoderProducesValidIso18004MatricesForTonAndAlipayUrls() {
        val tonPayload = "ton://transfer/UQD8x9mN4pQ2kL7vR5sT1wY3zA6bC9dE0fG2hJ4kM6nP8qR0"
        val tonMatrix = QrCodeEncoder.encode(tonPayload)
        assertValidIso18004Matrix(tonMatrix)

        val alipayUrl = "https://openauth.alipay.com/oauth2/publicAppAuthorize.htm?app_id=2026000012345678&scope=auth_user&redirect_uri=https%3A%2F%2Ffabushi.ombhrum.com%2Fapi%2Fv1%2Fauth%2Falipay%2Fcallback&state=attempt_9f8e7d6c5b4a3210"
        val alipayMatrix = QrCodeEncoder.encode(alipayUrl)
        assertValidIso18004Matrix(alipayMatrix)
        assertTrue(alipayMatrix.version >= 7)
    }

    private fun assertValidIso18004Matrix(m: QrMatrix) {
        assertNotNull(m)
        assertEquals(m.version * 4 + 17, m.size)
        val n = m.size

        // Check all three 7x7 finder patterns
        val origins = listOf(0 to 0, (n - 7) to 0, 0 to (n - 7))
        for ((ox, oy) in origins) {
            for (dy in 0..6) {
                for (dx in 0..6) {
                    val dist = max(kotlin.math.abs(dx - 3), kotlin.math.abs(dy - 3))
                    val expectedDark = dist != 2
                    assertEquals("Finder ($ox,$oy) at ($dx,$dy)", expectedDark, m.get(ox + dx, oy + dy))
                }
            }
        }

        // Check timing patterns between finders
        for (i in 8 until n - 8) {
            assertEquals("Horizontal timing at x=$i", i % 2 == 0, m.get(i, 6))
            assertEquals("Vertical timing at y=$i", i % 2 == 0, m.get(6, i))
        }

        // Check mandatory dark module at (8, 4 * version + 9)
        assertTrue("Dark module", m.get(8, 4 * m.version + 9))
    }

    @Test
    fun wcag21ContrastRegressionGuardsForMutedBadgesSubCardsApproveAndLegacyThemes() {
        val applePresets = listOf(
            TelegramThemePreset.APPLE_DAY,
            TelegramThemePreset.APPLE_NIGHT,
            TelegramThemePreset.APPLE_TINTED,
        )
        for (preset in applePresets) {
            val p = resolveTelegramPalette(TelegramThemeSettings(preset = preset))

            // Muted unread badge foreground vs background (>= 4.5:1; fixes 1.68:1 APPLE_DAY bug)
            assertTrue(
                "$preset unreadBadgeMutedContent on unreadBadgeMuted",
                contrastRatio(p.unreadBadgeMutedContent, p.unreadBadgeMuted) >= 4.5,
            )

            // Alpha-composited outgoing sub-cards over lightest outgoingBubbleTop stop
            val subCardTop = p.outgoingSubCardBg.compositeOver(p.outgoingBubbleTop)
            val subCardSelectedTop = p.outgoingSubCardSelectedBg.compositeOver(p.outgoingBubbleTop)
            assertTrue(
                "$preset outgoingText on outgoingSubCardBg@top",
                contrastRatio(p.outgoingText, subCardTop) >= 4.5,
            )
            assertTrue(
                "$preset outgoingMeta on outgoingSubCardBg@top",
                contrastRatio(p.outgoingMeta, subCardTop) >= 3.0,
            )
            assertTrue(
                "$preset outgoingText on outgoingSubCardSelectedBg@top",
                contrastRatio(p.outgoingText, subCardSelectedTop) >= 4.5,
            )

            // Mika Approve button (>= 4.5:1; fixes 2.02-2.22:1 white-on-secretGreen bug)
            assertTrue(
                "$preset approveActionContent on approveActionBg",
                contrastRatio(p.approveActionContent, p.approveActionBg) >= 4.5,
            )

            // Green status text badges (>= 4.5:1; fixes 1.98-2.22:1 APPLE_DAY secretGreen-on-white bug)
            assertTrue(
                "$preset statusGreenText on appBackground",
                contrastRatio(p.statusGreenText, p.appBackground) >= 4.5,
            )
            assertTrue(
                "$preset statusGreenText on cardBg",
                contrastRatio(p.statusGreenText, p.cardBg) >= 4.5,
            )
            val greenPillBg = p.secretGreen.copy(alpha = 0.14f).compositeOver(p.cardBg)
            assertTrue(
                "$preset statusGreenText on greenPillBg",
                contrastRatio(p.statusGreenText, greenPillBg) >= 4.5,
            )
        }

        // Legacy light themes must keep dark outgoingText legible on pale outgoing bubbles & code blocks
        for (legacy in listOf(TelegramThemePreset.CLASSIC_GREEN, TelegramThemePreset.DAY_BLUE)) {
            val p = resolveTelegramPalette(TelegramThemeSettings(preset = legacy))
            assertTrue(
                "$legacy outgoingText on outgoingBubbleBottom",
                contrastRatio(p.outgoingText, p.outgoingBubbleBottom) >= 4.5,
            )
            val codeBg = p.outgoingCodeBg.compositeOver(p.outgoingBubbleBottom)
            assertTrue(
                "$legacy outgoingText on outgoingCodeBg@bottom",
                contrastRatio(p.outgoingText, codeBg) >= 4.5,
            )
            assertTrue(
                "$legacy unreadBadgeMutedContent on unreadBadgeMuted",
                contrastRatio(p.unreadBadgeMutedContent, p.unreadBadgeMuted) >= 4.5,
            )
            assertEquals(Color.White, p.selectedPillContent)
        }
    }

    @Test
    fun timelineItemKeysRemainUniqueUnderClockSkewEpochZeroNegativeTimestampsAndDuplicateIds() {
        val zone = ZoneOffset.UTC
        val day2 = ZonedDateTime.of(2026, 11, 23, 0, 0, 1, 0, zone).toInstant().toEpochMilli()
        val day1 = ZonedDateTime.of(2026, 11, 22, 23, 59, 59, 0, zone).toInstant().toEpochMilli()
        val day2Later = ZonedDateTime.of(2026, 11, 23, 0, 0, 5, 0, zone).toInstant().toEpochMilli()

        val msgs = listOf(
            TelegramMessageItem("dup-1", "c1", "me", "Me", "Midnight optimistic", outgoing = true, time = "00:00", timestampMs = day2),
            TelegramMessageItem("m-skew", "c1", "u1", "Peer", "Skewed previous day", outgoing = false, time = "23:59", timestampMs = day1),
            TelegramMessageItem("m-zero", "c1", "u1", "Peer", "Epoch zero", outgoing = false, time = "00:00", timestampMs = 0L),
            TelegramMessageItem("m-neg", "c1", "u1", "Peer", "Negative ts", outgoing = false, time = "00:00", timestampMs = -5000L),
            TelegramMessageItem("dup-1", "c1", "me", "Me", "Duplicate id on day2", outgoing = true, time = "00:00", timestampMs = day2Later),
        )

        val timeline = buildTimelineItems(msgs, uiLanguage = "en-US", zoneId = zone)
        // Multiple DateDividers for 2026-11-23 and duplicate message id "dup-1" exist in timeline
        assertTrue(timeline.filterIsInstance<ChatTimelineItem.DateDivider>().count { it.dayKey == "2026-11-23" } == 2)
        val keys = timeline.mapIndexed { idx, entry -> timelineItemKey(idx, entry) }
        assertEquals("All LazyColumn keys must be strictly unique", keys.size, keys.toSet().size)
    }

    @Test
    fun filterMessageInlineKeyboardDeduplicatesMikaConfirmWithoutAffectingOtherCards() {
        val approveBtn = InlineKeyboardButtonModel("a", "✅ 批准", InlineKeyboardButtonKind.MIKA_ACTION, "mika_confirm:approve:t1:ask1")
        val declineBtn = InlineKeyboardButtonModel("d", "❌ 拒绝", InlineKeyboardButtonKind.MIKA_ACTION, "mika_confirm:decline:t1:ask1")
        val docsBtn = InlineKeyboardButtonModel("u", "Open Docs", InlineKeyboardButtonKind.URL, "https://example.com")

        // ASK_CONFIRM: suppresses mika_confirm:* buttons while retaining unrelated inline buttons
        val askConfirm = TelegramMessageItem(
            id = "m1",
            conversationId = "c1",
            senderId = "mika",
            senderName = "Mika",
            text = "Confirm shell command?",
            outgoing = false,
            time = "10:00",
            mikaTaskId = "t1",
            mikaAskId = "ask1",
            mikaRequiresConfirmation = true,
            inlineKeyboard = listOf(
                InlineKeyboardRow(listOf(approveBtn, declineBtn)),
                InlineKeyboardRow(listOf(docsBtn)),
            ),
        )
        val filteredConfirm = filterMessageInlineKeyboard(askConfirm)
        assertEquals(1, filteredConfirm.size)
        assertEquals(listOf(docsBtn), filteredConfirm[0].buttons)

        // ASK_INPUT / ASK_TAKEOVER (mikaAskId != null, mikaRequiresConfirmation == false): keeps keyboard untouched
        val askTakeover = askConfirm.copy(
            id = "m2",
            mikaRequiresConfirmation = false,
            inlineKeyboard = listOf(InlineKeyboardRow(listOf(docsBtn))),
        )
        assertEquals(askTakeover.inlineKeyboard, filterMessageInlineKeyboard(askTakeover))
    }

    @Test
    fun matchesBotUsernameNormalizesAtPrefixAndCase() {
        assertTrue(matchesBotUsername("@my_bot", "my_bot"))
        assertTrue(matchesBotUsername("my_bot", "@My_Bot"))
        assertTrue(matchesBotUsername("  @AlphaBot  ", "alphabot"))
        assertFalse(matchesBotUsername("@", "my_bot"))
        assertFalse(matchesBotUsername(null, "my_bot"))
        assertFalse(matchesBotUsername("@my_bot", ""))
        assertFalse(matchesBotUsername("@other_bot", "my_bot"))
    }

    @Test
    fun sanitizeMediaCacheFileNameRejectsTraversalAndBlankNames() {
        assertEquals("media-msg-1", sanitizeMediaCacheFileName(null, "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("   ", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName(".", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("..", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("../secret.png", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("a/b/../c.png", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("a\\..\\c.png", "msg-1"))
        assertEquals("folder-photo.png", sanitizeMediaCacheFileName("folder/photo.png", "msg-1"))
        assertEquals("report.pdf", sanitizeMediaCacheFileName("report.pdf", "msg-1"))
        assertFalse(sanitizeMediaCacheFileName("..", "../bad").contains(".."))

        // NUL bytes and ASCII control characters (0x00..0x1F, 0x7F)
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("\u0000", "msg-1"))
        assertEquals("photo.png", sanitizeMediaCacheFileName("photo\u0000.png", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("\u0000..\u0000/secret", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("\u0007\u001F\u007F", "msg-1"))
        assertEquals("media-msg-1", sanitizeMediaCacheFileName("\n\r\t\u007F", "msg-1"))
        assertEquals("abc.jpg", sanitizeMediaCacheFileName("a\nb\rc.jpg", "msg-1"))
        val sanitizedFallback = sanitizeMediaCacheFileName("\u0000", "id\u0000\n\r\u007F../bad")
        assertTrue(sanitizedFallback.none { it.code < 0x20 || it.code == 0x7F })
        assertFalse(sanitizedFallback.contains(".."))

        // Oversized ASCII and multi-byte UTF-8 names stay bounded under Linux NAME_MAX
        val longAscii = sanitizeMediaCacheFileName("a".repeat(300) + ".png", "msg-1")
        assertTrue(longAscii.length <= 120)
        assertTrue(longAscii.endsWith(".png"))
        assertTrue(longAscii.toByteArray(Charsets.UTF_8).size <= 200)

        val longUtf8 = sanitizeMediaCacheFileName("中".repeat(150) + ".png", "msg-1")
        assertTrue(longUtf8.length <= 120)
        assertTrue(longUtf8.endsWith(".png"))
        assertTrue(longUtf8.toByteArray(Charsets.UTF_8).size <= 200)

        // Real JVM filesystem write verification across adversarial inputs
        val tempDir = Files.createTempDirectory("fabushi-media-sanitize-test").toFile()
        try {
            val vectors = listOf(
                "\u0000" to "msg-1",
                "photo\u0000.png" to "msg-2",
                "\u0000..\u0000/secret" to "msg-3",
                "\u0007\u001F\u007F" to "msg-4",
                "\n\r\t\u007F" to "msg-5",
                "a\nb\rc.jpg" to "msg-6",
                "../secret.png" to "id\u0000../bad",
                ("a".repeat(300) + ".png") to "msg-7",
                ("中".repeat(150) + ".png") to "msg-8",
            )
            vectors.forEachIndexed { idx, (raw, fallback) ->
                val safeName = sanitizeMediaCacheFileName(raw, fallback)
                assertTrue("No control chars in [$safeName]", safeName.none { it.code < 0x20 || it.code == 0x7F })
                val target = File(tempDir, safeName)
                assertEquals(
                    "File must stay strictly inside tempDir for vector #$idx",
                    tempDir.canonicalFile,
                    target.canonicalFile.parentFile,
                )
                val payload = "media-bytes-$idx".toByteArray()
                target.writeBytes(payload)
                assertEquals("media-bytes-$idx", target.readText())
            }
        } finally {
            tempDir.deleteRecursively()
        }
    }

    @Test
    fun customAssistCodeAndHexColorValidationAndCrlfAndMinValueHash() {
        // Spec §8.4 custom assist code: 8..16 ASCII alphanumeric with >=1 letter and >=1 digit
        assertTrue(isValidCustomAssistCode("Abc12345"))
        assertTrue(isValidCustomAssistCode("  Abc12345  "))
        assertTrue(isValidCustomAssistCode("Abcd1234Efgh5678"))
        assertFalse(isValidCustomAssistCode("        "))
        assertFalse(isValidCustomAssistCode("12345678"))
        assertFalse(isValidCustomAssistCode("abcdefgh"))
        assertFalse(isValidCustomAssistCode("Abc1234"))
        assertFalse(isValidCustomAssistCode("Abcd1234Efgh56789"))
        assertFalse(isValidCustomAssistCode("Abc1234!"))
        assertFalse(isValidCustomAssistCode("中文123456"))

        // Custom #RRGGBB hex color validation
        assertTrue(isValidHexColor("#1084FE"))
        assertTrue(isValidHexColor("#abcdef"))
        assertTrue(isValidHexColor(" #00FF99 "))
        assertFalse(isValidHexColor("1084FE"))
        assertFalse(isValidHexColor("#12345"))
        assertFalse(isValidHexColor("#1234567"))
        assertFalse(isValidHexColor("#GGGGGG"))
        assertFalse(isValidHexColor(""))

        // CRLF normalization inside fenced code blocks
        val crlfSegments = parseMessageCodeSegments("Header\r\n```kt\r\nval a = 1\r\nval b = 2\r\n```\r\nFooter")
        assertEquals(3, crlfSegments.size)
        val codeSeg = crlfSegments[1] as MessageContentSegment.CodeBlockSegment
        assertEquals("kt", codeSeg.language)
        assertEquals("val a = 1\nval b = 2", codeSeg.code)
        assertFalse(codeSeg.code.contains("\r"))

        // Int.MIN_VALUE hashCode safety ("polygenelubricants".hashCode() == Int.MIN_VALUE)
        assertEquals(Int.MIN_VALUE, "polygenelubricants".hashCode())
        val minHashColor = resolveBotAvatarColor(null, "polygenelubricants")
        assertTrue(minHashColor.alpha > 0f)
    }
}
