package com.ombhrum.fabushi

import androidx.compose.material3.AlertDialog
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * Native modal primitive for specialized interactive surfaces.
 *
 * App-wide confirm/destructive alerts stay owned by AppAlertController/AppAlertHost. This wrapper
 * is intentionally presentation-only for richer modal surfaces such as model selection and
 * feedback forms, so ProductionRenderer never instantiates a second generic alert owner.
 */
@Composable
internal fun FabushiModalDialog(
    onDismissRequest: () -> Unit,
    confirmButton: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    dismissButton: @Composable (() -> Unit)? = null,
    title: @Composable (() -> Unit)? = null,
    text: @Composable (() -> Unit)? = null,
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        confirmButton = confirmButton,
        modifier = modifier,
        dismissButton = dismissButton,
        title = title,
        text = text,
    )
}
