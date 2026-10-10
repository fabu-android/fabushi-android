package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun AgentProfileEditor(
    agent: MobileBotSummaryAndroid?,
    onClose: () -> Unit,
    onConfirm: (String, String, String) -> Unit,
) {
    val target = agent ?: return
    var name by remember(target.id, target.name) { mutableStateOf(target.name) }
    var description by remember(target.id, target.description) { mutableStateOf(target.description) }

    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Agent profile") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Name") },
                    singleLine = true,
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent-profile-name-${target.id}"),
                )
                OutlinedTextField(
                    value = description,
                    onValueChange = { description = it },
                    label = { Text("Description") },
                    minLines = 3,
                    maxLines = 6,
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent-profile-description-${target.id}"),
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = name.isNotBlank(),
                onClick = { onConfirm(target.id, name, description) },
                modifier = Modifier.testTag("agent-profile-save-${target.id}"),
            ) {
                Text("Save")
            }
        },
        dismissButton = {
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("agent-profile-cancel-${target.id}"),
            ) {
                Text("Cancel")
            }
        },
    )
}
