package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import java.net.URI

/**
 * Presentation-only projection for Desktop assistant message image/channel/source-trust and
 * send-message:text URL-card seams.
 *
 * Desktop authority:
 * frontend/src/recovered/features/conversation/cards/transcript-card/send-message-text.ts
 * frontend/src/recovered/features/conversation/cards/transcript-card/url-card.ts
 * frontend/src/recovered/features/conversation/workspace/transcript.tsx
 *
 * Host transcript JSON remains canonical. This file validates immutable data and never owns or
 * retries a message side effect.
 */
data class MobileTranscriptImage(
    val url: String,
    val alt: String? = null,
)

internal data class TranscriptAssistantPresentation(
    val images: List<MobileTranscriptImage> = emptyList(),
    val channel: String? = null,
    val sourceTrusted: Boolean = false,
    val urlCardUrl: String? = null,
)

internal data class TranscriptSendMessageTextProjection(
    val id: String,
    val content: String,
    val streaming: Boolean,
    val images: List<MobileTranscriptImage>,
    val channel: String?,
    val sourceTrusted: Boolean,
    val urlCardUrl: String?,
)

private val transcriptBareMarkdownLink =
    Regex("""^\[[^\]\n]*]\(\s*([^\)\s]+)(?:\s+[^)]*)?\)\s*$""")

private fun parseTranscriptImages(value: Any?): List<MobileTranscriptImage>? {
    if (value == null) return emptyList()
    val rows = value as? JSONArray ?: return null
    return buildList {
        for (index in 0 until rows.length()) {
            val row = rows.opt(index) as? JSONObject ?: return null
            val rawUrl = row.opt("url")
            if (rawUrl !is String || rawUrl.isEmpty()) return null
            val rawAlt = if (!row.has("alt")) null else row.opt("alt")
            if (rawAlt != null && rawAlt !is String) return null
            add(MobileTranscriptImage(url = rawUrl, alt = rawAlt as? String))
        }
    }
}

internal fun extractTranscriptBareLink(content: String): String? {
    val trimmed = content.trim()
    val markdown = transcriptBareMarkdownLink.matchEntire(trimmed)
    return normalizeTranscriptHttpUrl(markdown?.groupValues?.get(1) ?: trimmed)
}

/**
 * Android cleartext traffic is disabled. Remote transcript images therefore load only from HTTPS
 * after the Host/source-trust seam has already accepted the row. Unsupported URLs stay visible as
 * inert image placeholders instead of silently broadening network authority.
 */
internal fun normalizeTranscriptRemoteImageUrl(value: String): String? {
    val normalized = normalizeTranscriptHttpUrl(value) ?: return null
    val parsed = runCatching { URI(normalized) }.getOrNull() ?: return null
    return normalized.takeIf { parsed.scheme.equals("https", ignoreCase = true) }
}

/** Strict mirror of the Desktop send-message:text projector. Malformed cards fail closed. */
internal fun projectTranscriptSendMessageText(entry: JSONObject): TranscriptSendMessageTextProjection? {
    if (entry.opt("kind") != "send-message") return null
    val id = (entry.opt("id") as? String)?.takeIf(String::isNotEmpty) ?: return null
    val message = entry.opt("message") as? JSONObject ?: return null
    if (message.opt("type") != "text") return null
    val content = message.opt("content") as? String ?: return null

    val streaming = when {
        !entry.has("streaming") -> false
        entry.isNull("streaming") -> return null
        entry.opt("streaming") is Boolean -> entry.getBoolean("streaming")
        else -> return null
    }
    if (entry.has("timestampMs")) {
        if (entry.isNull("timestampMs") || entry.opt("timestampMs") !is Number) return null
        val timestamp = (entry.opt("timestampMs") as Number).toDouble()
        if (!timestamp.isFinite()) return null
    }

    val channel = when {
        !message.has("channel") || message.isNull("channel") -> null
        message.opt("channel") is String -> message.getString("channel")
        else -> return null
    }
    val images = when {
        !message.has("images") -> emptyList()
        else -> parseTranscriptImages(message.opt("images")) ?: return null
    }
    val urlCardUrl = if (!streaming && images.isEmpty()) extractTranscriptBareLink(content) else null
    return TranscriptSendMessageTextProjection(
        id = id,
        content = content,
        streaming = streaming,
        images = images,
        channel = channel,
        sourceTrusted = true,
        urlCardUrl = urlCardUrl,
    )
}

private fun explicitTrustedFlag(value: JSONObject): Boolean =
    value.opt("isSourceTrusted") == true

private fun projectedUrlCard(value: JSONObject): String? {
    val presentation = value.optJSONObject("presentation") ?: return null
    if (presentation.opt("kind") != "url-card") return null
    return (presentation.opt("url") as? String)?.let(::normalizeTranscriptHttpUrl)
}

/**
 * Reads only presentation metadata from a canonical ordinary assistant message. If the Host
 * retained a full sendMessageText projection, reuse the strict projector above. Otherwise accept
 * only well-typed direct fields; malformed optional media is dropped rather than invalidating the
 * canonical text message.
 */
internal fun projectCanonicalAssistantPresentation(entry: JSONObject): TranscriptAssistantPresentation {
    val retained = entry.optJSONObject("sendMessageText")
    if (retained != null) {
        val strict = projectTranscriptSendMessageText(retained)
        if (strict != null) {
            return TranscriptAssistantPresentation(
                images = strict.images,
                channel = strict.channel,
                sourceTrusted = explicitTrustedFlag(retained) || strict.sourceTrusted,
                urlCardUrl = projectedUrlCard(retained) ?: strict.urlCardUrl,
            )
        }
    }

    val images = if (entry.has("images")) {
        parseTranscriptImages(entry.opt("images")) ?: emptyList()
    } else {
        emptyList()
    }
    val channel = when (val raw = entry.opt("channel")) {
        is String -> raw
        else -> null
    }
    val sourceTrusted = explicitTrustedFlag(entry)
    val streaming = entry.opt("isStreaming") == true
    val content = (entry.opt("content") as? String).orEmpty()
    val explicitUrl = when {
        entry.opt("presentationKind") == "url-card" ->
            (entry.opt("url") as? String)?.let(::normalizeTranscriptHttpUrl)
        else -> null
    }
    val inferredUrl =
        if (sourceTrusted && !streaming && images.isEmpty()) extractTranscriptBareLink(content) else null
    return TranscriptAssistantPresentation(
        images = images,
        channel = channel,
        sourceTrusted = sourceTrusted,
        urlCardUrl = explicitUrl ?: inferredUrl,
    )
}
