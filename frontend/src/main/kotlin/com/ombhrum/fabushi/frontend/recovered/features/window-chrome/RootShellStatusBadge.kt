package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
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
    Popup(properties = PopupProperties(focusable = false)) {
        Surface {
            Row(modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp)) {
                Text(projection.label)
            }
        }
    }
}
