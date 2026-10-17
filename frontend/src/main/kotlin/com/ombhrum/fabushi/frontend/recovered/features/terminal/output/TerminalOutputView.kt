package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
internal fun TerminalOutputView(
    snapshot: TerminalOutputSnapshot,
) {
    val running = snapshot.status == TerminalOutputStatus.RUNNING
    val statusLabel = when (snapshot.status) {
        TerminalOutputStatus.IDLE -> "Idle"
        TerminalOutputStatus.RUNNING -> "Running"
        TerminalOutputStatus.EXITED -> "Exited"
        TerminalOutputStatus.ERROR -> "Error"
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xFFF6F6F7), RoundedCornerShape(14.dp))
            .semantics {
                contentDescription = "Terminal output, $statusLabel"
                if (running) liveRegion = LiveRegionMode.Polite
            }
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("Terminal", fontWeight = FontWeight.SemiBold)
        Text(
            "$ " + snapshot.command,
            color = GrokMobileInk,
            fontFamily = FontFamily.Monospace,
            fontSize = 12.sp,
        )
        snapshot.cwd?.takeIf(String::isNotBlank)?.let {
            Text(it, color = GrokMobileMuted, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
        }
        Text(
            statusLabel,
            color = if (snapshot.status == TerminalOutputStatus.ERROR) Color.Red else GrokMobileMuted,
            fontSize = 11.sp,
        )
        if (snapshot.output.isNotBlank()) {
            SelectionContainer {
                Text(
                    snapshot.output,
                    color = GrokMobileInk,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                )
            }
        }
    }
}
