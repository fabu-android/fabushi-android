package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionPhase
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState

internal enum class RootShellStatusBadgeTone { WORKING, INFO, OFFLINE }

internal data class RootShellStatusBadgeProjection(
    val tone: RootShellStatusBadgeTone,
    val label: String,
)

internal fun rootShellStatusBadgeProjection(
    loggedIn: Boolean,
    connection: RootConnectionResilienceProjection,
): RootShellStatusBadgeProjection? {
    // Desktop hides browser/fullscreen modes. Android has no renderer-owned browser/fullscreen
    // connection mode; auth-hidden/native pre-login surfaces are represented by HIDDEN/loggedOut.
    if (!loggedIn || connection.phase == RootConnectionPhase.HIDDEN) return null
    return when (connection.transport) {
        RootConnectionTransportState.CONNECTING ->
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.WORKING, "Connecting")
        RootConnectionTransportState.CONNECTED ->
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.INFO, "Connected")
        RootConnectionTransportState.DOWN ->
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.OFFLINE, "Disconnected")
    }
}

@Composable
internal fun RootShellStatusBadge(
    loggedIn: Boolean,
    connection: RootConnectionResilienceProjection,
) {
    val projection = rootShellStatusBadgeProjection(loggedIn, connection) ?: return
    val dotColor: Color = when (projection.tone) {
        RootShellStatusBadgeTone.WORKING -> MaterialTheme.colorScheme.tertiary
        RootShellStatusBadgeTone.INFO -> MaterialTheme.colorScheme.primary
        RootShellStatusBadgeTone.OFFLINE -> MaterialTheme.colorScheme.error
    }
    Popup(
        alignment = androidx.compose.ui.Alignment.TopCenter,
        properties = PopupProperties(focusable = false),
    ) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .background(dotColor, CircleShape)
                .semantics { contentDescription = projection.label },
        )
    }
}
