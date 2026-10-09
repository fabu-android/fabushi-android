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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat

@Composable
internal fun OfflineAsrDraftControl(
    currentDraft: String,
    enabled: Boolean,
    onDraftChange: (String) -> Unit,
    sessionKey: String,
    testTag: String,
) {
    val context = LocalContext.current
    val transcriber = remember(sessionKey) { NativeOfflineSpeechTranscriber(context.applicationContext) }
    val available = remember { transcriber.isAvailable() }
    var listening by remember { mutableStateOf(false) }
    var error by remember(available) { mutableStateOf(if (available) null else "设备离线语音识别不可用") }

    fun startOfflineTranscription() {
        if (!available) {
            error = "设备离线语音识别不可用"
            return
        }
        transcriber.start { result ->
            listening = false
            result.onSuccess { text ->
                val clean = text.trim()
                if (clean.isNotEmpty()) {
                    val prefix = currentDraft.trimEnd()
                    onDraftChange(if (prefix.isEmpty()) clean else "$prefix $clean")
                    error = null
                }
            }.onFailure { failure ->
                error = failure.message ?: "离线语音识别失败"
            }
        }.onSuccess {
            listening = true
            error = null
        }.onFailure { failure ->
            listening = false
            error = failure.message ?: "离线语音识别无法启动"
        }
    }

    val microphonePermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) startOfflineTranscription() else error = "请允许麦克风权限后再使用离线语音输入"
    }

    DisposableEffect(transcriber) {
        onDispose { transcriber.cancel() }
    }

    Column {
        Button(
            onClick = {
                if (listening) {
                    transcriber.cancel()
                    listening = false
                } else if (
                    ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                ) {
                    startOfflineTranscription()
                } else {
                    microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                }
            },
            enabled = enabled && available,
            modifier = Modifier.size(48.dp).testTag(testTag),
            colors = ButtonDefaults.buttonColors(
                containerColor = if (listening) Color(0xFFE34B5F) else Color.Black,
                contentColor = Color.White,
            ),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
        ) {
            Text(if (listening) "■" else "🎙", fontSize = 17.sp)
        }
        error?.let {
            Text(it, color = Color(0xFFD14343), fontSize = 9.sp, modifier = Modifier.padding(top = 2.dp))
        }
    }
}
