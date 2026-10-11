package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
            "$$\n${block.expression}\n$$",
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
    val normalized = normalizeTranscriptHttpUrl(text)
    Text(
        text,
        color = if (normalized == null) GrokMobileInk else Color(0xFF2A72D4),
        fontSize = 16.sp,
        modifier = if (normalized == null) {
            Modifier
        } else {
            Modifier.clickable {
                runCatching { uriHandler.openUri(normalized) }
            }
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
