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
internal fun MobileOnboarding(step: Int, onContinue: () -> Unit, onSkip: () -> Unit) {
    Box(Modifier.fillMaxSize().background(homeBackground).testTag(TestTags.MobileOnboarding), contentAlignment = Alignment.Center) {
        Column(Modifier.fillMaxWidth().padding(28.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Spacer(Modifier.height(42.dp))
            Box(Modifier.size(92.dp).background(homeSurface, CircleShape).border(1.dp, homeBorder, CircleShape), contentAlignment = Alignment.Center) {
                Text("✦", color = homeAccent, fontSize = 48.sp, fontWeight = FontWeight.Bold)
            }
            Text("欢迎来到法布施", color = homePrimaryText, fontSize = 28.sp, fontWeight = FontWeight.Bold)
            Text("统一的聊天、插件和 Mahayana 多步骤智能体工作台。每一步工作都会像消息一样实时出现。", color = homeSecondaryText, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                repeat(3) { index -> Box(Modifier.size(width = 24.dp, height = 5.dp).background(if (index <= step) homeAccent else homeBorder, RoundedCornerShape(8.dp))) }
            }
            Spacer(Modifier.weight(1f))
            Button(onClick = onContinue, modifier = Modifier.fillMaxWidth().testTag(TestTags.MobileOnboardingContinue), colors = ButtonDefaults.buttonColors(containerColor = homeAccent, contentColor = Color.Black)) {
                Text(if (step >= 2) "开始使用" else "继续")
            }
            TextButton(onClick = onSkip, modifier = Modifier.testTag(TestTags.MobileOnboardingSkip)) { Text("跳过介绍", color = homeSecondaryText) }
        }
    }
}

@Composable
internal fun MobileAuthLoading() {
    Box(Modifier.fillMaxSize().background(Color(0xFFFAFAF7)), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Box(Modifier.size(70.dp).background(Color(0xFF202020), CircleShape), contentAlignment = Alignment.Center) {
                Text("✦", color = Color(0xFFFF9D22), fontSize = 38.sp, fontWeight = FontWeight.Bold)
            }
            CircularProgressIndicator(color = Color.Black, modifier = Modifier.size(28.dp))
            Text("正在连接 Fabushi…", color = Color.Black.copy(alpha = 0.58f))
        }
    }
}

@Composable
internal fun LoginBlob(
    color: Color,
    width: Dp,
    height: Dp,
    rotation: Float,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier
            .size(width = width, height = height)
            .rotate(rotation)
            .background(color, RoundedCornerShape(percent = 42)),
        contentAlignment = Alignment.Center,
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.size(width = 11.dp, height = 25.dp).background(Color.White, RoundedCornerShape(99.dp)))
            Box(Modifier.size(width = 11.dp, height = 25.dp).background(Color.White, RoundedCornerShape(99.dp)))
        }
    }
}

@Composable
internal fun MobileLogin(
    state: MarketplaceUiState,
    onBegin: () -> Unit,
    onReopen: () -> Unit,
    onCancel: () -> Unit,
) {
    Box(Modifier.fillMaxSize().background(Color(0xFFFAFAF7)).testTag(TestTags.MobileLogin)) {
        LoginBlob(Color(0xFFFF920C), 92.dp, 92.dp, -8f, Modifier.align(Alignment.TopStart).offset(x = 66.dp, y = 100.dp))
        LoginBlob(Color(0xFF8A4CFF), 58.dp, 66.dp, 12f, Modifier.align(Alignment.TopEnd).offset(x = (-88).dp, y = 116.dp))
        LoginBlob(Color(0xFF00C978), 86.dp, 70.dp, 6f, Modifier.align(Alignment.CenterEnd).offset(x = 42.dp, y = (-120).dp))
        LoginBlob(Color(0xFF1585F7), 86.dp, 66.dp, 7f, Modifier.align(Alignment.CenterStart).offset(x = (-36).dp, y = (-74).dp))
        LoginBlob(Color(0xFF00BCAE), 72.dp, 72.dp, -9f, Modifier.align(Alignment.BottomStart).offset(x = 52.dp, y = (-142).dp))
        LoginBlob(Color(0xFFFF253F), 94.dp, 84.dp, 8f, Modifier.align(Alignment.BottomCenter).offset(x = 34.dp, y = (-112).dp))
        LoginBlob(Color(0xFFA66C37), 64.dp, 64.dp, 17f, Modifier.align(Alignment.BottomEnd).offset(x = 22.dp, y = (-208).dp))

        Column(
            Modifier.fillMaxSize().navigationBarsPadding().padding(horizontal = 28.dp, vertical = 18.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Spacer(Modifier.weight(1f))
            Text("Fabushi", color = Color.Black, fontSize = 48.sp, fontWeight = FontWeight.Bold, letterSpacing = (-1.2).sp)
            Spacer(Modifier.height(14.dp))
            Text(
                "你的常驻智能体团队，持续完成工作。",
                color = Color.Black.copy(alpha = 0.42f),
                fontSize = 18.sp,
                fontWeight = FontWeight.SemiBold,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
            Spacer(Modifier.weight(1f))

            if (state.loginError != null) {
                Text(
                    "登录暂时不可用，请稍后重试。",
                    color = Color(0xFFD14343),
                    fontSize = 13.sp,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
            }

            if (state.browserLoginAttemptId != null) {
                Button(
                    onClick = onReopen,
                    modifier = Modifier.fillMaxWidth().height(58.dp).testTag(TestTags.MobileLoginReopen),
                    shape = RoundedCornerShape(30.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF171717), contentColor = Color.White),
                ) { Text("继续登录", fontSize = 18.sp, fontWeight = FontWeight.Bold) }
                TextButton(onClick = onCancel, modifier = Modifier.testTag(TestTags.MobileLoginCancel)) {
                    Text("取消登录", color = Color.Black.copy(alpha = 0.46f), fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                }
            } else {
                Button(
                    onClick = onBegin,
                    enabled = !state.loginBusy,
                    modifier = Modifier.fillMaxWidth().height(58.dp).testTag(TestTags.MobileLoginBrowser),
                    shape = RoundedCornerShape(30.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF171717), contentColor = Color.White),
                ) {
                    if (state.loginBusy) {
                        CircularProgressIndicator(color = Color.White, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    } else {
                        Text("登录", fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }
    }
}

