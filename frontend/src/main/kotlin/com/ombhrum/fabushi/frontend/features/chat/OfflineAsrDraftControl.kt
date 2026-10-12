package com.ombhrum.fabushi

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import java.util.concurrent.atomic.AtomicLong

internal class OfflineAsrGenerationFence {
    private val generation = AtomicLong(0L)

    fun begin(): Long = generation.incrementAndGet()

    fun accepts(token: Long): Boolean = token == generation.get()

    fun invalidate() {
        generation.incrementAndGet()
    }
}

internal fun applyOfflineAsrDraftResult(
    draftAtStart: String,
    currentDraft: String,
    recognizedText: String,
): String? {
    if (currentDraft != draftAtStart) return null
    val clean = recognizedText.trim()
    if (clean.isEmpty()) return currentDraft
    val prefix = currentDraft.trimEnd()
    return if (prefix.isEmpty()) clean else "$prefix $clean"
}

@Composable
internal fun OfflineAsrDraftControl(
    currentDraft: String,
    enabled: Boolean,
    onDraftChange: (String) -> Unit,
    sessionKey: String,
    testTag: String,
) {
    val context = LocalContext.current
    val latestDraft = rememberUpdatedState(currentDraft)
    val transcriber = remember(sessionKey) { NativeOfflineSpeechTranscriber(context.applicationContext) }
    val generationFence = remember(sessionKey) { OfflineAsrGenerationFence() }
    val available = remember { transcriber.isAvailable() }
    var listening by remember { mutableStateOf(false) }
    var processing by remember { mutableStateOf(false) }
    var error by remember(available) { mutableStateOf(if (available) null else "设备离线语音识别不可用") }

    fun startOfflineTranscription() {
        if (!available) {
            error = "设备离线语音识别不可用"
            return
        }
        val generation = generationFence.begin()
        val draftAtStart = latestDraft.value
        transcriber.start { result ->
            if (!generationFence.accepts(generation)) return@start
            listening = false
            processing = false
            result.onSuccess { text ->
                val nextDraft = applyOfflineAsrDraftResult(
                    draftAtStart = draftAtStart,
                    currentDraft = latestDraft.value,
                    recognizedText = text,
                )
                if (nextDraft == null) {
                    error = "草稿已更改，未应用离线转写结果"
                } else if (nextDraft != latestDraft.value) {
                    onDraftChange(nextDraft)
                    error = null
                }
            }.onFailure { failure ->
                error = failure.message ?: "离线语音识别失败"
            }
        }.onSuccess {
            listening = true
            processing = false
            error = null
        }.onFailure { failure ->
            if (!generationFence.accepts(generation)) return@onFailure
            listening = false
            processing = false
            error = failure.message ?: "离线语音识别无法启动"
        }
    }

    val microphonePermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) startOfflineTranscription() else error = "请允许麦克风权限后再使用离线语音输入"
    }

    DisposableEffect(transcriber) {
        onDispose {
            generationFence.invalidate()
            transcriber.cancel()
        }
    }

    DisposableEffect(enabled, transcriber) {
        if (!enabled) {
            generationFence.invalidate()
            transcriber.cancel()
            listening = false
            processing = false
        }
        onDispose { }
    }

    Column {
        Button(
            onClick = {
                if (listening) {
                    if (transcriber.stop()) {
                        listening = false
                        processing = true
                    }
                } else if (
                    !processing &&
                        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                ) {
                    startOfflineTranscription()
                } else {
                    microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                }
            },
            enabled = enabled && available && !processing,
            modifier = Modifier.size(48.dp).testTag(testTag),
            colors = ButtonDefaults.buttonColors(
                containerColor = if (listening) Color(0xFFE34B5F) else Color.Black,
                contentColor = Color.White,
            ),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
        ) {
            Text(if (processing) "…" else if (listening) "■" else "🎙", fontSize = 17.sp)
        }
        error?.let {
            Text(it, color = Color(0xFFD14343), fontSize = 9.sp, modifier = Modifier.padding(top = 2.dp))
        }
    }
}
