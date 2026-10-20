package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

internal const val ANDROID_TRANSCRIPT_LOAD_ERROR_TITLE = "Couldn't load conversation"
internal const val ANDROID_TRANSCRIPT_LOAD_ERROR_DETAIL =
    "Couldn't load this conversation. Check your connection and try again."
internal const val ANDROID_TRANSCRIPT_LOAD_ERROR_RETRY = "Retry"

@Composable
internal fun AndroidTranscriptLoadError(
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier
            .fillMaxWidth()
            .padding(24.dp)
            .semantics { liveRegion = LiveRegionMode.Assertive }
            .testTag("android-transcript-load-error"),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            ANDROID_TRANSCRIPT_LOAD_ERROR_TITLE,
            color = GrokMobileInk,
            fontWeight = FontWeight.SemiBold,
            fontSize = 18.sp,
        )
        Text(
            ANDROID_TRANSCRIPT_LOAD_ERROR_DETAIL,
            color = GrokMobileMuted,
            fontSize = 13.sp,
        )
        Button(
            onClick = onRetry,
            modifier = Modifier.testTag("android-transcript-load-error-retry"),
        ) {
            Text(ANDROID_TRANSCRIPT_LOAD_ERROR_RETRY)
        }
    }
}
