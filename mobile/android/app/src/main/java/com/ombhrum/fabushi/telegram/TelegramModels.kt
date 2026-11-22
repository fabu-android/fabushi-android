package com.ombhrum.fabushi.telegram

import com.ombhrum.fabushi.ChatMessage
import com.ombhrum.fabushi.ChatPollOption
import com.ombhrum.fabushi.ChatReaction
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.ConversationParticipant
import com.ombhrum.fabushi.ConversationSummary
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

// ============================================================================
// 1. UTF-16 Message Entities & Rich Formatting Engine
// ============================================================================

enum class MessageEntityType(val wire: String, val label: String) {
    BOLD("bold", "粗体"),
    ITALIC("italic", "斜体"),
    UNDERLINE("underline", "下划线"),
    STRIKETHROUGH("strikethrough", "删除线"),
    SPOILER("spoiler", "剧透遮罩"),
    CODE("code", "行内代码"),
    PRE("pre", "代码块"),
    URL("url", "链接"),
    TEXT_URL("text_url", "超链接"),
    MENTION("mention", "@提及"),
    HASHTAG("hashtag", "#话题"),
    BOT_COMMAND("bot_command", "/指令");

    companion object {
        fun fromWire(value: String): MessageEntityType? =
            entries.firstOrNull { it.wire.equals(value.trim(), ignoreCase = true) }
    }
}

/**
 * Telegram-compatible UTF-16 message entity span.
 * [offset] and [length] are measured in UTF-16 code units (Kotlin [Char] indices).
 */
data class MessageEntity(
    val type: MessageEntityType,
    val offset: Int,
    val length: Int,
    val url: String? = null,
    val language: String? = null,
) {
    val endOffset: Int get() = offset + length
}

object TelegramEntitiesEngine {
    private val autoUrlRegex = Regex("""https?://[^\s<>()\[\]"']+""")
    private val autoMentionRegex = Regex("""(?<![A-Za-z0-9_])@[A-Za-z0-9_]{2,32}""")
    private val autoHashtagRegex = Regex("""(?<![A-Za-z0-9_])#[\p{L}\p{N}_]{1,64}""")
    private val autoCommandRegex = Regex("""(?<![A-Za-z0-9_/])/[A-Za-z0-9_]{2,64}(?:@[A-Za-z0-9_]{2,32})?""")

    /**
     * Clamps [offset] and [length] safely to valid UTF-16 boundaries of [text] without
     * splitting a high/low surrogate pair in the middle of an emoji or supplementary glyph.
     */
    fun sliceSafeUtf16(text: String, offset: Int, length: Int): Pair<Int, Int> {
        if (text.isEmpty() || length <= 0) return 0 to 0
        var start = offset.coerceIn(0, text.length)
        var end = (offset.coerceAtLeast(0) + length).coerceIn(start, text.length)
        if (start > 0 && start < text.length && Character.isLowSurrogate(text[start]) && Character.isHighSurrogate(text[start - 1])) {
            start -= 1
        }
        if (end > start && end < text.length && Character.isLowSurrogate(text[end]) && Character.isHighSurrogate(text[end - 1])) {
            end += 1
        }
        return start to end
    }

    fun substringSafeUtf16(text: String, offset: Int, length: Int): String {
        val (start, end) = sliceSafeUtf16(text, offset, length)
        return if (end > start) text.substring(start, end) else ""
    }

    /**
     * Parses Telegram/Markdown-lite syntax (`**bold**`, `__italic__`, `++underline++`,
     * `~~strikethrough~~`, `||spoiler||`, `` `code` ``, ` ```lang\ncode``` `, `[label](url)`)
     * into stripped display text plus exact UTF-16 [MessageEntity] spans, and augments plain
     * segments with automatic URL, `@mention`, `#hashtag`, and `/bot_command` entities.
     */
    fun parseMarkdownAndAutoEntities(raw: String): Pair<String, List<MessageEntity>> {
        if (raw.isEmpty()) return "" to emptyList()
        val out = StringBuilder(raw.length)
        val entities = mutableListOf<MessageEntity>()
        val codeExcludedRanges = mutableListOf<IntRange>()

        var i = 0
        val n = raw.length
        while (i < n) {
            // 1. Fenced code block: ```lang\ncode```
            if (raw.startsWith("```", i)) {
                val closeIdx = raw.indexOf("```", i + 3)
                if (closeIdx > i + 3) {
                    val inner = raw.substring(i + 3, closeIdx)
                    val newlineIdx = inner.indexOf('\n')
                    val (lang, body) = if (newlineIdx >= 0 && newlineIdx <= 24 && inner.substring(0, newlineIdx).all { !it.isWhitespace() }) {
                        inner.substring(0, newlineIdx).trim().ifEmpty { null } to inner.substring(newlineIdx + 1)
                    } else {
                        null to inner
                    }
                    if (body.isNotEmpty()) {
                        val startOut = out.length
                        out.append(body)
                        entities += MessageEntity(MessageEntityType.PRE, startOut, body.length, language = lang)
                        codeExcludedRanges += startOut until (startOut + body.length)
                        i = closeIdx + 3
                        continue
                    }
                }
            }

            // 2. Inline code: `code`
            if (raw[i] == '`') {
                val closeIdx = raw.indexOf('`', i + 1)
                if (closeIdx > i + 1 && !raw.substring(i + 1, closeIdx).contains('\n')) {
                    val body = raw.substring(i + 1, closeIdx)
                    val startOut = out.length
                    out.append(body)
                    entities += MessageEntity(MessageEntityType.CODE, startOut, body.length)
                    codeExcludedRanges += startOut until (startOut + body.length)
                    i = closeIdx + 1
                    continue
                }
            }

            // 3. Markdown link: [label](https://...)
            if (raw[i] == '[') {
                val closeBracket = raw.indexOf(']', i + 1)
                if (closeBracket > i + 1 && closeBracket + 1 < n && raw[closeBracket + 1] == '(') {
                    val closeParen = raw.indexOf(')', closeBracket + 2)
                    if (closeParen > closeBracket + 2) {
                        val label = raw.substring(i + 1, closeBracket)
                        val url = raw.substring(closeBracket + 2, closeParen).trim()
                        if (url.startsWith("http://") || url.startsWith("https://") || url.startsWith("tg://") || url.startsWith("ton://") || url.startsWith("fabushi://")) {
                            val startOut = out.length
                            val (parsedLabel, nested) = parseInlineMarkersOnly(label, startOut)
                            out.append(parsedLabel)
                            if (parsedLabel.isNotEmpty()) {
                                entities += MessageEntity(MessageEntityType.TEXT_URL, startOut, parsedLabel.length, url = url)
                                entities += nested
                            }
                            i = closeParen + 1
                            continue
                        }
                    }
                }
            }

            // 4. Symmetric paired delimiters: **, __, ++, ~~, ||
            val delimiter = pairedDelimiterAt(raw, i)
            if (delimiter != null) {
                val (marker, entityType) = delimiter
                val closeIdx = raw.indexOf(marker, i + marker.length)
                if (closeIdx > i + marker.length) {
                    val innerRaw = raw.substring(i + marker.length, closeIdx)
                    val startOut = out.length
                    val (parsedInner, nested) = parseInlineMarkersOnly(innerRaw, startOut)
                    if (parsedInner.isNotEmpty()) {
                        out.append(parsedInner)
                        entities += MessageEntity(entityType, startOut, parsedInner.length)
                        entities += nested
                        i = closeIdx + marker.length
                        continue
                    }
                }
            }

            out.append(raw[i])
            i += 1
        }

        val plain = out.toString()
        val autoDetected = detectAutoEntities(plain, codeExcludedRanges, entities)
        val merged = (entities + autoDetected)
            .filter { it.length > 0 && it.offset >= 0 && it.endOffset <= plain.length }
            .distinct()
            .sortedWith(compareBy<MessageEntity> { it.offset }.thenByDescending { it.length }.thenBy { it.type.ordinal })
        return plain to merged
    }

    private fun pairedDelimiterAt(text: String, index: Int): Pair<String, MessageEntityType>? {
        return when {
            text.startsWith("**", index) -> "**" to MessageEntityType.BOLD
            text.startsWith("__", index) -> "__" to MessageEntityType.ITALIC
            text.startsWith("++", index) -> "++" to MessageEntityType.UNDERLINE
            text.startsWith("~~", index) -> "~~" to MessageEntityType.STRIKETHROUGH
            text.startsWith("||", index) -> "||" to MessageEntityType.SPOILER
            else -> null
        }
    }

    private fun parseInlineMarkersOnly(segment: String, baseOffset: Int): Pair<String, List<MessageEntity>> {
        val out = StringBuilder(segment.length)
        val nested = mutableListOf<MessageEntity>()
        var i = 0
        while (i < segment.length) {
            if (segment[i] == '`') {
                val closeIdx = segment.indexOf('`', i + 1)
                if (closeIdx > i + 1) {
                    val body = segment.substring(i + 1, closeIdx)
                    val localOffset = baseOffset + out.length
                    out.append(body)
                    nested += MessageEntity(MessageEntityType.CODE, localOffset, body.length)
                    i = closeIdx + 1
                    continue
                }
            }
            val delimiter = pairedDelimiterAt(segment, i)
            if (delimiter != null) {
                val (marker, entityType) = delimiter
                val closeIdx = segment.indexOf(marker, i + marker.length)
                if (closeIdx > i + marker.length) {
                    val inner = segment.substring(i + marker.length, closeIdx)
                    val localOffset = baseOffset + out.length
                    val (parsedInner, deeper) = parseInlineMarkersOnly(inner, localOffset)
                    if (parsedInner.isNotEmpty()) {
                        out.append(parsedInner)
                        nested += MessageEntity(entityType, localOffset, parsedInner.length)
                        nested += deeper
                        i = closeIdx + marker.length
                        continue
                    }
                }
            }
            out.append(segment[i])
            i += 1
        }
        return out.toString() to nested
    }

    private fun detectAutoEntities(
        plain: String,
        excludedRanges: List<IntRange>,
        existing: List<MessageEntity>,
    ): List<MessageEntity> {
        fun overlapsCode(start: Int, end: Int): Boolean =
            excludedRanges.any { range -> start < range.last + 1 && end > range.first }

        fun overlapsLink(start: Int, end: Int): Boolean =
            existing.any { (it.type == MessageEntityType.TEXT_URL || it.type == MessageEntityType.URL) && start < it.endOffset && end > it.offset }

        val found = mutableListOf<MessageEntity>()
        autoUrlRegex.findAll(plain).forEach { match ->
            val start = match.range.first
            val end = match.range.last + 1
            if (!overlapsCode(start, end) && !overlapsLink(start, end)) {
                found += MessageEntity(MessageEntityType.URL, start, end - start, url = match.value)
            }
        }
        val urlSpans = found.toList()
        fun overlapsAnyUrl(start: Int, end: Int): Boolean =
            overlapsLink(start, end) || urlSpans.any { start < it.endOffset && end > it.offset }

        autoCommandRegex.findAll(plain).forEach { match ->
            val start = match.range.first
            val end = match.range.last + 1
            if (!overlapsCode(start, end) && !overlapsAnyUrl(start, end)) {
                found += MessageEntity(MessageEntityType.BOT_COMMAND, start, end - start)
            }
        }
        autoMentionRegex.findAll(plain).forEach { match ->
            val start = match.range.first
            val end = match.range.last + 1
            val insideCommand = found.any { it.type == MessageEntityType.BOT_COMMAND && start >= it.offset && end <= it.endOffset }
            if (!overlapsCode(start, end) && !overlapsAnyUrl(start, end) && !insideCommand) {
                found += MessageEntity(MessageEntityType.MENTION, start, end - start)
            }
        }
        autoHashtagRegex.findAll(plain).forEach { match ->
            val start = match.range.first
            val end = match.range.last + 1
            if (!overlapsCode(start, end) && !overlapsAnyUrl(start, end)) {
                found += MessageEntity(MessageEntityType.HASHTAG, start, end - start)
            }
        }
        return found
    }

    /**
     * Applies formatting to a UTF-16 selection `[selectionStart, selectionEnd)` on an existing
     * text & entity list, toggling off if the exact span already carries [type].
     */
    fun applyFormattingSelection(
        text: String,
        selectionStart: Int,
        selectionEnd: Int,
        entities: List<MessageEntity>,
        type: MessageEntityType,
        url: String? = null,
        language: String? = null,
    ): Pair<String, List<MessageEntity>> {
        val lo = min(selectionStart, selectionEnd).coerceIn(0, text.length)
        val hi = max(selectionStart, selectionEnd).coerceIn(0, text.length)
        val (safeStart, safeEnd) = sliceSafeUtf16(text, lo, hi - lo)
        if (safeEnd <= safeStart) return text to entities
        val length = safeEnd - safeStart
        val existingIndex = entities.indexOfFirst {
            it.type == type && it.offset == safeStart && it.length == length
        }
        val updated = if (existingIndex >= 0) {
            entities.filterIndexed { index, _ -> index != existingIndex }
        } else {
            (entities + MessageEntity(type, safeStart, length, url = url, language = language))
                .sortedWith(compareBy<MessageEntity> { it.offset }.thenByDescending { it.length })
        }
        return text to updated
    }

    /**
     * Wraps the selected UTF-16 range `[selectionStart, selectionEnd)` with Telegram Markdown
     * delimiters in composer draft text so users can see or edit formatting inline.
     */
    fun wrapDraftSelectionWithMarkdown(
        draft: String,
        selectionStart: Int,
        selectionEnd: Int,
        type: MessageEntityType,
        url: String? = null,
        language: String? = null,
    ): String {
        val lo = min(selectionStart, selectionEnd).coerceIn(0, draft.length)
        val hi = max(selectionStart, selectionEnd).coerceIn(0, draft.length)
        val (start, end) = sliceSafeUtf16(draft, lo, hi - lo)
        val selected = if (end > start) draft.substring(start, end) else "文本"
        val wrapped = when (type) {
            MessageEntityType.BOLD -> "**$selected**"
            MessageEntityType.ITALIC -> "__${selected}__"
            MessageEntityType.UNDERLINE -> "++${selected}++"
            MessageEntityType.STRIKETHROUGH -> "~~${selected}~~"
            MessageEntityType.SPOILER -> "||${selected}||"
            MessageEntityType.CODE -> "`$selected`"
            MessageEntityType.PRE -> "```${language.orEmpty()}\n$selected\n```"
            MessageEntityType.TEXT_URL, MessageEntityType.URL -> "[$selected](${url ?: "https://fabushi.ombhrum.com"})"
            MessageEntityType.MENTION -> if (selected.startsWith("@")) selected else "@$selected"
            MessageEntityType.HASHTAG -> if (selected.startsWith("#")) selected else "#$selected"
            MessageEntityType.BOT_COMMAND -> if (selected.startsWith("/")) selected else "/$selected"
        }
        return draft.substring(0, start) + wrapped + draft.substring(end)
    }
}

// ============================================================================
// 2. Telegram 5-Bit 100-Bar Voice Waveform Codec
// ============================================================================

object TelegramWaveformCodec {
    const val BAR_COUNT = 100
    const val MAX_BAR_VALUE = 31 // 5-bit unsigned: 0..31

    /**
     * Resamples arbitrary amplitude values into 100 5-bit bars (`0..31`).
     * Automatically scales values > 31 (e.g. `0..255` or `0..32767`) into `0..31`.
     */
    fun normalizeTo100Bars(samples: List<Int>, seedHint: String = "voice"): List<Int> {
        if (samples.isEmpty()) return syntheticSpeechBars(seedHint)
        val maxVal = samples.maxOf { it.coerceAtLeast(0) }
        val scaledInput = samples.map { raw ->
            val v = raw.coerceAtLeast(0)
            when {
                maxVal <= MAX_BAR_VALUE -> v
                maxVal <= 255 -> ((v * MAX_BAR_VALUE) / 255.0).roundToInt()
                else -> ((v.toDouble() / maxVal.toDouble()) * MAX_BAR_VALUE).roundToInt()
            }.coerceIn(0, MAX_BAR_VALUE)
        }
        if (scaledInput.size == BAR_COUNT) return scaledInput
        return List(BAR_COUNT) { barIdx ->
            val pos = (barIdx.toDouble() / (BAR_COUNT - 1).coerceAtLeast(1)) * (scaledInput.size - 1)
            val left = pos.toInt().coerceIn(0, scaledInput.lastIndex)
            val right = (left + 1).coerceIn(0, scaledInput.lastIndex)
            val frac = pos - left
            (scaledInput[left] * (1.0 - frac) + scaledInput[right] * frac).roundToInt().coerceIn(0, MAX_BAR_VALUE)
        }
    }

    /**
     * Packs 5-bit integers (`0..31`) into Telegram's compact bitstream (`100 * 5 = 500 bits = 63 bytes`).
     */
    fun encode5Bit(bars: List<Int>): ByteArray {
        val normalized = if (bars.size == BAR_COUNT) {
            bars.map { it.coerceIn(0, MAX_BAR_VALUE) }
        } else {
            normalizeTo100Bars(bars)
        }
        val totalBits = normalized.size * 5
        val out = ByteArray((totalBits + 7) / 8)
        normalized.forEachIndexed { index, value ->
            val v = value and 0x1F
            val bitOffset = index * 5
            for (bit in 0 until 5) {
                if (((v shr bit) and 1) != 0) {
                    val targetBit = bitOffset + bit
                    out[targetBit / 8] = (out[targetBit / 8].toInt() or (1 shl (targetBit % 8))).toByte()
                }
            }
        }
        return out
    }

    /**
     * Unpacks a Telegram 5-bit bitstream into [barCount] integers in `0..31`.
     */
    fun decode5Bit(bytes: ByteArray, barCount: Int = BAR_COUNT): List<Int> {
        if (bytes.isEmpty() || barCount <= 0) return List(barCount.coerceAtLeast(0)) { 0 }
        val totalBitsAvailable = bytes.size * 8
        return List(barCount) { index ->
            val bitOffset = index * 5
            var value = 0
            for (bit in 0 until 5) {
                val sourceBit = bitOffset + bit
                if (sourceBit < totalBitsAvailable) {
                    val b = bytes[sourceBit / 8].toInt() and 0xFF
                    if (((b shr (sourceBit % 8)) and 1) != 0) {
                        value = value or (1 shl bit)
                    }
                }
            }
            value.coerceIn(0, MAX_BAR_VALUE)
        }
    }

    fun encodeBase64(bars: List<Int>): String =
        Base64.getEncoder().encodeToString(encode5Bit(bars))

    fun decodeBase64(encoded: String, barCount: Int = BAR_COUNT): List<Int> =
        runCatching { decode5Bit(Base64.getDecoder().decode(encoded.trim()), barCount) }
            .getOrElse { syntheticSpeechBars(encoded) }

    /**
     * Converts a PCM-16 frame into a perceptual Telegram `0..31` bar height.
     */
    fun computeRmsBar31(pcm16: ShortArray, readCount: Int = pcm16.size): Int {
        val n = readCount.coerceIn(0, pcm16.size)
        if (n == 0) return 0
        var sumSq = 0.0
        for (i in 0 until n) {
            val sample = pcm16[i].toDouble() / 32768.0
            sumSq += sample * sample
        }
        val rms = sqrt(sumSq / n)
        if (rms <= 0.0005) return 1
        // Logarithmic speech curve mapping [-54dB .. 0dB] -> [1 .. 31]
        val db = (20.0 * ln(rms) / ln(10.0)).coerceIn(-54.0, 0.0)
        return (((db + 54.0) / 54.0) * MAX_BAR_VALUE).roundToInt().coerceIn(1, MAX_BAR_VALUE)
    }

    fun syntheticSpeechBars(seed: String): List<Int> {
        val hash = abs(seed.hashCode())
        return List(BAR_COUNT) { idx ->
            val envelope = sin((idx.toDouble() / BAR_COUNT) * PI)
            val cadence = abs(sin((idx + (hash % 19)) * 0.37)) * 0.65 + abs(sin((idx + (hash % 7)) * 0.91)) * 0.35
            (3 + (envelope * cadence * 27.0).roundToInt()).coerceIn(2, MAX_BAR_VALUE)
        }
    }
}

// ============================================================================
// 3. E2EE & VoIP Call 333-Emoji Fingerprint Verification
// ============================================================================

object CallsEmojiFingerprint {
    /**
     * Curated 333 distinct visual emojis used for Telegram VoIP/Video Call & Secret Chat SAS verification.
     */
    val EMOJI_TABLE_333: List<String> = listOf(
        "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵",
        "🐔", "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋",
        "🐌", "🐞", "🐜", "🐢", "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐡", "🐠",
        "🐟", "🐬", "🐳", "🦈", "🐊", "🐅", "🐆", "🦓", "🦍", "🦧", "🐘", "🦛", "🦏", "🐪", "🐫",
        "🦒", "🦘", "🐃", "🐂", "🐄", "🐎", "🐖", "🐏", "🐑", "🦙", "🐐", "🦌", "🐕", "🐩", "🐈",
        "🐓", "🦃", "🦚", "🦜", "🦢", "🦩", "🕊️", "🐇", "🦝", "🦨", "🦡", "🦦", "🦥", "🐁", "🐀",
        "🐿️", "🦔", "🌵", "🎄", "🌲", "🌳", "🌴", "🌱", "🌿", "☘️", "🍀", "🎍", "🎋", "🍃", "🍂",
        "🍁", "🍄", "🐚", "🌾", "💐", "🌷", "🌹", "🥀", "🌺", "🌸", "🌼", "🌻", "🌞", "🌝", "🌛",
        "🌜", "🌚", "🌕", "🌖", "🌗", "🌘", "🌑", "🌒", "🌓", "🌔", "🌙", "🌎", "🌍", "🌏", "🪐",
        "💫", "⭐", "🌟", "✨", "⚡", "☄️", "💥", "🔥", "🌪️", "🌈", "☀️", "⛅", "☁️", "❄️", "☃️",
        "⛄", "🌬️", "💧", "🌊", "🍎", "🍐", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🍈", "🍒", "🍑",
        "🥭", "🍍", "🥥", "🥝", "🍅", "🍆", "🥑", "🥦", "🥬", "🥒", "🌶️", "🌽", "🥕", "🧄", "🧅",
        "🥔", "🍠", "🥐", "🥯", "🍞", "🥖", "🥨", "🧀", "🥚", "🍳", "🧈", "🥞", "🧇", "🥓", "🥩",
        "🍗", "🍖", "🌭", "🍔", "🍟", "🍕", "🥪", "🥙", "🧆", "🌮", "🌯", "🥗", "🥘", "🍝", "🍜",
        "🍲", "🍛", "🍣", "🍱", "🥟", "🍤", "🍙", "🍚", "🍘", "🍥", "🥠", "🥮", "🍢", "🍡", "🍧",
        "🍨", "🍦", "🥧", "🧁", "🍰", "🎂", "🍮", "🍭", "🍬", "🍫", "🍿", "🍩", "🍪", "🌰", "🥜",
        "🍯", "🥛", "☕", "🍵", "🧃", "🥤", "🍶", "🍺", "🍷", "🥂", "🥃", "🍸", "🍹", "🧉", "🍾",
        "🧊", "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🏒",
        "🏑", "🥍", "🏏", "🥅", "⛳", "🪁", "🏹", "🎣", "🤿", "🥊", "🥋", "🎽", "🛹", "🛼", "🛷",
        "⛸️", "🥌", "🎿", "🏆", "🥇", "🥈", "🥉", "🏅", "🎖️", "🏵️", "🎗️", "🎫", "🎟️", "🎪", "🎭",
        "🎨", "🎬", "🎤", "🎧", "🎼", "🎹", "🥁", "🎷", "🎺", "🎸", "🪕", "🎻", "🎲", "♟️", "🎯",
        "🎳", "🎮", "🎰", "🧩", "🚗", "🚕", "🚙", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚐", "🚚",
        "🚃", "🚋", "🚞",
    )

    init {
        check(EMOJI_TABLE_333.size == 333) { "EMOJI_TABLE_333 must contain exactly 333 entries" }
    }

    /**
     * Derives the 4-glyph E2EE call verification fingerprint using SHA-256 over the shared key seed.
     */
    fun deriveCallEmojis(sharedKeyOrSeed: String): List<String> {
        val digest = MessageDigest.getInstance("SHA-256").digest(sharedKeyOrSeed.toByteArray(Charsets.UTF_8))
        return List(4) { block ->
            val offset = block * 8
            var acc = 0L
            for (b in 0 until 8) {
                acc = (acc shl 8) or (digest[offset + b].toLong() and 0xFFL)
            }
            val unsignedIndex = ((acc ushr 1) % EMOJI_TABLE_333.size).toInt()
            EMOJI_TABLE_333[unsignedIndex]
        }
    }

    /**
     * Derives an 8x8 = 64-cell 2-bit color identicon (`0..3`) and formatted hex digest for Secret Chats.
     */
    fun deriveSecretChatIdenticon(seed: String): SecretChatFingerprint {
        val digest = MessageDigest.getInstance("SHA-256").digest(seed.toByteArray(Charsets.UTF_8))
        val cells = ArrayList<Int>(64)
        for (byteIndex in 0 until 16) {
            val v = digest[byteIndex].toInt() and 0xFF
            cells += (v and 0x03)
            cells += ((v shr 2) and 0x03)
            cells += ((v shr 4) and 0x03)
            cells += ((v shr 6) and 0x03)
        }
        val hexGroups = digest.joinToString("") { "%02X".format(it) }.chunked(4).chunked(4).joinToString("\n") { it.joinToString("  ") }
        return SecretChatFingerprint(
            cells8x8 = cells,
            emojis4 = deriveCallEmojis("secret:$seed"),
            hexBlocks = hexGroups,
            protocolSummary = "X3DH (X25519 + Ed25519) · Double Ratchet (ChaCha20-Poly1305)",
        )
    }
}

data class SecretChatFingerprint(
    val cells8x8: List<Int>,
    val emojis4: List<String>,
    val hexBlocks: String,
    val protocolSummary: String,
)

// ============================================================================
// 4. Multi-Account Generation Fence & Folder Filtering
// ============================================================================

class GenerationFence(initial: Long = 1L) {
    private val counter = AtomicLong(initial)

    val currentGeneration: Long get() = counter.get()

    fun advance(): Long = counter.incrementAndGet()

    fun isCurrent(capturedGeneration: Long): Boolean = counter.get() == capturedGeneration
}

data class TelegramAccount(
    val id: String,
    val slotIndex: Int,
    val displayName: String,
    val username: String,
    val phone: String,
    val bio: String = "",
    val avatarSeed: String = id,
    val unreadCount: Int = 0,
    val isActive: Boolean = false,
    val isPremium: Boolean = true,
    val statusText: String = "在线",
)

data class TelegramFolderTab(
    val id: String,
    val title: String,
    val icon: String? = null,
    val isBuiltIn: Boolean = false,
    val conversationIds: List<String> = emptyList(),
    val includeContacts: Boolean = false,
    val includeBots: Boolean = false,
    val includeGroups: Boolean = false,
    val includeChannels: Boolean = false,
    val excludeMuted: Boolean = false,
    val excludeRead: Boolean = false,
    val excludeArchived: Boolean = true,
)

object TelegramFolderFilter {
    val DEFAULT_BUILT_IN_TABS: List<TelegramFolderTab> = listOf(
        TelegramFolderTab(id = "all", title = "全部", isBuiltIn = true),
        TelegramFolderTab(id = "personal", title = "私聊", isBuiltIn = true, includeContacts = true),
        TelegramFolderTab(id = "unread", title = "未读", isBuiltIn = true, excludeRead = true),
        TelegramFolderTab(id = "bots", title = "机器人", isBuiltIn = true, includeBots = true),
        TelegramFolderTab(id = "groups", title = "群组", isBuiltIn = true, includeGroups = true),
        TelegramFolderTab(id = "channels", title = "频道", isBuiltIn = true, includeChannels = true),
    )

    fun matches(folder: TelegramFolderTab, chat: TelegramChatItem): Boolean {
        if (folder.excludeArchived && chat.isArchived && folder.id !in chat.folderIds) return false
        if (folder.id == "all") return !chat.isArchived
        if (folder.id in chat.folderIds || chat.id in folder.conversationIds) return true
        if (folder.excludeMuted && chat.isMuted) return false
        if (folder.excludeRead && chat.unreadCount <= 0 && !chat.markedUnread) return false

        return when (folder.id) {
            "personal" -> (chat.kind == ConversationKind.DIRECT || chat.kind == ConversationKind.SECRET || chat.kind == ConversationKind.SAVED_MESSAGES) && !chat.isBot
            "unread" -> chat.unreadCount > 0 || chat.markedUnread
            "bots" -> chat.isBot
            "groups" -> chat.kind == ConversationKind.GROUP
            "channels" -> chat.kind == ConversationKind.CHANNEL
            else -> {
                val anyCategoryFilter = folder.includeContacts || folder.includeBots || folder.includeGroups || folder.includeChannels
                if (!anyCategoryFilter) {
                    chat.id in folder.conversationIds || folder.id in chat.folderIds
                } else {
                    (folder.includeContacts && (chat.kind == ConversationKind.DIRECT || chat.kind == ConversationKind.SECRET) && !chat.isBot) ||
                        (folder.includeBots && chat.isBot) ||
                        (folder.includeGroups && chat.kind == ConversationKind.GROUP) ||
                        (folder.includeChannels && chat.kind == ConversationKind.CHANNEL)
                }
            }
        }
    }
}

// ============================================================================
// 5. Core Telegram Chats, Stories, Topics, Messages & Keyboards
// ============================================================================

enum class GlobalSearchFilter(val label: String) {
    CHATS("对话"),
    MEDIA("媒体"),
    LINKS("链接"),
    FILES("文件"),
    MUSIC("音乐"),
    VOICE("语音"),
}

data class TelegramStoryItem(
    val id: String,
    val authorId: String,
    val authorName: String,
    val avatarSeed: String,
    val caption: String,
    val headline: String,
    val gradientIndex: Int = 0,
    val createdAtMs: Long = System.currentTimeMillis(),
    val expiresAtMs: Long = System.currentTimeMillis() + 86_400_000L,
    val viewed: Boolean = false,
    val isMine: Boolean = false,
    val viewCount: Int = 18,
    val reactionsCount: Int = 5,
)

data class TelegramForumTopic(
    val id: String,
    val conversationId: String,
    val title: String,
    val iconEmoji: String,
    val accentColorHex: Long = 0xFF2AABEEL,
    val unreadCount: Int = 0,
    val isPinned: Boolean = false,
    val isClosed: Boolean = false,
    val lastPreview: String = "",
)

enum class InlineKeyboardButtonKind {
    CALLBACK,
    URL,
    MINI_APP,
    SWITCH_INLINE,
    MIKA_ACTION,
}

data class InlineKeyboardButtonModel(
    val id: String,
    val text: String,
    val kind: InlineKeyboardButtonKind = InlineKeyboardButtonKind.CALLBACK,
    val payload: String = "",
)

data class InlineKeyboardRow(
    val buttons: List<InlineKeyboardButtonModel>,
) {
    val boundedButtons: List<InlineKeyboardButtonModel>
        get() = buttons.take(8) // Telegram 1..8 inline columns rule
}

data class TelegramReplyHeader(
    val replyToMessageId: String,
    val senderName: String,
    val quoteText: String,
    val mediaBadge: String? = null,
)

data class TelegramForwardHeader(
    val originName: String,
    val originChannelId: String? = null,
    val dateLabel: String? = null,
)

data class TelegramPollOption(
    val id: String,
    val text: String,
    val voterCount: Int,
    val chosen: Boolean = false,
    val isCorrect: Boolean = false,
)

data class TelegramPollModel(
    val question: String,
    val options: List<TelegramPollOption>,
    val isAnonymous: Boolean = true,
    val multipleAnswers: Boolean = false,
    val isQuiz: Boolean = false,
    val correctOptionId: String? = null,
    val explanation: String? = null,
    val closed: Boolean = false,
) {
    val totalVotes: Int get() = options.sumOf { it.voterCount }.coerceAtLeast(if (options.any { it.chosen }) 1 else 0)
    val hasVoted: Boolean get() = options.any { it.chosen } || closed
}

data class TelegramChatItem(
    val id: String,
    val title: String,
    val subtitle: String,
    val preview: String,
    val time: String,
    val kind: ConversationKind,
    val unreadCount: Int = 0,
    val mentionCount: Int = 0,
    val isPinned: Boolean = false,
    val isMuted: Boolean = false,
    val isArchived: Boolean = false,
    val markedUnread: Boolean = false,
    val isVerified: Boolean = false,
    val isBot: Boolean = false,
    val isCompanionGhost: Boolean = false,
    val isForum: Boolean = false,
    val isSecretChat: Boolean = false,
    val secretVerified: Boolean = false,
    val secretFingerprintSeed: String = id,
    val online: Boolean = false,
    val memberCount: Int = 0,
    val subscriberCount: Int = 0,
    val ttlSeconds: Int = 0,
    val lastMessageOutgoing: Boolean = false,
    val lastDeliveryState: String = "read",
    val draftText: String = "",
    val typingLabel: String? = null,
    val pinnedMessageIds: List<String> = emptyList(),
    val folderIds: List<String> = emptyList(),
    val username: String? = null,
    val miniAppPluginId: String? = null,
    val rawSummary: ConversationSummary? = null,
) {
    fun toConversationSummary(): ConversationSummary =
        rawSummary ?: ConversationSummary(
            id = id,
            title = title,
            description = subtitle,
            participants = listOf(ConversationParticipant(actorId = id, role = "member", joinedAtMs = 0L)),
            preview = preview,
            time = time,
            badge = title.take(1).uppercase().ifBlank { "T" },
            kind = kind,
            unreadCount = unreadCount,
            isPinned = isPinned,
            isMuted = isMuted,
            isArchived = isArchived,
            pinnedMessageIds = pinnedMessageIds,
            folderIds = folderIds,
            markedUnread = markedUnread,
        )
}

data class TelegramMessageItem(
    val id: String,
    val conversationId: String,
    val senderId: String,
    val senderName: String,
    val text: String,
    val entities: List<MessageEntity> = emptyList(),
    val contentType: String = "text",
    val outgoing: Boolean,
    val time: String,
    val timestampMs: Long = System.currentTimeMillis(),
    val deliveryState: String = "read", // sending | sent | delivered | read
    val edited: Boolean = false,
    val pinned: Boolean = false,
    val silent: Boolean = false,
    val scheduledAtMs: Long? = null,
    val ttlSeconds: Int? = null,
    val topicId: String? = null,
    val replyHeader: TelegramReplyHeader? = null,
    val forwardHeader: TelegramForwardHeader? = null,
    val reactions: List<ChatReaction> = emptyList(),
    val inlineKeyboard: List<InlineKeyboardRow> = emptyList(),
    val poll: TelegramPollModel? = null,
    val waveformBars: List<Int> = emptyList(),
    val durationSeconds: Int = 0,
    val mediaFileName: String? = null,
    val mediaBlobId: String? = null,
    val mediaMimeType: String? = null,
    val mediaSizeBytes: Int = 0,
    val stickerEmoji: String? = null,
    val stickerPackTitle: String? = null,
    val contactName: String? = null,
    val contactPhone: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
    val venueTitle: String? = null,
    val viewsCount: Int? = null,
    val commentsCount: Int? = null,
    val translatedText: String? = null,
    val mikaTaskId: String? = null,
    val mikaTaskStatus: String? = null,
    val mikaStepLogs: List<String> = emptyList(),
) {
    fun toLegacyChatMessage(): ChatMessage = ChatMessage(
        id = id,
        conversationId = conversationId,
        text = text,
        contentType = when (contentType) {
            "videoNote" -> "video"
            else -> contentType
        },
        mediaFileName = mediaFileName,
        mediaBlobId = mediaBlobId,
        mediaMimeType = mediaMimeType,
        mediaSizeBytes = mediaSizeBytes,
        contactName = contactName,
        latitude = latitude,
        longitude = longitude,
        pollQuestion = poll?.question,
        pollOptions = poll?.options?.map { ChatPollOption(it.id, it.text, it.voterCount, it.chosen) }.orEmpty(),
        pollMultipleAnswers = poll?.multipleAnswers ?: false,
        outgoing = outgoing,
        time = time,
        replyToMessageId = replyHeader?.replyToMessageId,
        forwardOrigin = forwardHeader?.originName,
        reactions = reactions,
        deliveryState = deliveryState,
        edited = edited,
        pinned = pinned,
    )
}

data class SlashCommandSuggestion(
    val command: String,
    val description: String,
    val botHandle: String? = null,
)

data class StickerPackModel(
    val id: String,
    val title: String,
    val installed: Boolean = true,
    val stickers: List<String>,
)

data class GifItemModel(
    val id: String,
    val title: String,
    val previewGlyph: String,
    val accentHex: Long,
)

data class ChannelStatisticsSnapshot(
    val conversationId: String,
    val subscribers: Int,
    val growth7dPercent: Double,
    val notificationsEnabledPercent: Double,
    val avgViewsPerPost: Int,
    val sharesPerPost: Int,
    val reactionsPerPost: Int,
    val dailyViewsBars: List<Int>,
)

// ============================================================================
// 6. Audio Player, Voice Recording, VoIP Calls & <60ms VAD Talk Mode
// ============================================================================

data class FloatingAudioPlaybackState(
    val active: Boolean = false,
    val messageId: String = "",
    val conversationId: String = "",
    val title: String = "",
    val subtitle: String = "",
    val isPlaying: Boolean = false,
    val progressFraction: Float = 0f,
    val durationSeconds: Int = 0,
    val speedMultiplier: Float = 1.0f,
)

enum class ComposerRecordMode {
    VOICE_MIC,
    ROUND_VIDEO_NOTE,
}

data class ActiveRecordingState(
    val isRecording: Boolean = false,
    val mode: ComposerRecordMode = ComposerRecordMode.VOICE_MIC,
    val lockedHandsFree: Boolean = false,
    val elapsedMs: Long = 0L,
    val dragOffsetX: Float = 0f,
    val dragOffsetY: Float = 0f,
    val liveBars: List<Int> = emptyList(),
)

enum class CallPhase {
    IDLE,
    OUTGOING_RINGING,
    CONNECTING,
    ACTIVE,
    ENDED,
}

data class CallParticipantTile(
    val id: String,
    val displayName: String,
    val muted: Boolean = false,
    val videoOn: Boolean = false,
    val speaking: Boolean = false,
    val screenSharing: Boolean = false,
)

data class TelegramCallUiState(
    val phase: CallPhase = CallPhase.IDLE,
    val callId: String = "",
    val conversationId: String = "",
    val peerTitle: String = "",
    val isGroupCall: Boolean = false,
    val isVideoCall: Boolean = false,
    val isScreenSharing: Boolean = false,
    val isMuted: Boolean = false,
    val isSpeakerOn: Boolean = true,
    val isMinimized: Boolean = false,
    val durationSeconds: Int = 0,
    val emojiFingerprint: List<String> = emptyList(),
    val participants: List<CallParticipantTile> = emptyList(),
    val networkQualityBars: Int = 4,
)

enum class TalkVadPhase(val label: String) {
    LISTENING("聆听中 · 随时打断"),
    USER_SPEAKING("正在检测语音输入…"),
    AI_THINKING("Hermes 极速推理中…"),
    AI_SPEAKING("AI 正在播报 · 说话即 <60ms 打断"),
}

data class TalkModeOrbState(
    val visible: Boolean = false,
    val phase: TalkVadPhase = TalkVadPhase.LISTENING,
    val vadLatencyMs: Int = 38,
    val rmsLevel: Float = 0.18f,
    val userTranscript: String = "",
    val assistantReply: String = "您好！已开启全双工 <60ms VAD 实时语音对话模式，随时开口即可无感打断。",
)

// ============================================================================
// 7. TON Wallet, Telegram Stars & TonConnect 2.0 Models
// ============================================================================

data class TonTransactionItem(
    val id: String,
    val direction: String, // "in" | "out" | "swap" | "stake"
    val title: String,
    val peerAddress: String,
    val amountLabel: String,
    val asset: String, // "TON" | "STARS" | "USDT"
    val timestampLabel: String,
    val memo: String = "",
    val bocHash: String = "",
)

data class TonCollectibleItem(
    val id: String,
    val name: String,
    val collection: String,
    val floorTon: Double,
    val badgeEmoji: String,
    val isGift: Boolean = false,
)

data class TonConnectPreview(
    val rawUri: String,
    val dappName: String,
    val dappDomain: String,
    val actionSummary: String,
    val estimatedDelta: String,
    val networkFeeTon: String,
    val riskNotice: String,
)

data class TonWalletUiState(
    val initialized: Boolean = false,
    val addressFriendly: String = "",
    val mnemonicWords: List<String> = emptyList(),
    val tonBalance: Double = 0.0,
    val usdtBalance: Double = 0.0,
    val starsBalance: Long = 0L,
    val stakedTon: Double = 0.0,
    val stakingApyPercent: Double = 0.0,
    val transactions: List<TonTransactionItem> = emptyList(),
    val collectibles: List<TonCollectibleItem> = emptyList(),
    val pendingTonConnect: TonConnectPreview? = null,
    val statusBanner: String? = null,
)

// ============================================================================
// 8. 全球法布施 (Global Dharma MiniApp) — 240+ Countries & Zen Room
// ============================================================================

data class CountryOfferingNode(
    val code: String,
    val nameZh: String,
    val nameEn: String,
    val region: String,
    val latitude: Double,
    val longitude: Double,
    val offeringsCount: Long,
    val lampLitByMe: Boolean = false,
)

data class OfferingArc(
    val id: String,
    val fromLat: Double,
    val fromLon: Double,
    val toLat: Double,
    val toLon: Double,
    val createdAtMs: Long,
    val label: String,
)

data class SutraTextItem(
    val id: String,
    val title: String,
    val tradition: String,
    val durationMinutes: Int,
    val content: String,
    val dedicationVerse: String,
)

data class MeritLeaderboardEntry(
    val rank: Int,
    val name: String,
    val countryCode: String,
    val meritPoints: Long,
    val wheelsSpun: Long,
)

data class DharmaAchievement(
    val id: String,
    val title: String,
    val description: String,
    val icon: String,
    val unlocked: Boolean,
)

data class GlobalDharmaUiState(
    val countries: List<CountryOfferingNode> = GlobalDharmaCatalog.build240Countries(),
    val selectedCountryCode: String = "CN",
    val activeArcs: List<OfferingArc> = emptyList(),
    val totalGlobalOfferings: Long = 0L,
    val myMeritPoints: Long = 0L,
    val prayerWheelSpins: Long = 0L,
    val mantraCount: Long = 0L,
    val practiceMinutesTotal: Int = 0,
    val woodenFishHits: Long = 0L,
    val sutras: List<SutraTextItem> = GlobalDharmaCatalog.DEFAULT_SUTRAS,
    val leaderboard: List<MeritLeaderboardEntry> = emptyList(),
    val achievements: List<DharmaAchievement> = emptyList(),
    val aiDharmaMessages: List<Pair<Boolean, String>> = emptyList(),
)

object GlobalDharmaCatalog {
    private data class SeedCountry(val code: String, val zh: String, val en: String, val region: String, val lat: Double, val lon: Double)

    private val NAMED_COUNTRIES = listOf(
        SeedCountry("CN", "中国", "China", "东亚", 35.86, 104.19),
        SeedCountry("IN", "印度", "India", "南亚", 20.59, 78.96),
        SeedCountry("NP", "尼泊尔", "Nepal", "南亚", 28.39, 84.12),
        SeedCountry("BT", "不丹", "Bhutan", "南亚", 27.51, 90.43),
        SeedCountry("LK", "斯里兰卡", "Sri Lanka", "南亚", 7.87, 80.77),
        SeedCountry("TH", "泰国", "Thailand", "东南亚", 15.87, 100.99),
        SeedCountry("MM", "缅甸", "Myanmar", "东南亚", 21.91, 95.95),
        SeedCountry("KH", "柬埔寨", "Cambodia", "东南亚", 12.56, 104.99),
        SeedCountry("LA", "老挝", "Laos", "东南亚", 19.85, 102.49),
        SeedCountry("VN", "越南", "Vietnam", "东南亚", 14.05, 108.27),
        SeedCountry("SG", "新加坡", "Singapore", "东南亚", 1.35, 103.82),
        SeedCountry("MY", "马来西亚", "Malaysia", "东南亚", 4.21, 101.97),
        SeedCountry("ID", "印度尼西亚", "Indonesia", "东南亚", -0.78, 113.92),
        SeedCountry("JP", "日本", "Japan", "东亚", 36.20, 138.25),
        SeedCountry("KR", "韩国", "South Korea", "东亚", 35.90, 127.76),
        SeedCountry("MN", "蒙古", "Mongolia", "东亚", 46.86, 103.84),
        SeedCountry("HK", "中国香港", "Hong Kong", "东亚", 22.31, 114.16),
        SeedCountry("MO", "中国澳门", "Macau", "东亚", 22.19, 113.54),
        SeedCountry("TW", "中国台湾", "Taiwan", "东亚", 23.69, 120.96),
        SeedCountry("US", "美国", "United States", "北美", 37.09, -95.71),
        SeedCountry("CA", "加拿大", "Canada", "北美", 56.13, -106.34),
        SeedCountry("MX", "墨西哥", "Mexico", "拉美", 23.63, -102.55),
        SeedCountry("BR", "巴西", "Brazil", "南美", -14.23, -51.92),
        SeedCountry("AR", "阿根廷", "Argentina", "南美", -38.41, -63.61),
        SeedCountry("CL", "智利", "Chile", "南美", -35.67, -71.54),
        SeedCountry("PE", "秘鲁", "Peru", "南美", -9.19, -75.01),
        SeedCountry("CO", "哥伦比亚", "Colombia", "南美", 4.57, -74.29),
        SeedCountry("GB", "英国", "United Kingdom", "欧洲", 55.37, -3.43),
        SeedCountry("FR", "法国", "France", "欧洲", 46.22, 2.21),
        SeedCountry("DE", "德国", "Germany", "欧洲", 51.16, 10.45),
        SeedCountry("CH", "瑞士", "Switzerland", "欧洲", 46.81, 8.22),
        SeedCountry("IT", "意大利", "Italy", "欧洲", 41.87, 12.56),
        SeedCountry("ES", "西班牙", "Spain", "欧洲", 40.46, -3.74),
        SeedCountry("NL", "荷兰", "Netherlands", "欧洲", 52.13, 5.29),
        SeedCountry("SE", "瑞典", "Sweden", "欧洲", 60.12, 18.64),
        SeedCountry("NO", "挪威", "Norway", "欧洲", 60.47, 8.46),
        SeedCountry("RU", "俄罗斯", "Russia", "欧亚", 61.52, 105.31),
        SeedCountry("TR", "土耳其", "Turkey", "西亚", 38.96, 35.24),
        SeedCountry("AE", "阿联酋", "UAE", "中东", 23.42, 53.84),
        SeedCountry("SA", "沙特阿拉伯", "Saudi Arabia", "中东", 23.88, 45.07),
        SeedCountry("EG", "埃及", "Egypt", "非洲", 26.82, 30.80),
        SeedCountry("ZA", "南非", "South Africa", "非洲", -30.55, 22.93),
        SeedCountry("KE", "肯尼亚", "Kenya", "非洲", -0.02, 37.90),
        SeedCountry("NG", "尼日利亚", "Nigeria", "非洲", 9.08, 8.67),
        SeedCountry("MA", "摩洛哥", "Morocco", "非洲", 31.79, -7.09),
        SeedCountry("AU", "澳大利亚", "Australia", "大洋洲", -25.27, 133.77),
        SeedCountry("NZ", "新西兰", "New Zealand", "大洋洲", -40.90, 174.88),
        SeedCountry("FJ", "斐济", "Fiji", "大洋洲", -17.71, 178.06),
        SeedCountry("IS", "冰岛", "Iceland", "欧洲", 64.96, -19.02),
        SeedCountry("PT", "葡萄牙", "Portugal", "欧洲", 39.39, -8.22),
    )

    private val ISO_REGION_MATRIX = listOf(
        "AF" to ("阿富汗" to (33.9 to 67.7)), "AL" to ("阿尔巴尼亚" to (41.1 to 20.1)),
        "DZ" to ("阿尔及利亚" to (28.0 to 1.6)), "AD" to ("安道尔" to (42.5 to 1.5)),
        "AO" to ("安哥拉" to (-11.2 to 17.8)), "AT" to ("奥地利" to (47.5 to 14.5)),
        "AZ" to ("阿塞拜疆" to (40.1 to 47.5)), "BS" to ("巴哈马" to (25.0 to -77.3)),
        "BH" to ("巴林" to (26.0 to 50.5)), "BD" to ("孟加拉国" to (23.6 to 90.3)),
        "BE" to ("比利时" to (50.5 to 4.4)), "BO" to ("玻利维亚" to (-16.2 to -63.5)),
        "BA" to ("波黑" to (43.9 to 17.6)), "BW" to ("博茨瓦纳" to (-22.3 to 24.6)),
        "BN" to ("文莱" to (4.5 to 114.7)), "BG" to ("保加利亚" to (42.7 to 25.4)),
        "CM" to ("喀麦隆" to (7.3 to 12.3)), "CR" to ("哥斯达黎加" to (9.7 to -83.7)),
        "HR" to ("克罗地亚" to (45.1 to 15.2)), "CU" to ("古巴" to (21.5 to -77.7)),
        "CY" to ("塞浦路斯" to (35.1 to 33.4)), "CZ" to ("捷克" to (49.8 to 15.4)),
        "DK" to ("丹麦" to (56.2 to 9.5)), "DO" to ("多米尼加" to (18.7 to -70.1)),
        "EC" to ("厄瓜多尔" to (-1.8 to -78.1)), "EE" to ("爱沙尼亚" to (58.5 to 25.0)),
        "ET" to ("埃塞俄比亚" to (9.1 to 40.4)), "FI" to ("芬兰" to (61.9 to 25.7)),
        "GE" to ("格鲁吉亚" to (42.3 to 43.3)), "GH" to ("加纳" to (7.9 to -1.0)),
        "GR" to ("希腊" to (39.0 to 21.8)), "GT" to ("危地马拉" to (15.7 to -90.2)),
        "HU" to ("匈牙利" to (47.1 to 19.5)), "IE" to ("爱尔兰" to (53.1 to -7.6)),
        "IL" to ("以色列" to (31.0 to 34.8)), "JM" to ("牙买加" to (18.1 to -77.2)),
        "JO" to ("约旦" to (30.5 to 36.2)), "KZ" to ("哈萨克斯坦" to (48.0 to 66.9)),
        "KW" to ("科威特" to (29.3 to 47.4)), "KG" to ("吉尔吉斯斯坦" to (41.2 to 74.7)),
        "LV" to ("拉脱维亚" to (56.8 to 24.6)), "LB" to ("黎巴嫩" to (33.8 to 35.8)),
        "LT" to ("立陶宛" to (55.1 to 23.8)), "LU" to ("卢森堡" to (49.8 to 6.1)),
        "MG" to ("马达加斯加" to (-18.7 to 46.8)), "MV" to ("马尔代夫" to (3.2 to 73.2)),
        "MT" to ("马耳他" to (35.9 to 14.3)), "MU" to ("毛里求斯" to (-20.3 to 57.5)),
        "MC" to ("摩纳哥" to (43.7 to 7.4)), "ME" to ("黑山" to (42.7 to 19.3)),
        "MZ" to ("莫桑比克" to (-18.6 to 35.5)), "NA" to ("纳米比亚" to (-22.9 to 18.4)),
        "OM" to ("阿曼" to (21.5 to 55.9)), "PK" to ("巴基斯坦" to (30.3 to 69.3)),
        "PA" to ("巴拿马" to (8.5 to -80.7)), "PY" to ("巴拉圭" to (-23.4 to -58.4)),
        "PH" to ("菲律宾" to (12.8 to 121.7)), "PL" to ("波兰" to (51.9 to 19.1)),
        "QA" to ("卡塔尔" to (25.3 to 51.1)), "RO" to ("罗马尼亚" to (45.9 to 24.9)),
        "RW" to ("卢旺达" to (-1.9 to 29.8)), "SN" to ("塞内加尔" to (14.4 to -14.4)),
        "RS" to ("塞尔维亚" to (44.0 to 21.0)), "SC" to ("塞舌尔" to (-4.6 to 55.4)),
        "SK" to ("斯洛伐克" to (48.6 to 19.6)), "SI" to ("斯洛文尼亚" to (46.1 to 14.9)),
        "TZ" to ("坦桑尼亚" to (-6.3 to 34.8)), "TN" to ("突尼斯" to (33.8 to 9.5)),
        "UA" to ("乌克兰" to (48.3 to 31.1)), "UY" to ("乌拉圭" to (-32.5 to -55.7)),
        "UZ" to ("乌兹别克斯坦" to (41.3 to 64.5)), "VA" to ("梵蒂冈" to (41.9 to 12.4)),
    )

    fun build240Countries(): List<CountryOfferingNode> {
        val list = ArrayList<CountryOfferingNode>(244)
        NAMED_COUNTRIES.forEach { c ->
            list += CountryOfferingNode(
                code = c.code,
                nameZh = c.zh,
                nameEn = c.en,
                region = c.region,
                latitude = c.lat,
                longitude = c.lon,
                offeringsCount = 0L,
                lampLitByMe = false,
            )
        }
        ISO_REGION_MATRIX.forEach { (code, meta) ->
            val (zh, coords) = meta
            list += CountryOfferingNode(
                code = code,
                nameZh = zh,
                nameEn = code,
                region = "全球十方",
                latitude = coords.first,
                longitude = coords.second,
                offeringsCount = 0L,
                lampLitByMe = false,
            )
        }
        val needed = 242 - list.size
        val goldenAngle = PI * (3.0 - sqrt(5.0))
        for (k in 0 until needed) {
            val y = 1.0 - (k.toDouble() / needed.coerceAtLeast(1)) * 1.75 - 0.1
            val lat = kotlin.math.asin(y.coerceIn(-0.92, 0.92)) * (180.0 / PI)
            val lon = (((k * goldenAngle * 180.0 / PI) + 360.0) % 360.0) - 180.0
            val code = "R%02d".format(k + 1)
            list += CountryOfferingNode(
                code = code,
                nameZh = "十方洲岛 #${k + 1}",
                nameEn = "Territory #${k + 1}",
                region = "环海诸岛",
                latitude = lat,
                longitude = lon,
                offeringsCount = 0L,
                lampLitByMe = false,
            )
        }
        return list
    }

    val DEFAULT_SUTRAS = listOf(
        SutraTextItem(
            id = "heart-sutra",
            title = "般若波罗蜜多心经",
            tradition = "玄奘法师译 · 260字核心般若",
            durationMinutes = 5,
            content = "观自在菩萨，行深般若波罗蜜多时，照见五蕴皆空，度一切苦厄。舍利子，色不异空，空不异色，色即是空，空即是色，受想行识，亦复如是。舍利子，是诸法空相，不生不灭，不垢不净，不增不减。是故空中无色，无受想行识，无眼耳鼻舌身意，无色声香味触法……揭谛揭谛，波罗揭谛，波罗僧揭谛，菩提萨婆诃。",
            dedicationVerse = "愿以此功德，庄严佛净土。上报四重恩，下济三途苦。",
        ),
        SutraTextItem(
            id = "diamond-verse",
            title = "金刚般若波罗蜜经 · 四句偈与应无所住",
            tradition = "鸠摩罗什译 · 金刚乘慧命",
            durationMinutes = 15,
            content = "诸菩萨摩诃萨应如是生清净心：不应住色生心，不应住声香味触法生心，应无所住而生其心。\n\n一切有为法，如梦幻泡影，如露亦如电，应作如是观。",
            dedicationVerse = "愿消三障诸烦恼，愿得智慧真明了，普愿罪障悉消除，世世常行菩萨道。",
        ),
        SutraTextItem(
            id = "samantabhadra",
            title = "大方广佛华严经 · 普贤菩萨十大愿王",
            tradition = "般若译 · 普贤行愿品",
            durationMinutes = 20,
            content = "一者礼敬诸佛，二者称赞如来，三者广修供养，四者忏悔业障，五者随喜功德，六者请转法轮，七者请佛住世，八者常随佛学，九者恒顺众生，十者普皆回向。\n\n诸供养中，法供养最！所谓如说修行供养、利益众生供养、摄受众生供养、代众生苦供养、勤修善根供养、不舍菩萨业供养、不离菩提心供养。",
            dedicationVerse = "文殊师利勇猛智，普贤慧行亦复然，我今回向诸善根，随彼一切常修学。",
        ),
        SutraTextItem(
            id = "medicine-buddha",
            title = "药师琉璃光如来十二大愿祈愿文",
            tradition = "消灾延寿 · 琉璃光明",
            durationMinutes = 10,
            content = "愿我来世得菩提时，身如琉璃，内外明彻，净无瑕秽，光明广大，功德巍巍，身善安住，焰网庄严过于日月；幽冥众生，悉蒙开晓，随意所趣，作诸事业。",
            dedicationVerse = "愿一切有情身心安泰、病苦消除、福慧增上、吉祥圆满。",
        ),
    )

    val DEFAULT_LEADERBOARD: List<MeritLeaderboardEntry> = emptyList()

    val DEFAULT_ACHIEVEMENTS: List<DharmaAchievement> = emptyList()
}

// ============================================================================
// 9. Hermes 6-Tab Studio, Magpie 47+ Gateway & Cloud Computer / Action Bus
// ============================================================================

enum class HermesStudioTab(val title: String, val badge: String) {
    SESSIONS_PROVIDERS("会话与模型", "MoA"),
    MEMORY_SOUL("记忆与灵魂", "Soul"),
    SKILLS_HUB("技能中心", "Skills"),
    TOOLSETS_TERMINAL("工具与终端", "TTY"),
    CRON_HEARTBEAT("定时与心跳", "Cron"),
    KANBAN_APPROVALS("看板与审批", "HITL"),
}

data class HermesSessionItem(
    val id: String,
    val title: String,
    val provider: String,
    val model: String,
    val parentBranchId: String? = null,
    val turnCount: Int = 2,
    val lastOutput: String = "",
)

data class HermesSkillItem(
    val id: String,
    val name: String,
    val category: String,
    val description: String,
    val enabled: Boolean = true,
)

data class HermesCronItem(
    val id: String,
    val title: String,
    val scheduleExpr: String,
    val prompt: String,
    val enabled: Boolean = true,
    val lastRunStatus: String = "成功 · 08:00",
)

enum class KanbanColumn(val label: String) {
    BACKLOG("待规划"),
    IN_PROGRESS("执行中"),
    REVIEW("待验收"),
    DONE("已完成"),
}

data class HermesKanbanCard(
    val id: String,
    val title: String,
    val assigneeAgent: String,
    val column: KanbanColumn,
)

data class HermesApprovalRequest(
    val id: String,
    val toolName: String,
    val summary: String,
    val riskTier: ActionRiskTier,
    val status: String = "pending", // pending | approved | denied
)

data class HermesStudioUiState(
    val activeTab: HermesStudioTab = HermesStudioTab.SESSIONS_PROVIDERS,
    val sessions: List<HermesSessionItem> = emptyList(),
    val activeSessionId: String = "hermes-main",
    val soulMarkdown: String = "# SOUL.md — Hermes Sovereign Agent\n- 慈悲、精准、实事求是，执行不可逆外部动作前必须请求确认。\n- 支持 `/moa` 混合多模型协同辩论与综合推理。",
    val memoryBlocks: List<Pair<String, String>> = listOf(
        "user_preferences" to "偏好中文技术架构回复，使用 Telegram 原生深浅色主题与 Mika 本机自动化。",
        "active_projects" to "fabu-android/fabushi-android v2.0.0 — GPUI-Telegram + Mika Phone Control Fusion。",
    ),
    val skills: List<HermesSkillItem> = emptyList(),
    val terminalLines: List<String> = listOf(
        "$ hermes status --all",
        "✓ Hermes Sovereign Engine v1.2 · 61 slash commands loaded",
        "✓ Toolsets: [mcp-core, telegram-bot, mika-phone-bridge, ton-wallet, sandbox-exec]",
    ),
    val cronJobs: List<HermesCronItem> = emptyList(),
    val kanbanCards: List<HermesKanbanCard> = emptyList(),
    val approvals: List<HermesApprovalRequest> = emptyList(),
)

enum class MagpieRoutingStrategy(val wire: String, val label: String, val subtitle: String) {
    ORDER("order", "Order 优先级", "按主备顺序自动故障转移"),
    ROTATE("rotate", "Rotate 轮询", "多 Key 等权轮转分摊并发"),
    CHEAPEST("cheapest", "Cheapest 最低成本", "实时选择百万 Token 最低价通道"),
    DRAIN("drain", "Drain 额度优先", "优先消耗即将到期的免费/赠金配额"),
    USAGE("usage", "Usage 负载均衡", "按各供应商实时窗口用量动态配平"),
    PACE("pace", "Pace 速率平滑", "RPM/TPM 令牌桶平滑防 429 限流"),
}

data class MagpieProviderPreset(
    val id: String,
    val name: String,
    val region: String, // "Global" | "CN" | "Cloud" | "Local"
    val protocol: String, // "OpenAI" | "Anthropic" | "Gemini" | "Responses"
    val defaultBaseUrl: String,
    val flagshipModel: String,
    val inputUsdPer1M: Double,
    val configured: Boolean = false,
    val healthy: Boolean = true,
)

object MagpiePresetCatalog {
    val PRESETS_48: List<MagpieProviderPreset> = listOf(
        MagpieProviderPreset("openai", "OpenAI Official", "Global", "Responses", "https://api.openai.com/v1", "gpt-5.4", 2.50, configured = true),
        MagpieProviderPreset("anthropic", "Anthropic Claude", "Global", "Anthropic", "https://api.anthropic.com/v1", "claude-4.6-sonnet", 3.00, configured = true),
        MagpieProviderPreset("gemini", "Google Gemini", "Global", "Gemini", "https://generativelanguage.googleapis.com/v1beta", "gemini-3.1-pro", 1.25, configured = true),
        MagpieProviderPreset("xai", "xAI Grok", "Global", "OpenAI", "https://api.x.ai/v1", "grok-4", 2.00, configured = true),
        MagpieProviderPreset("deepseek", "DeepSeek 深度求索", "CN", "OpenAI", "https://api.deepseek.com/v1", "deepseek-v3.2", 0.27, configured = true),
        MagpieProviderPreset("qwen", "Alibaba Qwen 通义千问", "CN", "OpenAI", "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen3-max", 0.40, configured = true),
        MagpieProviderPreset("kimi", "Moonshot Kimi 月之暗面", "CN", "OpenAI", "https://api.moonshot.cn/v1", "kimi-k2.5", 0.60),
        MagpieProviderPreset("zhipu", "Zhipu GLM 智谱清言", "CN", "OpenAI", "https://open.bigmodel.cn/api/paas/v4", "glm-5", 0.50),
        MagpieProviderPreset("minimax", "MiniMax 海螺", "CN", "OpenAI", "https://api.minimax.chat/v1", "MiniMax-M2", 0.45),
        MagpieProviderPreset("doubao", "Volcengine 豆包大模型", "CN", "OpenAI", "https://ark.cn-beijing.volces.com/api/v3", "doubao-seed-2.0-pro", 0.22),
        MagpieProviderPreset("siliconflow", "SiliconFlow 硅基流动", "CN", "OpenAI", "https://api.siliconflow.cn/v1", "Pro/deepseek-ai/DeepSeek-R1", 0.28),
        MagpieProviderPreset("yi", "01.AI 零一万物", "CN", "OpenAI", "https://api.lingyiwanwu.com/v1", "yi-lightning", 0.19),
        MagpieProviderPreset("stepfun", "StepFun 阶跃星辰", "CN", "OpenAI", "https://api.stepfun.com/v1", "step-2-16k", 0.50),
        MagpieProviderPreset("baichuan", "Baichuan 百川智能", "CN", "OpenAI", "https://api.baichuan-ai.com/v1", "Baichuan4-Turbo", 0.55),
        MagpieProviderPreset("hunyuan", "Tencent 腾讯混元", "CN", "OpenAI", "https://api.hunyuan.cloud.tencent.com/v1", "hunyuan-turboS", 0.35),
        MagpieProviderPreset("ernie", "Baidu 百度文心千帆", "CN", "OpenAI", "https://qianfan.baidubce.com/v2", "ernie-4.5-8k", 0.40),
        MagpieProviderPreset("spark", "iFlytek 讯飞星火", "CN", "OpenAI", "https://spark-api-open.xf-yun.com/v1", "4.0Ultra", 0.42),
        MagpieProviderPreset("openrouter", "OpenRouter Unified", "Global", "OpenAI", "https://openrouter.ai/api/v1", "openrouter/auto", 1.00, configured = true),
        MagpieProviderPreset("groq", "Groq LPU", "Global", "OpenAI", "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", 0.59),
        MagpieProviderPreset("cerebras", "Cerebras Wafer-Scale", "Global", "OpenAI", "https://api.cerebras.ai/v1", "llama-4-scout-17b", 0.20),
        MagpieProviderPreset("sambanova", "SambaNova Cloud", "Global", "OpenAI", "https://api.sambanova.ai/v1", "DeepSeek-R1", 0.50),
        MagpieProviderPreset("together", "Together AI", "Global", "OpenAI", "https://api.together.xyz/v1", "Qwen/Qwen3-235B", 0.80),
        MagpieProviderPreset("fireworks", "Fireworks AI", "Global", "OpenAI", "https://api.fireworks.ai/inference/v1", "accounts/fireworks/models/deepseek-v3", 0.90),
        MagpieProviderPreset("mistral", "Mistral La Plateforme", "Global", "OpenAI", "https://api.mistral.ai/v1", "mistral-large-latest", 2.00),
        MagpieProviderPreset("cohere", "Cohere Command", "Global", "OpenAI", "https://api.cohere.ai/compatibility/v1", "command-a-03-2025", 2.50),
        MagpieProviderPreset("perplexity", "Perplexity Sonar", "Global", "OpenAI", "https://api.perplexity.ai", "sonar-reasoning-pro", 2.00),
        MagpieProviderPreset("hyperbolic", "Hyperbolic Labs", "Global", "OpenAI", "https://api.hyperbolic.xyz/v1", "DeepSeek-V3", 0.25),
        MagpieProviderPreset("novita", "Novita AI", "Global", "OpenAI", "https://api.novita.ai/v3/openai", "deepseek/deepseek-r1", 0.55),
        MagpieProviderPreset("deepinfra", "DeepInfra", "Global", "OpenAI", "https://api.deepinfra.com/v1/openai", "meta-llama/Llama-4-Maverick", 0.30),
        MagpieProviderPreset("chutes", "Chutes Serverless", "Global", "OpenAI", "https://llm.chutes.ai/v1", "deepseek-ai/DeepSeek-V3", 0.20),
        MagpieProviderPreset("featherless", "Featherless AI", "Global", "OpenAI", "https://api.featherless.ai/v1", "Qwen/Qwen2.5-72B-Instruct", 0.35),
        MagpieProviderPreset("nebius", "Nebius AI Studio", "Global", "OpenAI", "https://api.studio.nebius.ai/v1", "Qwen/Qwen3-32B", 0.25),
        MagpieProviderPreset("kluster", "Kluster.ai", "Global", "OpenAI", "https://api.kluster.ai/v1", "klusterai/Meta-Llama-3.3-70B", 0.30),
        MagpieProviderPreset("upstage", "Upstage Solar", "Global", "OpenAI", "https://api.upstage.ai/v1/solar", "solar-pro", 0.60),
        MagpieProviderPreset("ai21", "AI21 Jamba", "Global", "OpenAI", "https://api.ai21.com/studio/v1", "jamba-1.6-large", 2.00),
        MagpieProviderPreset("bedrock", "AWS Bedrock Mantle", "Cloud", "Anthropic", "https://bedrock-runtime.us-east-1.amazonaws.com", "anthropic.claude-sonnet-4", 3.00),
        MagpieProviderPreset("azure", "Azure OpenAI Foundry", "Cloud", "OpenAI", "https://your-resource.openai.azure.com/openai/deployments", "gpt-4.1", 2.00),
        MagpieProviderPreset("vertex", "Google Cloud Vertex AI", "Cloud", "Gemini", "https://aiplatform.googleapis.com/v1", "gemini-2.5-pro", 1.25),
        MagpieProviderPreset("cloudflare", "Cloudflare Workers AI", "Cloud", "OpenAI", "https://api.cloudflare.com/client/v4/accounts/cf/ai/v1", "@cf/meta/llama-3.3-70b", 0.29),
        MagpieProviderPreset("github", "GitHub Models", "Cloud", "OpenAI", "https://models.inference.ai.azure.com", "gpt-4.1-mini", 0.00),
        MagpieProviderPreset("huggingface", "HuggingFace Inference", "Cloud", "OpenAI", "https://router.huggingface.co/v1", "Qwen/Qwen3-Coder", 0.35),
        MagpieProviderPreset("nvidia", "NVIDIA NIM", "Cloud", "OpenAI", "https://integrate.api.nvidia.com/v1", "nvidia/llama-3.3-nemotron-super-49b", 0.40),
        MagpieProviderPreset("replicate", "Replicate", "Cloud", "OpenAI", "https://api.replicate.com/v1", "meta/meta-llama-3-70b-instruct", 0.65),
        MagpieProviderPreset("baseten", "Baseten Model APIs", "Cloud", "OpenAI", "https://inference.baseten.co/v1", "deepseek-v3", 0.45),
        MagpieProviderPreset("friendli", "FriendliAI", "Cloud", "OpenAI", "https://api.friendli.ai/serverless/v1", "meta-llama-3.3-70b-instruct", 0.50),
        MagpieProviderPreset("ollama", "Ollama Local / LAN", "Local", "OpenAI", "http://127.0.0.1:11434/v1", "qwen3:8b", 0.00),
        MagpieProviderPreset("vllm", "vLLM Self-Hosted", "Local", "OpenAI", "http://127.0.0.1:8000/v1", "deepseek-r1-distill-32b", 0.00),
        MagpieProviderPreset("lmstudio", "LM Studio Local", "Local", "OpenAI", "http://127.0.0.1:1234/v1", "gemma-3-27b-it", 0.00),
    )

    private val skKeyRegex = Regex("""\b(sk-[A-Za-z0-9_-]{12,}|AIza[0-9A-Za-z_-]{20,}|ghp_[A-Za-z0-9]{20,})\b""")
    private val emailRegex = Regex("""\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b""")
    private val phoneRegex = Regex("""(?<!\d)(\+?\d{1,3}[-\s]?)?1[3-9]\d{9}(?!\d)""")

    fun maskSecretsAndPii(input: String): Pair<String, Int> {
        var count = 0
        var masked = skKeyRegex.replace(input) { count++; "[REDACTED_API_KEY]" }
        masked = emailRegex.replace(masked) { count++; "[REDACTED_EMAIL]" }
        masked = phoneRegex.replace(masked) { count++; "[REDACTED_PHONE]" }
        return masked to count
    }
}

data class MagpieGatewayUiState(
    val strategy: MagpieRoutingStrategy = MagpieRoutingStrategy.CHEAPEST,
    val presets: List<MagpieProviderPreset> = MagpiePresetCatalog.PRESETS_48,
    val totalRequests: Long = 0L,
    val totalPromptTokens: Long = 0L,
    val totalCompletionTokens: Long = 0L,
    val totalCostUsd: Double = 0.0,
    val piiRedactedCount: Int = 0,
    val lastRouteExplanation: String = "尚未发起请求 · 请先配置 API Key 或同步本地订阅",
    val lastMaskedPreview: String = "",
    val discoveredModels: List<String> = emptyList(),
    val modelsDevStatusLabel: String = "未同步 · 点击拉取 models.dev 实时目录与上游 /models",
    val localSubscriptionsSummary: String = "未扫描 · 支持 Codex / Vertex ADC / Gemini / Claude 本地订阅",
)

enum class CodingProviderKindUi(val wire: String, val label: String, val recommendedHandle: String, val supportsAcp: Boolean) {
    CLAUDE_CODE("claude_code", "Claude Code", "@claude_code_bot", false),
    CODEX("codex", "Codex", "@codex_bot", false),
    OPEN_CODE("open_code", "OpenCode", "@opencode_bot", true),
    CURSOR_CLI("cursor_cli", "Cursor CLI", "@cursor_cli_bot", true),
    ANTIGRAVITY("antigravity", "Antigravity", "@antigravity_bot", false),
    ACP_CUSTOM("acp_custom", "Custom ACP", "@acp_coding_bot", true),
}

enum class CodingPermissionModeUi(val wire: String, val label: String) {
    ASK("ask", "每次询问"),
    AUTO_EDIT("auto_edit", "自动编辑"),
    FULL_ACCESS("full_access", "完全访问"),
}

data class CodingAgentBotItem(
    val botId: String,
    val displayName: String,
    val username: String,
    val provider: CodingProviderKindUi,
    val permissionMode: CodingPermissionModeUi,
    val workspacePath: String,
    val useWorktree: Boolean = true,
    val boundNodeId: String? = null,
    val activeBranch: String = "main",
    val totalTurns: Int = 0,
)

data class RemoteAssistConfigUi(
    val assistId: String = "",
    val allowAssist: Boolean = false,
    val codeMode: String = "temporary",
    val currentCode: String = "",
)

data class RemoteStreamSettingsUi(
    val codec: String = "auto",
    val targetFps: Int = 60,
    val quality: String = "auto",
    val colorMode: String = "yuv420",
    val hdr: Boolean = false,
    val audioEnabled: Boolean = true,
    val micPassthrough: Boolean = false,
    val privacyScreen: Boolean = false,
    val displayIndex: Int = 0,
)

data class RemoteMcpGrantItem(
    val familyId: String,
    val clientName: String,
    val scopes: List<String>,
    val allowedNodeIds: List<String>,
)

data class UserVaultUiState(
    val initialized: Boolean = false,
    val kdfAlg: String = "split_key_v2",
    val keyVersion: Int = 2,
    val encryptedItemCount: Int = 0,
    val activeSyncRequestId: String? = null,
    val activeSyncVerificationCode: String? = null,
    val activeSyncStatus: String = "未发起跨设备同步",
    val lastVaultMessage: String = "Split-Key V2 (Argon2id + HKDF) · ChaCha20-Poly1305 按用户隔离加密",
)

data class MiniAppPipSessionUi(
    val active: Boolean = false,
    val minimized: Boolean = false,
    val botUsername: String = "fabushi_bot",
    val shortName: String = "global_dharma",
    val appTitle: String = "全球法布施",
    val controlMode: String = "idle",
    val statusText: String = "",
    val activeTool: String? = null,
)

object TelegramLocalTimeFormatter {
    fun formatLocalTime(timestampMs: Long, timeZone: java.util.TimeZone = java.util.TimeZone.getDefault()): String {
        val fmt = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
        fmt.timeZone = timeZone
        return fmt.format(java.util.Date(timestampMs))
    }

    fun formatLocalDateHeader(timestampMs: Long, timeZone: java.util.TimeZone = java.util.TimeZone.getDefault()): String {
        val fmt = java.text.SimpleDateFormat("yyyy-MM-dd", java.util.Locale.getDefault())
        fmt.timeZone = timeZone
        return fmt.format(java.util.Date(timestampMs))
    }
}

enum class ComputerNodeKind(val label: String) {
    LOCAL_DEVICE("本机 Android · Mika"),
    CLOUD_SERVER("远程云服务器 GPU"),
    CLOUD_SANDBOX("云端安全沙箱"),
}

data class ComputerNodeItem(
    val id: String,
    val name: String,
    val kind: ComputerNodeKind,
    val osInfo: String,
    val online: Boolean = true,
    val cpuLoadPercent: Int = 18,
    val memorySummary: String = "6.2 / 12.0 GB",
    val leaseOwner: String? = null,
    val humanTakeoverActive: Boolean = false,
)

enum class ActionRiskTier(val code: String, val label: String, val requiresApproval: Boolean) {
    R0_READ("R0", "只读观测 (R0)", false),
    R1_LOW("R1", "低风险可逆 (R1)", false),
    R2_MEDIUM("R2", "中风险状态写入 (R2)", false),
    R3_HIGH("R3", "高风险外部通信/文件 (R3)", true),
    R4_CRITICAL("R4", "极高危支付/权限/删除 (R4)", true),
}

data class ActionBusSchemaItem(
    val name: String,
    val domain: String,
    val riskTier: ActionRiskTier,
    val description: String,
    val schemaPreview: String,
)

enum class GlobalTaskFsmState(val label: String) {
    QUEUED("Queued 排队"),
    PLANNING("Planning 规划"),
    AWAITING_APPROVAL("AwaitingApproval 待确认"),
    RUNNING("Running 执行中"),
    PAUSED("Paused 已暂停"),
    COMPLETED("Completed 已完成"),
    FAILED("Failed 失败"),
}

data class GlobalTaskItem(
    val id: String,
    val title: String,
    val targetNodeId: String,
    val state: GlobalTaskFsmState,
    val progressPercent: Int,
    val detail: String,
)

data class CloudComputerUiState(
    val nodes: List<ComputerNodeItem> = emptyList(),
    val selectedNodeId: String = "node-local-android",
    val actionBusTools: List<ActionBusSchemaItem> = emptyList(),
    val globalTasks: List<GlobalTaskItem> = emptyList(),
    val lastOpOutput: String = "就绪 · 选择节点并获取 CUA 租约即可安全操控本机或云端计算节点。",
    val assistConfig: RemoteAssistConfigUi = RemoteAssistConfigUi(),
    val streamSettings: RemoteStreamSettingsUi = RemoteStreamSettingsUi(),
    val mcpGrants: List<RemoteMcpGrantItem> = emptyList(),
    val actionBusCategoryCount: Int = 19,
)

// ============================================================================
// 10. Mika Phone Agent Bridge Callbacks (Wave 2 Plug-in Contract)
// ============================================================================

data class MikaTaskSummaryItem(
    val taskId: String,
    val prompt: String,
    val status: String, // "running" | "waiting_confirmation" | "completed" | "failed"
    val latestStep: String,
    val updatedAtLabel: String,
)

data class MikaBridgeUiSnapshot(
    val available: Boolean = true,
    val accessibilityEnabled: Boolean = false,
    val wirelessAdbPaired: Boolean = false,
    val helperBinderAlive: Boolean = false,
    val virtualDisplayActive: Boolean = false,
    val capsuleVisible: Boolean = false,
    val sandboxReady: Boolean = true,
    val activeSpeedProfile: String = "标准 (Balanced)",
    val activeTask: MikaTaskSummaryItem? = null,
    val pendingConfirmationPrompt: String? = null,
    val recentTasks: List<MikaTaskSummaryItem> = emptyList(),
)

interface MikaBridgeCallbacks {
    fun currentSnapshot(): MikaBridgeUiSnapshot = MikaBridgeUiSnapshot()
    fun submitPhoneTask(prompt: String, sourceConversationId: String? = null, onStepUpdate: (String) -> Unit = {}) {}
    fun stopActiveTask() {}
    fun resolveConfirmation(approved: Boolean) {}
    fun toggleFloatingCapsule(enabled: Boolean) {}
    fun openAccessibilitySettings() {}
    fun openWirelessAdbPairing() {}
    fun openLinuxTerminal() {}
}
