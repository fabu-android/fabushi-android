package com.ombhrum.fabushi

import androidx.compose.material3.AlertDialog
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * Dedicated host for stateful product modals such as model selection and feedback.
 *
 * These surfaces are intentionally distinct from [AppAlertController]: AppAlert owns queued,
 * action-oriented alerts, while product modals own richer editable/selection state in their
 * feature. Keeping Material3 AlertDialog construction here prevents ProductionRenderer from
 * becoming a second generic alert owner without forcing form state into AppAlert's action model.
 */
@Composable
internal fun FabushiProductModal(
    onDismissRequest: () -> Unit,
    title: @Composable () -> Unit,
    text: @Composable () -> Unit,
    confirmButton: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    dismissButton: @Composable () -> Unit = {},
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        title = title,
        text = text,
        confirmButton = confirmButton,
        modifier = modifier,
        dismissButton = dismissButton,
    )
}
