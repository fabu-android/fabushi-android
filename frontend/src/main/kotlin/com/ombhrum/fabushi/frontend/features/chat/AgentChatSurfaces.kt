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


@Composable
internal fun MobileAgentChat(
    state: MarketplaceUiState,
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
    onOpenGeneratedMiniApp: (MobileChatMessage) -> Unit,
    onClose: () -> Unit,
) {
    Scaffold(containerColor = homeBackground, modifier = Modifier.fillMaxSize().testTag(TestTags.MahayanaAgentChat)) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("‹", color = homePrimaryText, fontSize = 34.sp, modifier = Modifier.clickable(onClick = onClose).padding(8.dp))
                Column(Modifier.weight(1f).padding(start = 4.dp)) {
                    Text("大乘助手", color = homePrimaryText, fontWeight = FontWeight.SemiBold, fontSize = 20.sp)
                    Text(if (state.chatBusy) "正在工作" else "Mahayana 多步骤智能体", color = if (state.chatBusy) homeAccent else homeSecondaryText, fontSize = 12.sp)
                }
            }
            LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 12.dp), verticalArrangement = Arrangement.spacedBy(9.dp)) {
                if (state.chatMessages.isEmpty()) item {
                    Column(Modifier.fillMaxWidth().padding(top = 92.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(9.dp)) {
                        Box(Modifier.size(68.dp).background(homeSurface, CircleShape), contentAlignment = Alignment.Center) { Text("✦", color = homeAccent, fontSize = 36.sp, fontWeight = FontWeight.Bold) }
                        Text("大乘助手", color = homePrimaryText, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                        Text("真实的模型路由、工具调用和每一步工作会逐条显示在这里。", color = homeSecondaryText, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                    }
                }
                items(state.chatMessages, key = { it.id }) { entry -> MobileAgentChatEntry(entry, onOpenGeneratedMiniApp) }
            }
            Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = state.chatDraft, onValueChange = onDraftChange, modifier = Modifier.weight(1f), enabled = !state.chatBusy, placeholder = { Text("消息大乘助手") }, maxLines = 5)
                if (!state.chatBusy) {
                    OfflineAsrDraftControl(
                        currentDraft = state.chatDraft,
                        enabled = true,
                        onDraftChange = onDraftChange,
                        sessionKey = "mahayana-assistant",
                        testTag = "mahayana-offline-asr",
                    )
                }
                if (state.chatBusy) {
                    Button(onClick = onStop, modifier = Modifier.size(52.dp).testTag(TestTags.MahayanaStop), contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp), colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFE44F61))) { Text("■", color = Color.White) }
                } else {
                    Button(onClick = onSend, enabled = state.chatDraft.trim().isNotEmpty(), modifier = Modifier.size(52.dp).testTag(TestTags.MahayanaSend), contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp), colors = ButtonDefaults.buttonColors(containerColor = homeAccent, contentColor = Color.Black)) { Text("↑", fontSize = 22.sp, fontWeight = FontWeight.Bold) }
                }
            }
        }
    }
}

@Composable
internal fun MobileAgentChatEntry(entry: MobileChatMessage, onOpenGeneratedMiniApp: (MobileChatMessage) -> Unit) {
    when {
        entry.kind == MobileChatEntryKind.THINKING -> Row(Modifier.fillMaxWidth().background(homeSurface, RoundedCornerShape(13.dp)).border(1.dp, Color(0xFF6F5BC6), RoundedCornerShape(13.dp)).padding(10.dp).testTag(TestTags.MahayanaThinking), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
            Box(Modifier.size(30.dp).background(homeBackground, CircleShape), contentAlignment = Alignment.Center) { Text("✦", color = homeAccent, fontSize = 18.sp) }
            Column(Modifier.weight(1f)) { Text(entry.actionTitle ?: "正在思考", color = homePrimaryText, fontWeight = FontWeight.SemiBold); Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) { CircularProgressIndicator(color = homeAccent, modifier = Modifier.size(13.dp), strokeWidth = 2.dp); Text("Mahayana 正在处理…", color = homeSecondaryText, fontSize = 12.sp) } }
        }
        entry.kind == MobileChatEntryKind.ACTION -> Row(Modifier.fillMaxWidth().background(homeSurface, RoundedCornerShape(11.dp)).padding(horizontal = 10.dp, vertical = 8.dp).testTag(TestTags.MahayanaStep), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(25.dp).background(homeBackground, CircleShape), contentAlignment = Alignment.Center) { Text("✦", color = homeAccent, fontSize = 15.sp) }
            Column(Modifier.weight(1f)) { Text(entry.actionTitle ?: "助手动作", color = homePrimaryText, fontSize = 13.sp, fontWeight = FontWeight.SemiBold); entry.actionDetail?.let { if (it.isNotBlank()) Text(it, color = homeSecondaryText, fontSize = 11.sp, maxLines = 2, overflow = TextOverflow.Ellipsis) } }
            Text(if (entry.actionStatus == "failed") "失败" else if (entry.actionStatus == "running") "进行中" else "完成", color = if (entry.actionStatus == "failed") Color(0xFFFF6B6B) else if (entry.actionStatus == "running") homeAccent else Color(0xFF65D6A0), fontSize = 11.sp)
        }
        entry.kind == MobileChatEntryKind.MINI_APP -> Column(Modifier.fillMaxWidth().background(homeSurface, RoundedCornerShape(14.dp)).border(1.dp, Color(0xFF6F5BC6), RoundedCornerShape(14.dp)).padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) { Text("▦", color = homeAccent, fontSize = 20.sp); Column { Text(entry.miniAppName ?: "生成的小程序", color = homePrimaryText, fontWeight = FontWeight.SemiBold); Text(entry.miniAppDescription ?: "可直接运行的 Fabushi 小程序产物", color = homeSecondaryText, fontSize = 11.sp) } }
            Button(onClick = { onOpenGeneratedMiniApp(entry) }, modifier = Modifier.testTag("mahayana-generated-miniapp-open"), colors = ButtonDefaults.buttonColors(containerColor = homeAccent, contentColor = Color.Black)) { Text("打开小程序", fontWeight = FontWeight.Bold) }
        }
        entry.role == MobileChatRole.USER -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) { Text(entry.text, color = Color.White, modifier = Modifier.background(Color.Black, RoundedCornerShape(16.dp)).padding(horizontal = 13.dp, vertical = 10.dp)) }
        else -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Start, verticalAlignment = Alignment.Top) { Text("✦", color = homeAccent, modifier = Modifier.padding(7.dp)); Text(entry.text, color = homePrimaryText, modifier = Modifier.background(homeSurface, RoundedCornerShape(16.dp)).padding(horizontal = 13.dp, vertical = 10.dp)) }
    }
}

