package com.ombhrum.fabushi

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay


internal fun shouldShowUpdateBanner(phase: AndroidUpdatePhase): Boolean = when (phase) {
    AndroidUpdatePhase.AVAILABLE,
    AndroidUpdatePhase.DOWNLOADING,
    AndroidUpdatePhase.WAITING_FOR_PERMISSION,
    AndroidUpdatePhase.INSTALLING,
    AndroidUpdatePhase.ERROR -> true
    else -> false
}

@Composable
internal fun UpdateBanner(
    state: AndroidUpdateUiState,
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp).testTag(TestTags.UpdateCard),
        colors = CardDefaults.cardColors(containerColor = homeSurface),
        shape = RoundedCornerShape(18.dp),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            val label = when (state.phase) {
                AndroidUpdatePhase.AVAILABLE -> "发现新版本 ${state.availableVersion ?: ""}"
                AndroidUpdatePhase.DOWNLOADING -> "正在下载更新 ${state.progressPercent ?: 0}%"
                AndroidUpdatePhase.WAITING_FOR_PERMISSION -> "更新已下载，等待安装权限"
                AndroidUpdatePhase.INSTALLING -> "系统安装器已打开"
                AndroidUpdatePhase.ERROR -> "更新失败"
                else -> "应用更新"
            }
            Text(label, color = homePrimaryText, fontWeight = FontWeight.SemiBold)
            state.message?.takeIf { it.isNotBlank() }?.let {
                Text(it, color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
            }
            if (state.phase == AndroidUpdatePhase.DOWNLOADING) {
                val progress = (state.progressPercent ?: 0).coerceIn(0, 100) / 100f
                LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
            }
            when (state.phase) {
                AndroidUpdatePhase.AVAILABLE,
                AndroidUpdatePhase.WAITING_FOR_PERMISSION,
                AndroidUpdatePhase.INSTALLING -> Button(
                    onClick = onInstallUpdate,
                    modifier = Modifier.fillMaxWidth().testTag(TestTags.UpdateAction),
                    colors = ButtonDefaults.buttonColors(containerColor = homeAccent, contentColor = Color.Black),
                ) {
                    Text(if (state.phase == AndroidUpdatePhase.AVAILABLE) "下载并安装" else "继续安装")
                }
                AndroidUpdatePhase.ERROR -> OutlinedButton(
                    onClick = onCheckUpdate,
                    modifier = Modifier.fillMaxWidth().testTag(TestTags.UpdateAction),
                ) { Text("重新检查", color = homePrimaryText) }
                else -> Unit
            }
        }
    }
}

