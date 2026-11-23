package com.ombhrum.fabushi.telegram

import com.ombhrum.fabushi.AndroidUpdateValidator
import com.ombhrum.fabushi.AndroidVersion
import com.ombhrum.fabushi.ConversationKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TelegramModelsAndEntitiesTest {

    @Test
    fun utf16MarkdownEntitiesPreserveSurrogatePairsAndCjkOffsets() {
        val raw = "🚀 全球 **法布施** 与 ||剧透心灯|| 详见 [官网](https://fabushi.ombhrum.com) /mika @MikaPhoneBot #共修"
        val (plain, entities) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(raw)

        assertEquals("🚀 全球 法布施 与 剧透心灯 详见 官网 /mika @MikaPhoneBot #共修", plain)

        val bold = entities.first { it.type == MessageEntityType.BOLD }
        assertEquals("法布施", TelegramEntitiesEngine.substringSafeUtf16(plain, bold.offset, bold.length))

        val spoiler = entities.first { it.type == MessageEntityType.SPOILER }
        assertEquals("剧透心灯", TelegramEntitiesEngine.substringSafeUtf16(plain, spoiler.offset, spoiler.length))

        val textUrl = entities.first { it.type == MessageEntityType.TEXT_URL }
        assertEquals("官网", TelegramEntitiesEngine.substringSafeUtf16(plain, textUrl.offset, textUrl.length))
        assertEquals("https://fabushi.ombhrum.com", textUrl.url)

        val cmd = entities.first { it.type == MessageEntityType.BOT_COMMAND }
        assertEquals("/mika", TelegramEntitiesEngine.substringSafeUtf16(plain, cmd.offset, cmd.length))

        val mention = entities.first { it.type == MessageEntityType.MENTION }
        assertEquals("@MikaPhoneBot", TelegramEntitiesEngine.substringSafeUtf16(plain, mention.offset, mention.length))

        val hashtag = entities.first { it.type == MessageEntityType.HASHTAG }
        assertEquals("#共修", TelegramEntitiesEngine.substringSafeUtf16(plain, hashtag.offset, hashtag.length))
    }

    @Test
    fun sliceSafeUtf16NeverSplitsEmojiSurrogatePairs() {
        val text = "A🚀B" // A (0), \uD83D (1), \uDE80 (2), B (3)
        // Starting on low surrogate index 2 expands back to index 1 so 🚀 is intact
        val (s1, e1) = TelegramEntitiesEngine.sliceSafeUtf16(text, offset = 2, length = 1)
        assertEquals(1, s1)
        assertEquals(3, e1)
        assertEquals("🚀", text.substring(s1, e1))

        // Ending after high surrogate index 1 expands forward to index 3
        val (s2, e2) = TelegramEntitiesEngine.sliceSafeUtf16(text, offset = 0, length = 2)
        assertEquals(0, s2)
        assertEquals(3, e2)
        assertEquals("A🚀", text.substring(s2, e2))
    }

    @Test
    fun fencedCodeBlockAndSelectionFormattingRoundTrip() {
        val raw = "前缀 ```rust\nfn main() {}\n``` 后缀"
        val (plain, entities) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(raw)
        val pre = entities.first { it.type == MessageEntityType.PRE }
        assertEquals("rust", pre.language)
        assertEquals("fn main() {}\n", TelegramEntitiesEngine.substringSafeUtf16(plain, pre.offset, pre.length))

        val (_, added) = TelegramEntitiesEngine.applyFormattingSelection("Hello Telegram", 6, 14, emptyList(), MessageEntityType.UNDERLINE)
        assertEquals(1, added.size)
        assertEquals(MessageEntityType.UNDERLINE, added.first().type)

        val (_, toggledOff) = TelegramEntitiesEngine.applyFormattingSelection("Hello Telegram", 6, 14, added, MessageEntityType.UNDERLINE)
        assertTrue(toggledOff.isEmpty())

        val wrapped = TelegramEntitiesEngine.wrapDraftSelectionWithMarkdown("请点击这里继续", 3, 5, MessageEntityType.SPOILER)
        assertEquals("请点击||这里||继续", wrapped)
    }

    @Test
    fun fiveBit100BarVoiceWaveformPacksInto63BytesAndRoundTripsBitIdentically() {
        val bars = List(TelegramWaveformCodec.BAR_COUNT) { idx -> (idx * 7 + 3) % 32 }
        val packed = TelegramWaveformCodec.encode5Bit(bars)
        assertEquals(63, packed.size)

        val unpacked = TelegramWaveformCodec.decode5Bit(packed, TelegramWaveformCodec.BAR_COUNT)
        assertEquals(bars, unpacked)

        val b64 = TelegramWaveformCodec.encodeBase64(bars)
        assertEquals(bars, TelegramWaveformCodec.decodeBase64(b64))

        val resampled = TelegramWaveformCodec.normalizeTo100Bars(listOf(0, 64, 128, 255))
        assertEquals(100, resampled.size)
        assertEquals(0, resampled.first())
        assertEquals(31, resampled.last())

        val silentBar = TelegramWaveformCodec.computeRmsBar31(ShortArray(256) { 0 })
        val loudBar = TelegramWaveformCodec.computeRmsBar31(ShortArray(256) { 26_000 })
        assertEquals(1, silentBar)
        assertTrue(loudBar >= 25)
    }

    @Test
    fun callsEmojiFingerprintUses333TableAndSecretChat8x8Identicon() {
        assertEquals(333, CallsEmojiFingerprint.EMOJI_TABLE_333.size)

        val fpA = CallsEmojiFingerprint.deriveCallEmojis("call-key-alpha")
        val fpA2 = CallsEmojiFingerprint.deriveCallEmojis("call-key-alpha")
        val fpB = CallsEmojiFingerprint.deriveCallEmojis("call-key-beta")

        assertEquals(4, fpA.size)
        assertEquals(fpA, fpA2)
        assertNotEquals(fpA, fpB)
        assertTrue(fpA.all { it in CallsEmojiFingerprint.EMOJI_TABLE_333 })

        val secretFp = CallsEmojiFingerprint.deriveSecretChatIdenticon("x3dh-ratchet-seed")
        assertEquals(64, secretFp.cells8x8.size)
        assertTrue(secretFp.cells8x8.all { it in 0..3 })
        assertEquals(4, secretFp.emojis4.size)
    }

    @Test
    fun generationFenceAndFolderFiltersEnforceAccountIsolationAndTabs() {
        val fence = GenerationFence(1L)
        val gen1 = fence.currentGeneration
        assertTrue(fence.isCurrent(gen1))
        val gen2 = fence.advance()
        assertFalse(fence.isCurrent(gen1))
        assertTrue(fence.isCurrent(gen2))

        val directUnread = TelegramChatItem("c1", "Alice", "online", "hi", "10:00", ConversationKind.DIRECT, unreadCount = 2)
        val botChat = TelegramChatItem("b1", "MikaBot", "bot", "ready", "10:01", ConversationKind.DIRECT, isBot = true)
        val archivedGroup = TelegramChatItem("g1", "OldGroup", "group", "bye", "09:00", ConversationKind.GROUP, isArchived = true)
        val activeChannel = TelegramChatItem("ch1", "全球法布施", "channel", "post", "10:02", ConversationKind.CHANNEL, isMuted = true)

        val tabs = TelegramFolderFilter.DEFAULT_BUILT_IN_TABS.associateBy { it.id }
        assertTrue(TelegramFolderFilter.matches(tabs.getValue("all"), directUnread))
        assertFalse(TelegramFolderFilter.matches(tabs.getValue("all"), archivedGroup))
        assertTrue(TelegramFolderFilter.matches(tabs.getValue("personal"), directUnread))
        assertFalse(TelegramFolderFilter.matches(tabs.getValue("personal"), botChat))
        assertTrue(TelegramFolderFilter.matches(tabs.getValue("bots"), botChat))
        assertTrue(TelegramFolderFilter.matches(tabs.getValue("unread"), directUnread))
        assertFalse(TelegramFolderFilter.matches(tabs.getValue("unread"), botChat))
        assertTrue(TelegramFolderFilter.matches(tabs.getValue("channels"), activeChannel))
    }

    @Test
    fun inlineKeyboardBoundsGlobalDharma240CountriesAndMagpiePiiMasking() {
        val row = InlineKeyboardRow((1..12).map { InlineKeyboardButtonModel("id-$it", "Btn $it") })
        assertEquals(8, row.boundedButtons.size)

        val countries = GlobalDharmaCatalog.build240Countries()
        assertTrue(countries.isNotEmpty())
        assertTrue(countries.none { it.nameZh.startsWith("十方洲岛") || it.code.matches(Regex("R\\d+")) })
        assertTrue(GlobalDharmaUiState().countries.isEmpty())
        assertTrue(countries.any { it.code == "CN" })
        assertTrue(countries.any { it.code == "NP" })

        assertTrue(MagpiePresetCatalog.PRESETS_48.size >= 47)
        assertTrue(MagpiePresetCatalog.PRESETS_48.all { !it.configured && !it.healthy })
        val (masked, redactedCount) = MagpiePresetCatalog.maskSecretsAndPii(
            "Key sk-abc1234567890XYZ mail ops@ombhrum.com tel 13800138000",
        )
        assertEquals(3, redactedCount)
        assertFalse(masked.contains("sk-abc1234567890XYZ"))
        assertTrue(masked.contains("[REDACTED_API_KEY]"))
        assertTrue(masked.contains("[REDACTED_EMAIL]"))
        assertTrue(masked.contains("[REDACTED_PHONE]"))
    }

    @Test
    fun androidUpdateValidatorAcceptsCanonicalFabuAndroidRepoAndVersionTags() {
        assertEquals("fabu-android/fabushi-android", AndroidUpdateValidator.CANONICAL_REPOSITORY)
        assertTrue(AndroidUpdateValidator.isTrustedRepository("fabu-android/fabushi-android"))
        assertTrue(AndroidUpdateValidator.isTrustedRepository("bhrumom/fabushi-android"))
        assertFalse(AndroidUpdateValidator.isTrustedRepository("evil/fork"))

        assertTrue(AndroidUpdateValidator.isSupportedReleaseTag("v2.0.0"))
        assertTrue(AndroidUpdateValidator.isSupportedReleaseTag("android-v2.0.0"))
        assertFalse(AndroidUpdateValidator.isSupportedReleaseTag("desktop-v2.0.0"))

        val validUrl = "https://github.com/fabu-android/fabushi-android/releases/download/v2.0.0/fabushi-android-release.apk"
        assertEquals(validUrl, AndroidUpdateValidator.trustedReleaseDownloadUrl(validUrl))

        assertTrue(AndroidVersion.isNewer("android-v2.0.0", 200, "v1.0.5", 105))
    }
}
