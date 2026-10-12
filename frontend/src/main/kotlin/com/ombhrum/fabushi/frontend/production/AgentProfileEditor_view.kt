package com.ombhrum.fabushi

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import androidx.compose.ui.unit.dp
import com.ombhrum.fabushi.androidpreload.media.AGENT_AVATAR_MAX_ZOOM
import com.ombhrum.fabushi.androidpreload.media.AGENT_AVATAR_MIN_ZOOM
import com.ombhrum.fabushi.androidpreload.media.AndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.media.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidpreload.media.clampAndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.media.initialAndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.media.panAndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.AndroidGeneratedAvatarImage

@Composable
internal fun AgentProfileEditor(
    agent: MobileBotSummaryAndroid?,
    onClose: () -> Unit,
    onConfirm: (String, String, String?, String, String?, String?) -> Unit,
    onSetAvatarImage: (String, String?) -> Unit,
    onGenerateAvatarImage: suspend (String, String) -> Result<AndroidGeneratedAvatarImage>,
    onSetNotifications: (String, Boolean) -> Unit,
) {
    val target = agent ?: return
    var name by remember(target.id, target.name) { mutableStateOf(target.name) }
    var title by remember(target.id, target.title) { mutableStateOf(target.title.orEmpty()) }
    var description by remember(target.id, target.description) { mutableStateOf(target.description) }
    var avatarShape by remember(target.id, target.avatarShape) { mutableStateOf(target.avatarShape) }
    var avatarColor by remember(target.id, target.avatarColor) { mutableStateOf(target.avatarColor) }
    var avatarDataUrl by remember(target.id, target.avatarDataUrl) { mutableStateOf(target.avatarDataUrl) }
    var avatarImageError by remember(target.id) { mutableStateOf<String?>(null) }
    var avatarImageBusy by remember(target.id) { mutableStateOf(false) }
    var avatarGenerationPrompt by remember(target.id) { mutableStateOf("") }
    var stagedAvatarSource by remember(target.id) { mutableStateOf<AndroidAvatarImageSource?>(null) }
    var stagedAvatarCrop by remember(target.id) { mutableStateOf<AndroidAvatarCrop?>(null) }
    var stagedAvatarPreview by remember(target.id) { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val cropStageSize = 180.dp
    val cropStageSizePx = with(density) { cropStageSize.toPx() }
    val avatarImagePicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null || avatarImageBusy) return@rememberLauncherForActivityResult
        avatarImageBusy = true
        avatarImageError = null
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                stageAgentAvatarImage(context, uri)
            }
            result.onSuccess { source ->
                stagedAvatarSource = source
                stagedAvatarCrop = initialAndroidAvatarCrop(source)
                stagedAvatarPreview = null
            }.onFailure { error ->
                avatarImageError = error.message ?: "Avatar image import failed"
            }
            avatarImageBusy = false
        }
    }

    LaunchedEffect(stagedAvatarSource, stagedAvatarCrop) {
        val source = stagedAvatarSource
        val crop = stagedAvatarCrop
        if (source == null || crop == null) {
            stagedAvatarPreview = null
            return@LaunchedEffect
        }
        val encoded = withContext(Dispatchers.IO) {
            encodeAgentAvatarCrop(source, crop)
        }
        encoded.onSuccess { stagedAvatarPreview = it }
            .onFailure { error -> avatarImageError = error.message ?: "Avatar crop failed" }
    }

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
                val cropSource = stagedAvatarSource
                val crop = stagedAvatarCrop
                if (cropSource != null && crop != null) {
                    Box(
                        Modifier
                            .size(cropStageSize)
                            .pointerInput(cropSource, crop) {
                                detectDragGestures { change, dragAmount ->
                                    change.consume()
                                    stagedAvatarCrop = panAndroidAvatarCrop(
                                        source = cropSource,
                                        crop = crop,
                                        deltaStageX = dragAmount.x,
                                        deltaStageY = dragAmount.y,
                                        stageSizePx = cropStageSizePx,
                                    )
                                }
                            }
                            .testTag("agent-profile-avatar-crop-stage-${target.id}"),
                    ) {
                        AgentAvatarAndroid(
                            bot = target.copy(
                                avatarShape = avatarShape,
                                avatarColor = avatarColor,
                                avatarDataUrl = stagedAvatarPreview ?: cropSource.dataUrl,
                            ),
                            size = cropStageSize,
                        )
                    }
                    Text("Drag to reposition")
                    Slider(
                        value = crop.zoom,
                        onValueChange = { zoom ->
                            stagedAvatarCrop = clampAndroidAvatarCrop(cropSource, crop.copy(zoom = zoom))
                        },
                        valueRange = AGENT_AVATAR_MIN_ZOOM..AGENT_AVATAR_MAX_ZOOM,
                        modifier = Modifier.testTag("agent-profile-avatar-zoom-${target.id}"),
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        TextButton(
                            onClick = { stagedAvatarCrop = initialAndroidAvatarCrop(cropSource) },
                            modifier = Modifier.testTag("agent-profile-avatar-crop-reset-${target.id}"),
                        ) { Text("Restart") }
                        TextButton(
                            enabled = stagedAvatarPreview != null,
                            onClick = {
                                val committed = stagedAvatarPreview ?: return@TextButton
                                avatarDataUrl = committed
                                stagedAvatarSource = null
                                stagedAvatarCrop = null
                                stagedAvatarPreview = null
                                avatarImageError = null
                                onSetAvatarImage(target.id, committed)
                            },
                            modifier = Modifier.testTag("agent-profile-avatar-crop-save-${target.id}"),
                        ) { Text("Set image") }
                        TextButton(
                            onClick = {
                                stagedAvatarSource = null
                                stagedAvatarCrop = null
                                stagedAvatarPreview = null
                            },
                            modifier = Modifier.testTag("agent-profile-avatar-crop-cancel-${target.id}"),
                        ) { Text("Cancel crop") }
                    }
                } else {
                    AgentAvatarAndroid(
                        bot = target.copy(
                            avatarShape = avatarShape,
                            avatarColor = avatarColor,
                            avatarDataUrl = avatarDataUrl,
                        ),
                        size = 72.dp,
                        modifier = Modifier.testTag("agent-profile-avatar-preview-${target.id}"),
                    )
                }
                if (!target.isGroup) {
                    OutlinedTextField(
                        value = avatarGenerationPrompt,
                        onValueChange = { avatarGenerationPrompt = it },
                        label = { Text("Describe avatar") },
                        enabled = !avatarImageBusy,
                        singleLine = false,
                        minLines = 1,
                        maxLines = 3,
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("agent-profile-avatar-generate-prompt-${target.id}"),
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(
                        enabled = !avatarImageBusy,
                        onClick = {
                            avatarImagePicker.launch(
                                PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                            )
                        },
                        modifier = Modifier.testTag("agent-profile-avatar-image-pick-${target.id}"),
                    ) {
                        Text(if (avatarImageBusy) "Importing…" else if (avatarDataUrl.isNullOrBlank()) "Choose image" else "Replace image")
                    }
                    if (!target.isGroup) {
                        TextButton(
                            enabled = !avatarImageBusy && avatarGenerationPrompt.isNotBlank(),
                            onClick = {
                                val prompt = avatarGenerationPrompt.trim()
                                if (prompt.isEmpty()) return@TextButton
                                avatarImageBusy = true
                                avatarImageError = null
                                scope.launch {
                                    val generated = onGenerateAvatarImage(target.id, prompt)
                                    generated.onSuccess { image ->
                                        val staged = withContext(Dispatchers.IO) {
                                            stageGeneratedAgentAvatarImage(image.imageData, image.mimeType)
                                        }
                                        staged.onSuccess { source ->
                                            stagedAvatarSource = source
                                            stagedAvatarCrop = initialAndroidAvatarCrop(source)
                                            stagedAvatarPreview = null
                                        }.onFailure { error ->
                                            avatarImageError = error.message ?: "Generated avatar could not be prepared"
                                        }
                                    }.onFailure { error ->
                                        avatarImageError = error.message ?: "Avatar generation failed"
                                    }
                                    avatarImageBusy = false
                                }
                            },
                            modifier = Modifier.testTag("agent-profile-avatar-generate-${target.id}"),
                        ) {
                            Text(if (avatarImageBusy) "Generating…" else "Generate image")
                        }
                    }
                    if (!avatarDataUrl.isNullOrBlank()) {
                        TextButton(
                            enabled = !avatarImageBusy,
                            onClick = {
                                avatarDataUrl = null
                                avatarImageError = null
                                onSetAvatarImage(target.id, null)
                            },
                            modifier = Modifier.testTag("agent-profile-avatar-image-clear-${target.id}"),
                        ) { Text("Remove image") }
                    }
                }
                avatarImageError?.let { error ->
                    Text(
                        error,
                        modifier = Modifier.testTag("agent-profile-avatar-image-error-${target.id}"),
                    )
                }
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

                    Text("Channels")
                    Text("Connect this agent to a messaging platform so it can talk to people there.")
                    desktopMainAgentChannelManifests
                        .map { manifest -> projectAgentChannelRow(manifest, connection = null) }
                        .forEach { row ->
                            Row(
                                horizontalArrangement = Arrangement.SpaceBetween,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("agent-profile-channel-${target.id}-${row.manifest.platform}"),
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(row.manifest.displayName)
                                    Text(row.detail)
                                }
                                row.statusLabel?.let { label ->
                                    Text(
                                        label,
                                        modifier = Modifier.testTag(
                                            "agent-profile-channel-status-${target.id}-${row.manifest.platform}",
                                        ),
                                    )
                                }
                            }
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
