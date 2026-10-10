package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.weight
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
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
    onConfirm: (String, String, String?, String, String?, String?) -> Unit,
    onSetNotifications: (String, Boolean) -> Unit,
) {
    val target = agent ?: return
    var name by remember(target.id, target.name) { mutableStateOf(target.name) }
    var title by remember(target.id, target.title) { mutableStateOf(target.title.orEmpty()) }
    var description by remember(target.id, target.description) { mutableStateOf(target.description) }
    var avatarShape by remember(target.id, target.avatarShape) { mutableStateOf(target.avatarShape) }
    var avatarColor by remember(target.id, target.avatarColor) { mutableStateOf(target.avatarColor) }

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
                if (!target.isGroup) {
                    OutlinedTextField(
                        value = title,
                        onValueChange = { title = it },
                        label = { Text("Title") },
                        singleLine = true,
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("agent-profile-title-${target.id}"),
                    )
                }
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
                ClothGhostAvatarAndroid(
                    botId = target.id,
                    size = 72.dp,
                    avatarShape = avatarShape,
                    avatarColor = avatarColor,
                    modifier = Modifier.testTag("agent-profile-avatar-preview-${target.id}"),
                )
                Text("Avatar shape")
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    listOf("circle", "square", "squircle").forEach { shape ->
                        TextButton(
                            onClick = { avatarShape = shape },
                            modifier = Modifier.testTag("agent-profile-avatar-shape-${target.id}-$shape"),
                        ) { Text(if (avatarShape == shape) "✓ $shape" else shape) }
                    }
                }
                Text("Avatar color")
                listOf(
                    listOf("#00C978", "#1685F7", "#8A4CFF"),
                    listOf("#FF681D", "#EE2546", "#F9A516"),
                ).forEach { colors ->
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        colors.forEach { color ->
                            TextButton(
                                onClick = { avatarColor = color },
                                modifier = Modifier.testTag(
                                    "agent-profile-avatar-color-${target.id}-${color.removePrefix("#")}",
                                ),
                            ) { Text(if (avatarColor == color) "✓ $color" else color) }
                        }
                    }
                }
                TextButton(
                    onClick = { avatarShape = null; avatarColor = null },
                    modifier = Modifier.testTag("agent-profile-avatar-reset-${target.id}"),
                ) { Text("Use default avatar") }
                if (!target.isGroup) {
                    Row(
                        horizontalArrangement = Arrangement.SpaceBetween,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text("Notifications")
                            Text("Get notified when this agent finishes or needs input")
                        }
                        Switch(
                            checked = target.notifyOnUpdatesEnabled,
                            onCheckedChange = { onSetNotifications(target.id, it) },
                            modifier = Modifier.testTag("agent-profile-notifications-${target.id}"),
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = name.isNotBlank(),
                onClick = { onConfirm(target.id, name, title.takeIf { !target.isGroup }, description, avatarShape, avatarColor) },
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
