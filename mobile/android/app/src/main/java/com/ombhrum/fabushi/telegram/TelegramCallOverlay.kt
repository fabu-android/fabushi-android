package com.ombhrum.fabushi.telegram

import android.annotation.SuppressLint
import android.content.Context
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.sin

// ============================================================================
// 1. 1:1 & Group VoIP / Video Call + Screen Share + 333-Emoji SAS Overlay
// ============================================================================

@Composable
fun TelegramCallOverlay(
    callState: TelegramCallUiState,
    onToggleMute: () -> Unit,
    onToggleVideo: () -> Unit,
    onToggleScreenShare: () -> Unit,
    onToggleSpeaker: () -> Unit,
    onMinimize: (Boolean) -> Unit,
    onEndCall: () -> Unit,
) {
    if (callState.phase == CallPhase.IDLE) return

    val durationText = "%02d:%02d".format(callState.durationSeconds / 60, callState.durationSeconds % 60)

    if (callState.isMinimized) {
        // Floating Telegram Green Call Banner at Top
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(Color(0xFF28A745))
                .statusBarsPadding()
                .clickable { onMinimize(false) }
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TelegramIcon(TelegramGlyph.PHONE_CALL, tint = Color.White, size = 18.dp)
                Spacer(Modifier.width(10.dp))
                Text(
                    text = "${callState.peerTitle} · $durationText (点击返回通话)",
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            Text(
                text = callState.emojiFingerprint.joinToString(""),
                fontSize = 14.sp,
            )
        }
        return
    }

    BackHandler { onMinimize(true) }
    var showEmojiVerifyDialog by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(Color(0xFF14283D), Color(0xFF0E1B2A), Color(0xFF09111B)),
                ),
            )
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Top Bar: Minimize + 333-Emoji 4-Glyph E2EE Verification Pill
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = { onMinimize(true) }) {
                    Text("▾ 最小化悬浮", color = Color.White.copy(alpha = 0.85f))
                }
                Surface(
                    shape = RoundedCornerShape(18.dp),
                    color = Color.White.copy(alpha = 0.14f),
                    modifier = Modifier
                        .clickable { showEmojiVerifyDialog = true }
                        .testTag("telegram-call-emoji-pill"),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = Color(0xFF4ECC5E), size = 14.dp)
                        Spacer(Modifier.width(6.dp))
                        Text(callState.emojiFingerprint.joinToString(" "), fontSize = 17.sp)
                    }
                }
            }

            Spacer(Modifier.height(24.dp))

            TelegramAvatar(
                idSeed = callState.conversationId,
                title = callState.peerTitle,
                size = if (callState.isGroupCall || callState.isVideoCall) 78.dp else 112.dp,
                online = true,
            )
            Spacer(Modifier.height(14.dp))
            Text(callState.peerTitle, color = Color.White, fontSize = 23.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text(
                text = when (callState.phase) {
                    CallPhase.OUTGOING_RINGING -> "正在呼叫并协商端到端密钥…"
                    CallPhase.CONNECTING -> "正在连接加密媒体流…"
                    CallPhase.ACTIVE -> "$durationText · 端到端加密高清${if (callState.isGroupCall) "群组语音/视频" else "通话"}"
                    else -> "通话已结束"
                },
                color = Color(0xFF9EC3E6),
                fontSize = 14.sp,
            )

            Spacer(Modifier.height(20.dp))

            // Video / Screen Share / Group Call Participant Grid
            if (callState.isGroupCall || callState.isVideoCall || callState.isScreenSharing) {
                if (callState.isScreenSharing) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(128.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .background(Color(0xFF1E3A56))
                            .border(1.dp, Color(0xFF64B5EF), RoundedCornerShape(16.dp))
                            .padding(14.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            TelegramIcon(TelegramGlyph.SCREEN_SHARE, tint = Color(0xFF64B5EF), size = 30.dp)
                            Spacer(Modifier.height(6.dp))
                            Text("正在共享本机屏幕画面 (60fps 低延迟)", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                    Spacer(Modifier.height(12.dp))
                }
                LazyVerticalGrid(
                    columns = GridCells.Fixed(2),
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    items(callState.participants, key = { it.id }) { p ->
                        Box(
                            modifier = Modifier
                                .height(110.dp)
                                .clip(RoundedCornerShape(16.dp))
                                .background(Color.White.copy(alpha = 0.08f))
                                .border(
                                    width = if (p.speaking) 1.8.dp else 1.dp,
                                    color = if (p.speaking) Color(0xFF4ECC5E) else Color.White.copy(alpha = 0.14f),
                                    shape = RoundedCornerShape(16.dp),
                                )
                                .padding(12.dp),
                        ) {
                            Column(
                                modifier = Modifier.fillMaxSize(),
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.Center,
                            ) {
                                TelegramAvatar(idSeed = p.id, title = p.displayName, size = 42.dp)
                                Spacer(Modifier.height(8.dp))
                                Text(p.displayName, color = Color.White, fontSize = 13.sp, maxLines = 1)
                            }
                        }
                    }
                }
            } else {
                Spacer(Modifier.weight(1f))
            }

            // Bottom Call Control Buttons
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 18.dp),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CallControlCircle(
                    glyph = TelegramGlyph.MUTE,
                    label = if (callState.isMuted) "已静音" else "静音",
                    active = callState.isMuted,
                    onClick = onToggleMute,
                )
                CallControlCircle(
                    glyph = TelegramGlyph.VIDEO_CALL,
                    label = if (callState.isVideoCall) "关镜头" else "开视频",
                    active = callState.isVideoCall,
                    onClick = onToggleVideo,
                )
                CallControlCircle(
                    glyph = TelegramGlyph.SCREEN_SHARE,
                    label = "共享屏幕",
                    active = callState.isScreenSharing,
                    onClick = onToggleScreenShare,
                )
                CallControlCircle(
                    glyph = TelegramGlyph.TALK_WAVE,
                    label = if (callState.isSpeakerOn) "免提开" else "听筒",
                    active = callState.isSpeakerOn,
                    onClick = onToggleSpeaker,
                )
                // Red End Call Button
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Box(
                        modifier = Modifier
                            .size(58.dp)
                            .clip(CircleShape)
                            .background(Color(0xFFE53935))
                            .clickable(onClick = onEndCall)
                            .testTag("telegram-end-call-button"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.PHONE_CALL, tint = Color.White, size = 26.dp)
                    }
                    Spacer(Modifier.height(6.dp))
                    Text("挂断", color = Color.White.copy(alpha = 0.85f), fontSize = 12.sp)
                }
            }
        }
    }

    if (showEmojiVerifyDialog) {
        AlertDialog(
            onDismissRequest = { showEmojiVerifyDialog = false },
            title = { Text("🔐 333-Emoji 端到端通话校验") },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                    Text(callState.emojiFingerprint.joinToString("  "), fontSize = 36.sp)
                    Spacer(Modifier.height(12.dp))
                    Text(
                        "如果 ${callState.peerTitle} 的屏幕上也显示完全相同的 4 个表情符号，则代表本次通话密钥 100% 端到端加密，无任何中间人攻击。",
                        fontSize = 14.sp,
                    )
                }
            },
            confirmButton = {
                Button(onClick = { showEmojiVerifyDialog = false }) { Text("安全一致") }
            },
        )
    }
}

@Composable
private fun CallControlCircle(
    glyph: TelegramGlyph,
    label: String,
    active: Boolean,
    onClick: () -> Unit,
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            modifier = Modifier
                .size(52.dp)
                .clip(CircleShape)
                .background(if (active) Color.White else Color.White.copy(alpha = 0.16f))
                .clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph, tint = if (active) Color(0xFF14283D) else Color.White, size = 22.dp)
        }
        Spacer(Modifier.height(6.dp))
        Text(label, color = Color.White.copy(alpha = 0.82f), fontSize = 11.sp)
    }
}

// ============================================================================
// 2. Full-Duplex <60ms VAD Talk Mode Orb Overlay
// ============================================================================

@Composable
fun TelegramTalkModeOrbOverlay(
    orbState: TalkModeOrbState,
    onBargeIn: () -> Unit,
    onSendUtterance: (String) -> Unit,
    onClose: () -> Unit,
) {
    if (!orbState.visible) return
    BackHandler(onBack = onClose)

    val transition = rememberInfiniteTransition(label = "talk-orb")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = (Math.PI * 2).toFloat(),
        animationSpec = infiniteRepeatable(tween(2200), RepeatMode.Restart),
        label = "orb-phase",
    )
    var manualPrompt by remember { mutableStateOf("") }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xF20B131E))
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(22.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Surface(shape = RoundedCornerShape(14.dp), color = Color(0xFF00BFA5).copy(alpha = 0.20f)) {
                    Text(
                        text = "⚡ 全双工端侧 VAD · ${orbState.vadLatencyMs}ms (<60ms)",
                        color = Color(0xFF4EF2D2),
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                    )
                }
                TextButton(onClick = onClose) {
                    Text("关闭", color = Color.White)
                }
            }

            Spacer(Modifier.height(34.dp))

            // Animated Multi-Ring Fluid Orb
            Box(
                modifier = Modifier
                    .size(220.dp)
                    .clickable(onClick = onBargeIn),
                contentAlignment = Alignment.Center,
            ) {
                Canvas(modifier = Modifier.fillMaxSize()) {
                    val center = Offset(size.width / 2f, size.height / 2f)
                    val baseR = size.minDimension * 0.29f
                    for (ring in 3 downTo 1) {
                        val pulse = 1f + 0.09f * ring * sin(phase + ring * 0.8f)
                        drawCircle(
                            brush = Brush.radialGradient(
                                listOf(Color(0xFF2AABEE).copy(alpha = 0.34f / ring), Color(0xFF00E5FF).copy(alpha = 0.04f)),
                                center = center,
                                radius = baseR * pulse * (1f + ring * 0.22f),
                            ),
                            radius = baseR * pulse * (1f + ring * 0.22f),
                            center = center,
                        )
                    }
                    drawCircle(
                        brush = Brush.linearGradient(listOf(Color(0xFF2AABEE), Color(0xFF00BFA5), Color(0xFF7C4DFF))),
                        radius = baseR * (1f + 0.05f * sin(phase * 2f)),
                        center = center,
                    )
                }
                TelegramIcon(TelegramGlyph.TALK_WAVE, tint = Color.White, size = 42.dp)
            }

            Spacer(Modifier.height(16.dp))
            Text(orbState.phase.label, color = Color(0xFF7FD3FF), fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text("轻触光球即可模拟语音开口 <60ms 极速打断 (Barge-In)", color = Color.White.copy(alpha = 0.58f), fontSize = 12.sp)

            Spacer(Modifier.height(20.dp))
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = Color.White.copy(alpha = 0.08f),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (orbState.userTranscript.isNotBlank()) {
                        Text("🗣 您说：${orbState.userTranscript}", color = Color(0xFFA7C4E0), fontSize = 13.sp)
                    }
                    Text(orbState.assistantReply, color = Color.White, fontSize = 15.sp, lineHeight = 22.sp)
                }
            }

            Spacer(Modifier.weight(1f))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                listOf("总结未读消息", "让 Mika 打开设置", "背诵般若心经").forEach { chip ->
                    Surface(
                        shape = RoundedCornerShape(14.dp),
                        color = Color.White.copy(alpha = 0.12f),
                        modifier = Modifier.clickable { onSendUtterance(chip) },
                    ) {
                        Text(chip, color = Color.White, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp))
                    }
                }
            }
            Spacer(Modifier.height(10.dp))
            Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = manualPrompt,
                    onValueChange = { manualPrompt = it },
                    placeholder = { Text("输入测试语音语句…", color = Color.Gray) },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Button(onClick = {
                    onSendUtterance(manualPrompt)
                    manualPrompt = ""
                }) { Text("说话") }
            }
        }
    }
}

// ============================================================================
// 3. E2EE Secret Chat 8x8 Identicon + 4-Emoji Fingerprint Dialog
// ============================================================================

@Composable
fun TelegramSecretChatDialog(
    chat: TelegramChatItem?,
    onVerify: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    if (chat == null) return
    val fp = remember(chat.secretFingerprintSeed) {
        CallsEmojiFingerprint.deriveSecretChatIdenticon(chat.secretFingerprintSeed)
    }
    val cellPalette = listOf(
        Color(0xFFE3F2FD),
        Color(0xFF64B5F6),
        Color(0xFF1E88E5),
        Color(0xFF0D47A1),
    )

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("🔐 端到端加密密钥指纹核验") },
        text = {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                // 8x8 Cryptographic Identicon Canvas
                Canvas(
                    modifier = Modifier
                        .size(144.dp)
                        .clip(RoundedCornerShape(14.dp))
                        .border(1.dp, Color.Gray.copy(alpha = 0.3f), RoundedCornerShape(14.dp)),
                ) {
                    val cellW = size.width / 8f
                    val cellH = size.height / 8f
                    for (r in 0 until 8) {
                        for (c in 0 until 8) {
                            val v = fp.cells8x8[r * 8 + c].coerceIn(0, 3)
                            drawRect(
                                color = cellPalette[v],
                                topLeft = Offset(c * cellW, r * cellH),
                                size = Size(cellW + 0.5f, cellH + 0.5f),
                            )
                        }
                    }
                }
                Text(fp.emojis4.joinToString("   "), fontSize = 28.sp)
                Text(
                    text = fp.hexBlocks,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    lineHeight = 16.sp,
                )
                Text(
                    text = fp.protocolSummary,
                    fontSize = 12.sp,
                    color = Color(0xFF2FA84F),
                    fontWeight = FontWeight.SemiBold,
                )
            }
        },
        confirmButton = {
            Button(onClick = { onVerify(chat.id) }) {
                Text(if (chat.secretVerified) "✓ 已核验一致" else "标记为已核验")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("关闭") }
        },
    )
}

// ============================================================================
// 4. Local 4-Digit Passcode & Biometric Lock Screen
// ============================================================================

@Composable
fun TelegramPasscodeScreen(
    errorMessage: String?,
    biometricEnabled: Boolean,
    onVerifyPin: (String) -> Boolean,
    onBiometricSuccess: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val context = LocalContext.current
    var enteredPin by remember { mutableStateOf("") }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Brush.verticalGradient(listOf(palette.drawerHeaderStart, palette.drawerHeaderEnd)))
            .statusBarsPadding()
            .navigationBarsPadding()
            .testTag("telegram-passcode-screen"),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.padding(28.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(68.dp)
                    .clip(CircleShape)
                    .background(Color.White.copy(alpha = 0.16f)),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = Color.White, size = 32.dp)
            }
            Spacer(Modifier.height(16.dp))
            Text("请输入 Telegram 本地密码", color = Color.White, fontSize = 19.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = errorMessage ?: "保护您的私密聊天、TON 钱包与 Mika 权限",
                color = if (errorMessage != null) Color(0xFFFFB4AB) else Color.White.copy(alpha = 0.78f),
                fontSize = 13.sp,
            )

            Spacer(Modifier.height(24.dp))

            // 4 PIN Dots
            Row(horizontalArrangement = Arrangement.spacedBy(18.dp)) {
                repeat(4) { index ->
                    val filled = index < enteredPin.length
                    Box(
                        modifier = Modifier
                            .size(16.dp)
                            .clip(CircleShape)
                            .background(if (filled) Color.White else Color.Transparent)
                            .border(2.dp, Color.White, CircleShape),
                    )
                }
            }

            Spacer(Modifier.height(34.dp))

            // 3x4 Numeric Keypad
            val rows = listOf(
                listOf("1", "2", "3"),
                listOf("4", "5", "6"),
                listOf("7", "8", "9"),
                listOf("指纹", "0", "⌫"),
            )
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                rows.forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                        row.forEach { key ->
                            Box(
                                modifier = Modifier
                                    .size(70.dp)
                                    .clip(CircleShape)
                                    .background(Color.White.copy(alpha = 0.14f))
                                    .clickable {
                                        when (key) {
                                            "⌫" -> if (enteredPin.isNotEmpty()) enteredPin = enteredPin.dropLast(1)
                                            "指纹" -> if (biometricEnabled) promptNativeBiometric(context, onBiometricSuccess)
                                            else -> if (enteredPin.length < 4) {
                                                val next = enteredPin + key
                                                enteredPin = next
                                                if (next.length == 4) {
                                                    if (!onVerifyPin(next)) enteredPin = ""
                                                }
                                            }
                                        }
                                    },
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = key,
                                    color = Color.White,
                                    fontSize = if (key == "指纹") 14.sp else 24.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@SuppressLint("MissingPermission")
private fun promptNativeBiometric(context: Context, onSuccess: () -> Unit) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        runCatching {
            val prompt = BiometricPrompt.Builder(context)
                .setTitle("解锁 GPUI-Telegram")
                .setSubtitle("验证指纹或面容以解锁加密工作台")
                .setNegativeButton("使用数字密码", context.mainExecutor) { _, _ -> }
                .build()
            prompt.authenticate(
                CancellationSignal(),
                context.mainExecutor,
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult?) {
                        onSuccess()
                    }
                },
            )
        }
    }
}
