package com.ombhrum.fabushi.telegram

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Pure-Kotlin ISO/IEC 18004 Byte-mode QR Code generator (Versions 1..14, EC Level L/M)
 * with GF(256) Reed-Solomon error correction, finder/separator/alignment/timing/dark-module
 * placement, BCH format & version information, and 8-mask penalty evaluation.
 */
internal data class QrMatrix(
    val version: Int,
    val size: Int,
    val modules: BooleanArray,
) {
    fun get(x: Int, y: Int): Boolean = modules[y * size + x]

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is QrMatrix) return false
        return version == other.version && size == other.size && modules.contentEquals(other.modules)
    }

    override fun hashCode(): Int = 31 * (31 * version + size) + modules.contentHashCode()
}

internal enum class QrErrorCorrectionLevel(val formatBits: Int) {
    LOW(1),    // 01
    MEDIUM(0), // 00
}

internal object QrCodeEncoder {
    private data class VersionSpec(
        val version: Int,
        val totalCodewords: Int,
        val ecCodewordsPerBlock: Int,
        val group1Blocks: Int,
        val group1DataCodewords: Int,
        val group2Blocks: Int = 0,
        val group2DataCodewords: Int = 0,
    ) {
        val totalDataCodewords: Int
            get() = group1Blocks * group1DataCodewords + group2Blocks * group2DataCodewords
    }

    // ISO/IEC 18004 Table 9 — EC Level L (Versions 1..14)
    private val SPECS_L = arrayOf(
        VersionSpec(1, 26, 7, 1, 19),
        VersionSpec(2, 44, 10, 1, 34),
        VersionSpec(3, 70, 15, 1, 55),
        VersionSpec(4, 100, 20, 1, 80),
        VersionSpec(5, 134, 26, 1, 108),
        VersionSpec(6, 172, 18, 2, 68),
        VersionSpec(7, 196, 20, 2, 78),
        VersionSpec(8, 242, 24, 2, 97),
        VersionSpec(9, 292, 30, 2, 116),
        VersionSpec(10, 346, 18, 2, 68, 2, 69),
        VersionSpec(11, 404, 20, 4, 81),
        VersionSpec(12, 466, 24, 2, 92, 2, 93),
        VersionSpec(13, 532, 26, 4, 107),
        VersionSpec(14, 581, 30, 3, 115, 1, 116),
    )

    // ISO/IEC 18004 Table 9 — EC Level M (Versions 1..14)
    private val SPECS_M = arrayOf(
        VersionSpec(1, 26, 10, 1, 16),
        VersionSpec(2, 44, 16, 1, 28),
        VersionSpec(3, 70, 26, 1, 44),
        VersionSpec(4, 100, 18, 2, 32),
        VersionSpec(5, 134, 24, 2, 43),
        VersionSpec(6, 172, 16, 4, 27),
        VersionSpec(7, 196, 18, 4, 31),
        VersionSpec(8, 242, 22, 2, 38, 2, 39),
        VersionSpec(9, 292, 22, 3, 36, 2, 37),
        VersionSpec(10, 346, 26, 4, 43, 1, 44),
        VersionSpec(11, 404, 30, 1, 50, 4, 51),
        VersionSpec(12, 466, 22, 6, 36, 2, 37),
        VersionSpec(13, 532, 22, 8, 37, 1, 38),
        VersionSpec(14, 581, 24, 4, 40, 5, 41),
    )

    // ISO/IEC 18004 Table E.1 — Alignment pattern center coordinates
    private val ALIGNMENT_CENTERS = arrayOf(
        intArrayOf(),            // V1
        intArrayOf(6, 18),       // V2
        intArrayOf(6, 22),       // V3
        intArrayOf(6, 26),       // V4
        intArrayOf(6, 30),       // V5
        intArrayOf(6, 34),       // V6
        intArrayOf(6, 22, 38),   // V7
        intArrayOf(6, 24, 42),   // V8
        intArrayOf(6, 26, 46),   // V9
        intArrayOf(6, 28, 50),   // V10
        intArrayOf(6, 30, 54),   // V11
        intArrayOf(6, 32, 58),   // V12
        intArrayOf(6, 34, 62),   // V13
        intArrayOf(6, 26, 46, 66), // V14
    )

    private val GF_EXP = IntArray(512)
    private val GF_LOG = IntArray(256)

    init {
        var x = 1
        for (i in 0 until 255) {
            GF_EXP[i] = x
            GF_LOG[x] = i
            x = x shl 1
            if ((x and 0x100) != 0) {
                x = x xor 0x11D
            }
        }
        for (i in 255 until 512) {
            GF_EXP[i] = GF_EXP[i - 255]
        }
    }

    private fun gfMul(a: Int, b: Int): Int {
        if (a == 0 || b == 0) return 0
        return GF_EXP[GF_LOG[a] + GF_LOG[b]]
    }

    private fun rsGeneratorPoly(degree: Int): IntArray {
        var poly = intArrayOf(1)
        for (i in 0 until degree) {
            val next = IntArray(poly.size + 1)
            val root = GF_EXP[i]
            for (j in poly.indices) {
                next[j] = next[j] xor poly[j]
                next[j + 1] = next[j + 1] xor gfMul(poly[j], root)
            }
            poly = next
        }
        return poly
    }

    private fun rsComputeRemainder(data: ByteArray, ecCount: Int): ByteArray {
        val gen = rsGeneratorPoly(ecCount)
        val result = IntArray(ecCount)
        for (b in data) {
            val factor = (b.toInt() and 0xFF) xor result[0]
            for (i in 0 until ecCount - 1) {
                result[i] = result[i + 1]
            }
            result[ecCount - 1] = 0
            for (i in 0 until ecCount) {
                result[i] = result[i] xor gfMul(gen[i + 1], factor)
            }
        }
        return ByteArray(ecCount) { result[it].toByte() }
    }

    fun encode(
        content: String,
        ecLevel: QrErrorCorrectionLevel = QrErrorCorrectionLevel.MEDIUM,
    ): QrMatrix {
        val rawBytes = content.toByteArray(Charsets.UTF_8)
        require(rawBytes.isNotEmpty()) { "QR payload must not be empty" }

        val (spec, effectiveEcLevel) = selectVersionSpec(rawBytes.size, ecLevel)
        val dataCodewords = encodeDataCodewords(rawBytes, spec)
        val interleaved = interleaveWithEc(dataCodewords, spec)

        val size = spec.version * 4 + 17
        val baseModules = BooleanArray(size * size)
        val isFunction = BooleanArray(size * size)

        drawFunctionPatterns(spec.version, size, baseModules, isFunction)
        placeCodewords(size, interleaved, baseModules, isFunction)

        var bestMask = 0
        var bestPenalty = Int.MAX_VALUE
        var bestModules = baseModules
        for (mask in 0..7) {
            val candidate = baseModules.copyOf()
            applyMask(size, mask, candidate, isFunction)
            drawFormatInfo(size, effectiveEcLevel, mask, candidate)
            if (spec.version >= 7) {
                drawVersionInfo(spec.version, size, candidate)
            }
            val penalty = computePenalty(size, candidate)
            if (penalty < bestPenalty) {
                bestPenalty = penalty
                bestMask = mask
                bestModules = candidate
            }
        }

        // Ensure format info matches winning mask cleanly
        drawFormatInfo(size, effectiveEcLevel, bestMask, bestModules)
        if (spec.version >= 7) {
            drawVersionInfo(spec.version, size, bestModules)
        }
        return QrMatrix(version = spec.version, size = size, modules = bestModules)
    }

    private fun selectVersionSpec(byteLen: Int, preferred: QrErrorCorrectionLevel): Pair<VersionSpec, QrErrorCorrectionLevel> {
        val preferredTable = if (preferred == QrErrorCorrectionLevel.MEDIUM) SPECS_M else SPECS_L
        for (spec in preferredTable) {
            val countBits = if (spec.version <= 9) 8 else 16
            val neededBits = 4 + countBits + byteLen * 8
            if (neededBits <= spec.totalDataCodewords * 8) {
                return spec to preferred
            }
        }
        if (preferred != QrErrorCorrectionLevel.LOW) {
            for (spec in SPECS_L) {
                val countBits = if (spec.version <= 9) 8 else 16
                val neededBits = 4 + countBits + byteLen * 8
                if (neededBits <= spec.totalDataCodewords * 8) {
                    return spec to QrErrorCorrectionLevel.LOW
                }
            }
        }
        throw IllegalArgumentException("QR payload length $byteLen bytes exceeds max supported Version 14 capacity")
    }

    private fun encodeDataCodewords(bytes: ByteArray, spec: VersionSpec): ByteArray {
        val capacityBits = spec.totalDataCodewords * 8
        val bits = ArrayList<Boolean>(capacityBits)

        fun appendBits(value: Int, count: Int) {
            for (i in count - 1 downTo 0) {
                bits.add(((value ushr i) and 1) != 0)
            }
        }

        // Byte mode indicator = 0100
        appendBits(0b0100, 4)
        val countBits = if (spec.version <= 9) 8 else 16
        appendBits(bytes.size, countBits)
        for (b in bytes) {
            appendBits(b.toInt() and 0xFF, 8)
        }

        // Terminator (up to 4 zero bits)
        val terminatorLen = minOf(4, capacityBits - bits.size)
        repeat(terminatorLen) { bits.add(false) }

        // Pad to byte boundary
        while (bits.size % 8 != 0) {
            bits.add(false)
        }

        val out = ByteArray(spec.totalDataCodewords)
        var byteIdx = 0
        var i = 0
        while (i < bits.size) {
            var v = 0
            for (b in 0 until 8) {
                v = (v shl 1) or if (bits[i + b]) 1 else 0
            }
            out[byteIdx++] = v.toByte()
            i += 8
        }

        var padToggle = true
        while (byteIdx < out.size) {
            out[byteIdx++] = (if (padToggle) 0xEC else 0x11).toByte()
            padToggle = !padToggle
        }
        return out
    }

    private fun interleaveWithEc(data: ByteArray, spec: VersionSpec): ByteArray {
        val totalBlocks = spec.group1Blocks + spec.group2Blocks
        val dataBlocks = ArrayList<ByteArray>(totalBlocks)
        val ecBlocks = ArrayList<ByteArray>(totalBlocks)

        var offset = 0
        repeat(spec.group1Blocks) {
            val block = data.copyOfRange(offset, offset + spec.group1DataCodewords)
            offset += spec.group1DataCodewords
            dataBlocks += block
            ecBlocks += rsComputeRemainder(block, spec.ecCodewordsPerBlock)
        }
        repeat(spec.group2Blocks) {
            val block = data.copyOfRange(offset, offset + spec.group2DataCodewords)
            offset += spec.group2DataCodewords
            dataBlocks += block
            ecBlocks += rsComputeRemainder(block, spec.ecCodewordsPerBlock)
        }

        val result = ByteArray(spec.totalCodewords)
        var idx = 0
        val maxDataLen = maxOf(spec.group1DataCodewords, spec.group2DataCodewords)
        for (col in 0 until maxDataLen) {
            for (block in dataBlocks) {
                if (col < block.size) {
                    result[idx++] = block[col]
                }
            }
        }
        for (col in 0 until spec.ecCodewordsPerBlock) {
            for (block in ecBlocks) {
                result[idx++] = block[col]
            }
        }
        return result
    }

    private fun drawFunctionPatterns(
        version: Int,
        size: Int,
        modules: BooleanArray,
        isFunction: BooleanArray,
    ) {
        fun setFn(x: Int, y: Int, dark: Boolean) {
            if (x in 0 until size && y in 0 until size) {
                modules[y * size + x] = dark
                isFunction[y * size + x] = true
            }
        }

        // Timing patterns
        for (i in 0 until size) {
            setFn(6, i, i % 2 == 0)
            setFn(i, 6, i % 2 == 0)
        }

        // Finder patterns + 1-module white separators
        fun drawFinder(ox: Int, oy: Int) {
            for (dy in -1..7) {
                for (dx in -1..7) {
                    val x = ox + dx
                    val y = oy + dy
                    if (x !in 0 until size || y !in 0 until size) continue
                    val dark = if (dx in 0..6 && dy in 0..6) {
                        val dist = maxOf(kotlin.math.abs(dx - 3), kotlin.math.abs(dy - 3))
                        dist != 2
                    } else {
                        false
                    }
                    setFn(x, y, dark)
                }
            }
        }
        drawFinder(0, 0)
        drawFinder(size - 7, 0)
        drawFinder(0, size - 7)

        // Alignment patterns
        val centers = ALIGNMENT_CENTERS[version - 1]
        for (cy in centers) {
            for (cx in centers) {
                // Skip overlap with the three finder patterns
                if ((cx == 6 && cy == 6) ||
                    (cx == 6 && cy == size - 7) ||
                    (cx == size - 7 && cy == 6)
                ) continue
                for (dy in -2..2) {
                    for (dx in -2..2) {
                        val dist = maxOf(kotlin.math.abs(dx), kotlin.math.abs(dy))
                        setFn(cx + dx, cy + dy, dist != 1)
                    }
                }
            }
        }

        // Reserve format info modules
        for (i in 0..8) {
            if (i != 6) {
                setFn(8, i, false)
                setFn(i, 8, false)
            }
        }
        for (i in 0 until 8) {
            setFn(size - 1 - i, 8, false)
        }
        for (i in 0 until 7) {
            setFn(8, size - 1 - i, false)
        }

        // Dark module (always dark at x=8, y=4*v+9)
        setFn(8, 4 * version + 9, true)

        // Reserve version info modules for Version >= 7
        if (version >= 7) {
            for (i in 0 until 6) {
                for (j in 0 until 3) {
                    setFn(size - 11 + j, i, false)
                    setFn(i, size - 11 + j, false)
                }
            }
        }
    }

    private fun placeCodewords(
        size: Int,
        codewords: ByteArray,
        modules: BooleanArray,
        isFunction: BooleanArray,
    ) {
        var bitIndex = 0
        val totalBits = codewords.size * 8
        var upward = true
        var right = size - 1
        while (right >= 1) {
            if (right == 6) right-- // Skip vertical timing column
            for (vert in 0 until size) {
                val y = if (upward) size - 1 - vert else vert
                for (dx in 0..1) {
                    val x = right - dx
                    val pos = y * size + x
                    if (!isFunction[pos]) {
                        val dark = if (bitIndex < totalBits) {
                            val b = codewords[bitIndex ushr 3].toInt() and 0xFF
                            ((b ushr (7 - (bitIndex and 7))) and 1) != 0
                        } else {
                            false
                        }
                        modules[pos] = dark
                        bitIndex++
                    }
                }
            }
            upward = !upward
            right -= 2
        }
    }

    private fun applyMask(
        size: Int,
        mask: Int,
        modules: BooleanArray,
        isFunction: BooleanArray,
    ) {
        for (y in 0 until size) {
            for (x in 0 until size) {
                val pos = y * size + x
                if (isFunction[pos]) continue
                val invert = when (mask) {
                    0 -> (x + y) % 2 == 0
                    1 -> y % 2 == 0
                    2 -> x % 3 == 0
                    3 -> (x + y) % 3 == 0
                    4 -> ((y / 2) + (x / 3)) % 2 == 0
                    5 -> ((x * y) % 2) + ((x * y) % 3) == 0
                    6 -> (((x * y) % 2) + ((x * y) % 3)) % 2 == 0
                    7 -> (((x + y) % 2) + ((x * y) % 3)) % 2 == 0
                    else -> false
                }
                if (invert) {
                    modules[pos] = !modules[pos]
                }
            }
        }
    }

    private fun computeFormatBits(ecLevel: QrErrorCorrectionLevel, mask: Int): Int {
        val data = (ecLevel.formatBits shl 3) or mask
        var rem = data
        repeat(10) {
            rem = (rem shl 1) xor ((rem ushr 9) * 0x537)
        }
        return ((data shl 10) or rem) xor 0x5412
    }

    private fun drawFormatInfo(
        size: Int,
        ecLevel: QrErrorCorrectionLevel,
        mask: Int,
        modules: BooleanArray,
    ) {
        val bits = computeFormatBits(ecLevel, mask)
        fun bit(i: Int): Boolean = ((bits ushr i) and 1) != 0

        // First copy around top-left finder
        for (i in 0..5) modules[i * size + 8] = bit(i)
        modules[7 * size + 8] = bit(6)
        modules[8 * size + 8] = bit(7)
        modules[8 * size + 7] = bit(8)
        for (i in 9..14) modules[8 * size + (14 - i)] = bit(i)

        // Second copy along top-right & bottom-left finders
        for (i in 0..7) modules[8 * size + (size - 1 - i)] = bit(i)
        for (i in 8..14) modules[(size - 15 + i) * size + 8] = bit(i)
        // Dark module
        modules[(size - 8) * size + 8] = true
    }

    private fun drawVersionInfo(version: Int, size: Int, modules: BooleanArray) {
        var rem = version
        repeat(12) {
            rem = (rem shl 1) xor ((rem ushr 11) * 0x1F25)
        }
        val bits = (version shl 12) or rem
        for (i in 0 until 18) {
            val dark = ((bits ushr i) and 1) != 0
            val a = size - 11 + (i % 3)
            val b = i / 3
            modules[b * size + a] = dark
            modules[a * size + b] = dark
        }
    }

    private fun computePenalty(size: Int, modules: BooleanArray): Int {
        var penalty = 0

        // Rule 1: Runs of >= 5 same-colored modules in rows and columns
        for (y in 0 until size) {
            var runColor = modules[y * size]
            var runLen = 1
            for (x in 1 until size) {
                val c = modules[y * size + x]
                if (c == runColor) {
                    runLen++
                } else {
                    if (runLen >= 5) penalty += 3 + (runLen - 5)
                    runColor = c
                    runLen = 1
                }
            }
            if (runLen >= 5) penalty += 3 + (runLen - 5)
        }
        for (x in 0 until size) {
            var runColor = modules[x]
            var runLen = 1
            for (y in 1 until size) {
                val c = modules[y * size + x]
                if (c == runColor) {
                    runLen++
                } else {
                    if (runLen >= 5) penalty += 3 + (runLen - 5)
                    runColor = c
                    runLen = 1
                }
            }
            if (runLen >= 5) penalty += 3 + (runLen - 5)
        }

        // Rule 2: 2x2 blocks of same color
        for (y in 0 until size - 1) {
            for (x in 0 until size - 1) {
                val c = modules[y * size + x]
                if (c == modules[y * size + x + 1] &&
                    c == modules[(y + 1) * size + x] &&
                    c == modules[(y + 1) * size + x + 1]
                ) {
                    penalty += 3
                }
            }
        }

        // Rule 3: Finder-like 1:1:3:1:1 patterns flanked by 4 light modules
        fun matchFinderLike(get: (Int) -> Boolean): Int {
            var hits = 0
            for (i in 0..size - 11) {
                val p1 = get(i) && !get(i + 1) && get(i + 2) && get(i + 3) && get(i + 4) &&
                    !get(i + 5) && get(i + 6) && !get(i + 7) && !get(i + 8) && !get(i + 9) && !get(i + 10)
                val p2 = !get(i) && !get(i + 1) && !get(i + 2) && !get(i + 3) && get(i + 4) &&
                    !get(i + 5) && get(i + 6) && get(i + 7) && get(i + 8) && !get(i + 9) && get(i + 10)
                if (p1 || p2) hits += 40
            }
            return hits
        }
        for (y in 0 until size) {
            penalty += matchFinderLike { x -> modules[y * size + x] }
        }
        for (x in 0 until size) {
            penalty += matchFinderLike { y -> modules[y * size + x] }
        }

        // Rule 4: Dark/light balance
        var darkCount = 0
        for (m in modules) if (m) darkCount++
        val total = size * size
        val percentDiff = kotlin.math.abs(darkCount * 20 - total * 10) / total
        penalty += percentDiff * 10

        return penalty
    }
}

@Composable
internal fun QrCodeCanvas(
    payload: String,
    modifier: Modifier = Modifier,
    size: Dp = 188.dp,
    quietZoneModules: Int = 4,
    darkColor: Color = Color(0xFF111114),
    lightColor: Color = Color.White,
) {
    val matrix = remember(payload) {
        runCatching { QrCodeEncoder.encode(payload) }.getOrNull()
    }
    Box(
        modifier = modifier
            .size(size)
            .clip(RoundedCornerShape(14.dp))
            .background(lightColor)
            .padding(6.dp),
        contentAlignment = Alignment.Center,
    ) {
        if (matrix != null) {
            Canvas(modifier = Modifier.size(size - 12.dp)) {
                val totalModules = matrix.size + quietZoneModules * 2
                val cell = this.size.minDimension / totalModules.toFloat()
                drawRect(color = lightColor)
                for (y in 0 until matrix.size) {
                    for (x in 0 until matrix.size) {
                        if (matrix.get(x, y)) {
                            drawRect(
                                color = darkColor,
                                topLeft = Offset((x + quietZoneModules) * cell, (y + quietZoneModules) * cell),
                                size = Size(cell + 0.35f, cell + 0.35f),
                            )
                        }
                    }
                }
            }
        } else {
            Text(
                text = "内容过长，请复制链接",
                color = darkColor,
                fontSize = 12.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(8.dp),
            )
        }
    }
}
