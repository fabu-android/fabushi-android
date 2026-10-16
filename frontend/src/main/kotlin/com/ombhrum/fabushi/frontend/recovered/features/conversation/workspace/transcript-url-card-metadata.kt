package com.ombhrum.fabushi

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.URI
import java.net.UnknownHostException
import java.util.Locale
import kotlinx.coroutines.CompletableDeferred
import okhttp3.Dns
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

internal data class TranscriptLinkCardMetadata(
    val title: String? = null,
    val description: String? = null,
    val siteName: String? = null,
    val hostname: String? = null,
)

internal enum class TranscriptLinkCardStatus { IDLE, LOADING, READY, EMPTY, FAILED }

internal data class TranscriptLinkCardSnapshot(
    val status: TranscriptLinkCardStatus,
    val metadata: TranscriptLinkCardMetadata? = null,
)

private const val MAX_TRANSCRIPT_LINK_HTML_BYTES = 512 * 1024
private const val MAX_TRANSCRIPT_LINK_TEXT = 512
private val transcriptTagRegex = Regex("<[^>]+>")
private val transcriptSpaceRegex = Regex("\\s+")
private val transcriptTitleRegex = Regex(
    """<title\b[^>]*>(.*?)</title>""",
    setOf(RegexOption.IGNORE_CASE, RegexOption.DOT_MATCHES_ALL),
)
private val transcriptMetaRegex = Regex(
    """<meta\b[^>]*(?:name|property)\s*=\s*["']([^"']+)["'][^>]*content\s*=\s*["']([^"']*)["'][^>]*>|<meta\b[^>]*content\s*=\s*["']([^"']*)["'][^>]*(?:name|property)\s*=\s*["']([^"']+)["'][^>]*>""",
    setOf(RegexOption.IGNORE_CASE, RegexOption.DOT_MATCHES_ALL),
)

private fun decodeTranscriptHtmlText(value: String): String? {
    val decoded = value
        .replace("&amp;", "&", ignoreCase = true)
        .replace("&quot;", "\"", ignoreCase = true)
        .replace("&#39;", "'", ignoreCase = true)
        .replace("&lt;", "<", ignoreCase = true)
        .replace("&gt;", ">", ignoreCase = true)
        .replace(transcriptTagRegex, " ")
        .replace(transcriptSpaceRegex, " ")
        .trim()
        .take(MAX_TRANSCRIPT_LINK_TEXT)
    return decoded.takeIf(String::isNotEmpty)
}

internal fun projectTranscriptLinkMetadataHtml(
    url: String,
    html: String,
): TranscriptLinkCardMetadata? {
    val normalized = normalizeTranscriptHttpUrl(url) ?: return null
    val uri = runCatching { URI(normalized) }.getOrNull() ?: return null
    val hostname = uri.host?.takeIf(String::isNotBlank) ?: return null
    val meta = linkedMapOf<String, String>()
    transcriptMetaRegex.findAll(html).forEach { match ->
        val firstKey = match.groupValues[1]
        val firstValue = match.groupValues[2]
        val secondValue = match.groupValues[3]
        val secondKey = match.groupValues[4]
        val key = (firstKey.ifBlank { secondKey }).trim().lowercase(Locale.US)
        val value = (firstValue.ifBlank { secondValue }).let(::decodeTranscriptHtmlText)
        if (key.isNotEmpty() && value != null && key !in meta) meta[key] = value
    }
    val title = meta["og:title"]
        ?: meta["twitter:title"]
        ?: transcriptTitleRegex.find(html)?.groupValues?.getOrNull(1)?.let(::decodeTranscriptHtmlText)
    val description = meta["og:description"] ?: meta["description"] ?: meta["twitter:description"]
    val siteName = meta["og:site_name"]
    return TranscriptLinkCardMetadata(
        title = title,
        description = description,
        siteName = siteName,
        hostname = hostname,
    )
}

internal fun isPublicTranscriptLinkAddress(address: InetAddress): Boolean {
    if (
        address.isAnyLocalAddress ||
        address.isLoopbackAddress ||
        address.isLinkLocalAddress ||
        address.isSiteLocalAddress ||
        address.isMulticastAddress
    ) return false
    return when (address) {
        is Inet4Address -> {
            val b = address.address.map { it.toInt() and 0xFF }
            when {
                b[0] == 0 -> false
                b[0] == 100 && b[1] in 64..127 -> false
                b[0] == 192 && b[1] == 0 && b[2] == 0 -> false
                b[0] == 198 && b[1] in 18..19 -> false
                b[0] >= 224 -> false
                else -> true
            }
        }
        is Inet6Address -> {
            val bytes = address.address
            val first = bytes[0].toInt() and 0xFF
            val second = bytes[1].toInt() and 0xFF
            !(first and 0xFE == 0xFC || (first == 0xFE && second and 0xC0 == 0x80))
        }
        else -> false
    }
}

private fun transcriptMetadataUrl(raw: String): String? {
    val normalized = normalizeTranscriptHttpUrl(raw) ?: return null
    val uri = runCatching { URI(normalized) }.getOrNull() ?: return null
    if (!uri.scheme.equals("https", ignoreCase = true)) return null
    if (uri.userInfo != null || uri.host.isNullOrBlank()) return null
    return normalized
}

private val transcriptPublicDns = object : Dns {
    override fun lookup(hostname: String): List<InetAddress> {
        val addresses = InetAddress.getAllByName(hostname).toList()
        if (addresses.isEmpty() || addresses.any { !isPublicTranscriptLinkAddress(it) }) {
            throw UnknownHostException("Transcript URL-card metadata host is not public")
        }
        return addresses
    }
}

private fun readTranscriptMetadataHtml(response: Response): String? {
    if (!response.isSuccessful || !response.request.url.isHttps) return null
    val body = response.body
    val type = body.contentType()
    if (type != null && type.type != "text" && type.subtype.lowercase(Locale.US) != "xhtml+xml") return null
    val declared = body.contentLength()
    if (declared > MAX_TRANSCRIPT_LINK_HTML_BYTES) return null
    val source = body.source()
    val bytes = source.readByteArray(MAX_TRANSCRIPT_LINK_HTML_BYTES.toLong() + 1L)
    if (bytes.size > MAX_TRANSCRIPT_LINK_HTML_BYTES) return null
    val charset = type?.charset(Charsets.UTF_8) ?: Charsets.UTF_8
    return bytes.toString(charset)
}

/**
 * Presentation-only URL metadata cache. Canonical transcript URL ownership stays in Rust Host.
 * Pending metadata requests are fenced by generation; dispose invalidates every in-flight result.
 */
internal class TranscriptUrlCardMetadataProvider(
    private val client: OkHttpClient = OkHttpClient.Builder()
        .dns(transcriptPublicDns)
        .followRedirects(true)
        .followSslRedirects(false)
        .build(),
) {
    private data class Entry(
        var generation: Long = 0,
        var snapshot: TranscriptLinkCardSnapshot = TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.IDLE),
        var request: CompletableDeferred<TranscriptLinkCardSnapshot>? = null,
    )

    private val lock = Any()
    private val entries = LinkedHashMap<String, Entry>()
    private var disposed = false

    fun cached(rawUrl: String): TranscriptLinkCardSnapshot? {
        val url = transcriptMetadataUrl(rawUrl) ?: return null
        return synchronized(lock) { entries[url]?.snapshot }
    }

    suspend fun load(rawUrl: String, healFailed: Boolean = false): TranscriptLinkCardSnapshot {
        val url = transcriptMetadataUrl(rawUrl)
            ?: return TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY)
        lateinit var entry: Entry
        lateinit var request: CompletableDeferred<TranscriptLinkCardSnapshot>
        var generation = 0L
        var ownsRequest = false
        synchronized(lock) {
            if (disposed) return TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY)
            entry = entries.getOrPut(url) { Entry() }
            when (entry.snapshot.status) {
                TranscriptLinkCardStatus.READY,
                TranscriptLinkCardStatus.EMPTY -> return entry.snapshot
                TranscriptLinkCardStatus.FAILED -> if (!healFailed) return entry.snapshot
                else -> Unit
            }
            val existing = entry.request
            if (existing != null) {
                request = existing
            } else {
                entry.generation += 1
                generation = entry.generation
                entry.snapshot = TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.LOADING)
                request = CompletableDeferred()
                entry.request = request
                ownsRequest = true
            }
        }
        if (!ownsRequest) return request.await()

        val settled = runCatching {
            val requestValue = Request.Builder()
                .url(url)
                .header("User-Agent", "Fabushi-Android-LinkPreview/1")
                .get()
                .build()
            client.newCall(requestValue).execute().use { response ->
                val html = readTranscriptMetadataHtml(response)
                    ?: return@use TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY)
                val metadata = projectTranscriptLinkMetadataHtml(response.request.url.toString(), html)
                    ?: return@use TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY)
                TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.READY, metadata)
            }
        }.getOrElse { TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.FAILED) }

        val visible = synchronized(lock) {
            if (disposed || entry.generation != generation) {
                TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY)
            } else {
                entry.snapshot = settled
                if (entry.request === request) entry.request = null
                settled
            }
        }
        request.complete(visible)
        return visible
    }

    fun reset() {
        synchronized(lock) {
            if (disposed) return
            entries.values.forEach { entry ->
                entry.generation += 1
                entry.request?.complete(TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.IDLE))
                entry.request = null
                entry.snapshot = TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.IDLE)
            }
        }
    }

    fun dispose() {
        synchronized(lock) {
            if (disposed) return
            disposed = true
            entries.values.forEach { entry ->
                entry.generation += 1
                entry.request?.complete(TranscriptLinkCardSnapshot(TranscriptLinkCardStatus.EMPTY))
                entry.request = null
            }
            entries.clear()
        }
    }
}
