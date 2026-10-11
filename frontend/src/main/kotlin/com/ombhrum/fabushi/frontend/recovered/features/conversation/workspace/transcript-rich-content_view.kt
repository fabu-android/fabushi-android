package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Native Compose consumer for the presentation-only rich transcript projection.
 *
 * This is intentionally not a markdown WebView and never executes HTML/JavaScript. Unsupported
 * syntax remains visible as text.
 */
@Composable
internal fun MobileAssistantRichTranscriptContent(
    text: String,
    streaming: Boolean,
    bubbleShape: RoundedCornerShape,
) {
    val blocks = projectTranscriptRichContent(text)
    Column(
        Modifier
            .background(Color.Black.copy(alpha = 0.055f), bubbleShape)
            .padding(horizontal = 15.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (streaming && text.isEmpty()) {
            Text("•••", color = GrokMobileMuted, fontSize = 14.sp)
        } else {
            blocks.forEach { block ->
                MobileTranscriptRichBlock(block)
            }
            if (streaming) {
                Text("▌", color = GrokMobileInk, fontSize = 16.sp)
            }
        }
    }
}

@Composable
internal fun MobileUserRichTranscriptContent(
    text: String,
    richTextJson: String?,
    bubbleShape: RoundedCornerShape,
) {
    val blocks = projectTranscriptTipTapDocument(richTextJson)
    Column(
        Modifier
            .background(Color.Black, bubbleShape)
            .padding(horizontal = 15.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        if (blocks == null) {
            if (text.isNotBlank()) Text(text, color = Color.White, fontSize = 16.sp)
        } else {
            blocks.forEach { block -> MobileUserTipTapBlock(block) }
        }
    }
}

@Composable
private fun MobileUserTipTapBlock(block: TranscriptTipTapBlock) {
    when (block) {
        is TranscriptTipTapBlock.Paragraph -> MobileUserTipTapInline(block.inline, 16)
        is TranscriptTipTapBlock.Heading -> MobileUserTipTapInline(
            block.inline,
            when (block.level) {
                1 -> 21
                2 -> 19
                3 -> 18
                else -> 16
            },
            forceBold = true,
        )
        is TranscriptTipTapBlock.Quote -> Column(
            Modifier
                .fillMaxWidth()
                .background(Color.White.copy(alpha = 0.10f), RoundedCornerShape(6.dp))
                .padding(horizontal = 9.dp, vertical = 7.dp),
            verticalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            block.blocks.forEach { MobileUserTipTapBlock(it) }
        }
        is TranscriptTipTapBlock.ListBlock -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            block.items.forEachIndexed { index, item ->
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    Text(
                        if (block.ordered) "${block.start + index}." else "•",
                        color = Color.White,
                        fontSize = 15.sp,
                    )
                    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        item.forEach { MobileUserTipTapBlock(it) }
                    }
                }
            }
        }
        is TranscriptTipTapBlock.Code -> Text(
            block.text,
            color = Color.White,
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
            modifier = Modifier
                .fillMaxWidth()
                .background(Color.White.copy(alpha = 0.10f), RoundedCornerShape(6.dp))
                .padding(7.dp),
        )
        TranscriptTipTapBlock.HorizontalRule -> Box(
            Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(Color.White.copy(alpha = 0.24f)),
        )
    }
}

@Composable
private fun MobileUserTipTapInline(
    inline: List<TranscriptTipTapInline>,
    fontSize: Int,
    forceBold: Boolean = false,
) {
    val uriHandler = LocalUriHandler.current
    val annotated = buildAnnotatedString {
        inline.forEach { span ->
            val start = length
            append(span.text)
            val end = length
            if (start == end) return@forEach
            val decorations = buildList {
                if (span.strike) add(TextDecoration.LineThrough)
                if (span.underline) add(TextDecoration.Underline)
            }
            addStyle(
                SpanStyle(
                    fontWeight = if (forceBold || span.bold) FontWeight.Bold else null,
                    fontStyle = if (span.italic) FontStyle.Italic else null,
                    textDecoration = decorations.takeIf { it.isNotEmpty() }?.let(TextDecoration::combine),
                    fontFamily = if (span.code) FontFamily.Monospace else null,
                    color = if (span.url != null) Color(0xFF8CCBFF) else Color.White,
                ),
                start,
                end,
            )
            span.url?.let { url ->
                addStringAnnotation(tag = "url", annotation = url, start = start, end = end)
            }
        }
    }
    ClickableText(
        text = annotated,
        style = androidx.compose.ui.text.TextStyle(
            color = Color.White,
            fontSize = fontSize.sp,
        ),
        onClick = { offset ->
            annotated
                .getStringAnnotations(tag = "url", start = offset, end = offset)
                .firstOrNull()
                ?.item
                ?.let { url -> runCatching { uriHandler.openUri(url) } }
        },
    )
}

@Composable
private fun MobileTranscriptRichBlock(block: TranscriptRichBlock) {
    when (block) {
        is TranscriptRichBlock.Paragraph -> MobileTranscriptParagraph(block.text)
        is TranscriptRichBlock.Heading -> Text(
            block.text,
            color = GrokMobileInk,
            fontSize = when (block.level) {
                1 -> 21.sp
                2 -> 19.sp
                else -> 17.sp
            },
            fontWeight = FontWeight.Bold,
        )
        is TranscriptRichBlock.Quote -> Text(
            block.text,
            color = GrokMobileMuted,
            fontSize = 15.sp,
            modifier = Modifier
                .fillMaxWidth()
                .background(Color.Black.copy(alpha = 0.035f), RoundedCornerShape(6.dp))
                .padding(horizontal = 10.dp, vertical = 8.dp),
        )
        is TranscriptRichBlock.ListBlock -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            block.items.forEachIndexed { index, item ->
                val prefix = when {
                    item.task -> if (item.checked) "☑ " else "☐ "
                    block.ordered -> "${block.start + index}. "
                    else -> "• "
                }
                Text(prefix + item.text, color = GrokMobileInk, fontSize = 15.sp)
            }
        }
        is TranscriptRichBlock.Table -> {
            val scroll = rememberScrollState()
            Column(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(scroll)
                    .background(Color.Black.copy(alpha = 0.025f), RoundedCornerShape(6.dp))
                    .padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(block.headers.joinToString("  |  "), color = GrokMobileInk, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                block.rows.forEach { row ->
                    Text(row.joinToString("  |  "), color = GrokMobileInk, fontSize = 13.sp)
                }
            }
        }
        is TranscriptRichBlock.Math -> Text(
            "\$\$\n${block.expression}\n\$\$",
            color = GrokMobileInk,
            fontFamily = FontFamily.Monospace,
            fontSize = 14.sp,
        )
        is TranscriptRichBlock.Code -> MobileTranscriptCodeBlock(block)
        TranscriptRichBlock.HorizontalRule -> Box(
            Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(Color.Black.copy(alpha = 0.12f)),
        )
    }
}

@Composable
private fun MobileTranscriptParagraph(text: String) {
    val uriHandler = LocalUriHandler.current
    val inline = projectTranscriptInlineContent(text)
    val annotated = buildAnnotatedString {
        inline.forEach { span ->
            val start = length
            append(span.text)
            val end = length
            if (start == end) return@forEach
            addStyle(
                SpanStyle(
                    fontWeight = if (span.bold) FontWeight.Bold else null,
                    fontStyle = if (span.italic) FontStyle.Italic else null,
                    textDecoration = if (span.strike) TextDecoration.LineThrough else null,
                    fontFamily = if (span.code || span.math) FontFamily.Monospace else null,
                    color = if (span.url != null) Color(0xFF2A72D4) else GrokMobileInk,
                ),
                start,
                end,
            )
            span.url?.let { url ->
                addStringAnnotation(tag = "url", annotation = url, start = start, end = end)
            }
        }
    }
    ClickableText(
        text = annotated,
        style = androidx.compose.ui.text.TextStyle(
            color = GrokMobileInk,
            fontSize = 16.sp,
        ),
        onClick = { offset ->
            annotated
                .getStringAnnotations(tag = "url", start = offset, end = offset)
                .firstOrNull()
                ?.item
                ?.let { url -> runCatching { uriHandler.openUri(url) } }
        },
    )
}

@Composable
private fun MobileTranscriptCodeBlock(block: TranscriptRichBlock.Code) {
    val clipboard = LocalClipboardManager.current
    Column(
        Modifier
            .fillMaxWidth()
            .background(Color.Black.copy(alpha = 0.07f), RoundedCornerShape(8.dp))
            .padding(9.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(
                block.language.ifBlank { "code" },
                color = GrokMobileMuted,
                fontSize = 11.sp,
            )
            TextButton(
                onClick = { clipboard.setText(AnnotatedString(block.code)) },
            ) {
                Text("Copy", fontSize = 11.sp)
            }
        }
        Text(
            block.code,
            color = GrokMobileInk,
            fontFamily = FontFamily.Monospace,
            fontSize = 13.sp,
        )
    }
}
